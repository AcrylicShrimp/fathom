use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::RETRY_AFTER;
use serde_json::{Value, json};

use crate::agent::retry::RetryPolicy;
use crate::agent::tool_registry::ToolRegistry;
use crate::agent::types::{StreamNote, ToolInvocation};

const RESPONSES_API_URL: &str = "https://api.openai.com/v1/responses";
const DEFAULT_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_REASONING_EFFORT: &str = "extra_high";
const FALLBACK_REASONING_EFFORT: &str = "high";
const DEFAULT_TIMEOUT_SECS: u64 = 45;

#[derive(Debug, Clone)]
struct PartialToolCall {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenAiStreamOutcome {
    pub(crate) tool_call_count: usize,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct OpenAiClient {
    http: reqwest::Client,
    api_key: Option<String>,
    retry_policy: RetryPolicy,
}

impl OpenAiClient {
    pub(crate) fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|error| format!("failed to construct reqwest client: {error}"))?;
        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(Self {
            http,
            api_key,
            retry_policy: RetryPolicy::conservative(),
        })
    }

    pub(crate) async fn stream_tool_calls<FS, FT>(
        &self,
        prompt: &str,
        tool_registry: &ToolRegistry,
        mut on_stream: FS,
        mut on_tool: FT,
    ) -> Result<OpenAiStreamOutcome, String>
    where
        FS: FnMut(StreamNote),
        FT: FnMut(ToolInvocation),
    {
        let Some(api_key) = self.api_key.as_deref() else {
            return Err("OPENAI_API_KEY is required but not configured".to_string());
        };

        let mut attempts = 0usize;
        let mut reasoning_effort = DEFAULT_REASONING_EFFORT;
        let max_retries = self.retry_policy.max_retries();
        let mut last_error = String::new();

        while attempts <= max_retries {
            on_stream(StreamNote {
                phase: "openai.request.start".to_string(),
                detail: format!("attempt={} effort={reasoning_effort}", attempts + 1),
            });

            let body = json!({
                "model": DEFAULT_MODEL,
                "stream": true,
                "input": prompt,
                "reasoning": { "effort": reasoning_effort },
                "tools": tool_registry.openai_tool_definitions(),
                "tool_choice": "required"
            });

            let response = self
                .http
                .post(RESPONSES_API_URL)
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await;

            match response {
                Ok(response) if response.status().is_success() => {
                    let result = self
                        .parse_stream(response, tool_registry, &mut on_stream, &mut on_tool)
                        .await;
                    match result {
                        Ok(outcome) => return Ok(outcome),
                        Err(error) => {
                            last_error = error;
                            if attempts >= max_retries {
                                break;
                            }
                            let delay = self.retry_policy.compute_delay(attempts, None);
                            on_stream(StreamNote {
                                phase: "openai.request.retry".to_string(),
                                detail: format!(
                                    "stream_parse_error; waiting {}ms before retry",
                                    delay.as_millis()
                                ),
                            });
                            tokio::time::sleep(delay).await;
                            attempts += 1;
                        }
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    let retry_after = parse_retry_after(response.headers());
                    let text = response.text().await.unwrap_or_default();
                    last_error = format!(
                        "OpenAI request failed: status={} body={}",
                        status.as_u16(),
                        truncate_for_log(&text)
                    );

                    let invalid_reasoning = status.as_u16() == 400
                        && reasoning_effort == DEFAULT_REASONING_EFFORT
                        && text.contains("reasoning")
                        && text.contains("effort");
                    if invalid_reasoning {
                        on_stream(StreamNote {
                            phase: "openai.request.fallback".to_string(),
                            detail: format!(
                                "falling back reasoning effort to `{}`",
                                FALLBACK_REASONING_EFFORT
                            ),
                        });
                        reasoning_effort = FALLBACK_REASONING_EFFORT;
                        attempts += 1;
                        continue;
                    }

                    if should_retry_status(status.as_u16()) && attempts < max_retries {
                        let delay = self.retry_policy.compute_delay(attempts, retry_after);
                        on_stream(StreamNote {
                            phase: "openai.request.retry".to_string(),
                            detail: format!(
                                "status={} waiting {}ms before retry",
                                status.as_u16(),
                                delay.as_millis()
                            ),
                        });
                        tokio::time::sleep(delay).await;
                        attempts += 1;
                        continue;
                    }

                    break;
                }
                Err(error) => {
                    last_error = format!("OpenAI transport error: {error}");
                    if should_retry_transport(&error) && attempts < max_retries {
                        let delay = self.retry_policy.compute_delay(attempts, None);
                        on_stream(StreamNote {
                            phase: "openai.request.retry".to_string(),
                            detail: format!(
                                "transport_error waiting {}ms before retry",
                                delay.as_millis()
                            ),
                        });
                        tokio::time::sleep(delay).await;
                        attempts += 1;
                        continue;
                    }

                    break;
                }
            }
        }

        Err(last_error)
    }

    async fn parse_stream<FS, FT>(
        &self,
        response: reqwest::Response,
        tool_registry: &ToolRegistry,
        on_stream: &mut FS,
        on_tool: &mut FT,
    ) -> Result<OpenAiStreamOutcome, String>
    where
        FS: FnMut(StreamNote),
        FT: FnMut(ToolInvocation),
    {
        let mut stream = response.bytes_stream();
        let mut line_buffer = String::new();
        let mut partial_calls: HashMap<String, PartialToolCall> = HashMap::new();
        let mut dispatched_keys: HashSet<String> = HashSet::new();
        let mut tool_call_count = 0usize;
        let mut diagnostics = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            let bytes = chunk_result.map_err(|error| format!("stream chunk error: {error}"))?;
            line_buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(newline_index) = line_buffer.find('\n') {
                let mut line = line_buffer[..newline_index].to_string();
                line_buffer = line_buffer[newline_index + 1..].to_string();
                line = line.trim_end_matches('\r').to_string();

                if line.is_empty() || !line.starts_with("data:") {
                    continue;
                }

                let payload = line[5..].trim();
                if payload == "[DONE]" {
                    return Ok(OpenAiStreamOutcome {
                        tool_call_count,
                        diagnostics,
                    });
                }

                let value: Value = serde_json::from_str(payload)
                    .map_err(|error| format!("invalid stream json payload: {error}"))?;
                handle_stream_event(
                    value,
                    tool_registry,
                    on_stream,
                    on_tool,
                    &mut partial_calls,
                    &mut dispatched_keys,
                    &mut tool_call_count,
                    &mut diagnostics,
                )?;
            }
        }

        Ok(OpenAiStreamOutcome {
            tool_call_count,
            diagnostics,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_stream_event<FS, FT>(
    value: Value,
    tool_registry: &ToolRegistry,
    on_stream: &mut FS,
    on_tool: &mut FT,
    partial_calls: &mut HashMap<String, PartialToolCall>,
    dispatched_keys: &mut HashSet<String>,
    tool_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<(), String>
where
    FS: FnMut(StreamNote),
    FT: FnMut(ToolInvocation),
{
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    on_stream(StreamNote {
        phase: "openai.stream.event".to_string(),
        detail: event_type.to_string(),
    });

    match event_type {
        "response.output_item.added" | "response.output_item.done" => {
            if let Some(item) = value.get("item") {
                maybe_finalize_item(
                    item,
                    tool_registry,
                    on_tool,
                    partial_calls,
                    dispatched_keys,
                    tool_call_count,
                    diagnostics,
                )?;
            }
        }
        "response.function_call_arguments.delta" => {
            let key = extract_call_key(&value).unwrap_or_else(|| "unknown_call".to_string());
            let delta = value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let partial = partial_calls.entry(key.clone()).or_insert(PartialToolCall {
                call_id: value
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                arguments: String::new(),
            });
            partial.arguments.push_str(delta);
        }
        "response.function_call_arguments.done" => {
            let key = extract_call_key(&value).unwrap_or_else(|| "unknown_call".to_string());
            let arguments = value
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default();

            let partial = partial_calls.entry(key.clone()).or_insert(PartialToolCall {
                call_id: value
                    .get("call_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                arguments: String::new(),
            });
            partial.arguments = arguments.to_string();

            if let Some(name) = partial.name.clone() {
                maybe_dispatch_partial(
                    key,
                    name,
                    partial.arguments.clone(),
                    partial.call_id.clone(),
                    tool_registry,
                    on_tool,
                    dispatched_keys,
                    tool_call_count,
                    diagnostics,
                )?;
            }
        }
        "response.error" => {
            return Err(format!("OpenAI stream error payload: {value}"));
        }
        _ => {}
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_finalize_item<FT>(
    item: &Value,
    tool_registry: &ToolRegistry,
    on_tool: &mut FT,
    partial_calls: &mut HashMap<String, PartialToolCall>,
    dispatched_keys: &mut HashSet<String>,
    tool_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<(), String>
where
    FT: FnMut(ToolInvocation),
{
    if item.get("type").and_then(Value::as_str) != Some("function_call") {
        return Ok(());
    }

    let key = item
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| item.get("call_id").and_then(Value::as_str))
        .unwrap_or("unknown_call")
        .to_string();

    let entry = partial_calls.entry(key.clone()).or_insert(PartialToolCall {
        call_id: item
            .get("call_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        name: item.get("name").and_then(Value::as_str).map(str::to_string),
        arguments: String::new(),
    });

    if let Some(name) = item.get("name").and_then(Value::as_str) {
        entry.name = Some(name.to_string());
    }
    if let Some(arguments) = item.get("arguments").and_then(Value::as_str)
        && !arguments.is_empty()
    {
        entry.arguments = arguments.to_string();
    }

    if let Some(name) = entry.name.clone() {
        maybe_dispatch_partial(
            key,
            name,
            entry.arguments.clone(),
            entry.call_id.clone(),
            tool_registry,
            on_tool,
            dispatched_keys,
            tool_call_count,
            diagnostics,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_dispatch_partial<FT>(
    key: String,
    tool_name: String,
    arguments_raw: String,
    call_id: Option<String>,
    tool_registry: &ToolRegistry,
    on_tool: &mut FT,
    dispatched_keys: &mut HashSet<String>,
    tool_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<(), String>
where
    FT: FnMut(ToolInvocation),
{
    if arguments_raw.trim().is_empty() {
        return Ok(());
    }

    let dispatch_key = call_id.clone().unwrap_or_else(|| key.clone());
    if dispatched_keys.contains(&dispatch_key) {
        return Ok(());
    }

    let args_value: Value = serde_json::from_str(&arguments_raw).map_err(|error| {
        format!("invalid arguments JSON for tool `{tool_name}`: {error}; payload={arguments_raw}")
    })?;
    tool_registry
        .validate(&tool_name, &args_value)
        .map_err(|error| format!("tool validation failed: {error}"))?;

    let args_json = serde_json::to_string(&args_value)
        .map_err(|error| format!("failed to canonicalize tool args: {error}"))?;

    on_tool(ToolInvocation {
        tool_name: tool_name.clone(),
        args_json: args_json.clone(),
        call_id: call_id.clone(),
    });

    diagnostics.push(format!(
        "dispatched tool_call={} name={tool_name}",
        dispatch_key
    ));
    dispatched_keys.insert(dispatch_key);
    *tool_call_count += 1;

    Ok(())
}

fn extract_call_key(value: &Value) -> Option<String> {
    value
        .get("item_id")
        .and_then(Value::as_str)
        .or_else(|| value.get("call_id").and_then(Value::as_str))
        .map(str::to_string)
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    let seconds = raw.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

fn should_retry_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn should_retry_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

fn truncate_for_log(value: &str) -> String {
    const LIMIT: usize = 400;
    if value.len() <= LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..LIMIT])
    }
}
