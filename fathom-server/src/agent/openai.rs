use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::RETRY_AFTER;
use serde_json::{Value, json};

use crate::agent::retry::RetryPolicy;
use crate::agent::types::{
    ActionArgDeltaNote, ActionArgDoneNote, ActionInvocation, PromptMessage, StreamNote,
};
use crate::environment::EnvironmentRegistry;

const RESPONSES_API_URL: &str = "https://api.openai.com/v1/responses";
const DEFAULT_MODEL: &str = "gpt-5.2-codex";
const DEFAULT_REASONING_EFFORT: &str = "high";
const DEFAULT_TIMEOUT_SECS: u64 = 45;

#[derive(Debug, Clone)]
struct PartialActionCall {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenAiStreamOutcome {
    pub(crate) action_call_count: usize,
    pub(crate) assistant_outputs: Vec<String>,
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn stream_actions<FS, FA, FD, FN, FT, FC>(
        &self,
        prompt_messages: &[PromptMessage],
        environment_registry: &EnvironmentRegistry,
        mut on_stream: FS,
        mut on_action: FA,
        mut on_action_args_delta: FD,
        mut on_action_args_done: FN,
        mut on_assistant_delta: FT,
        mut on_assistant_done: FC,
    ) -> Result<OpenAiStreamOutcome, String>
    where
        FS: FnMut(StreamNote),
        FA: FnMut(ActionInvocation),
        FD: FnMut(ActionArgDeltaNote),
        FN: FnMut(ActionArgDoneNote),
        FT: FnMut(String),
        FC: FnMut(String),
    {
        let Some(api_key) = self.api_key.as_deref() else {
            return Err("OPENAI_API_KEY is required but not configured".to_string());
        };

        let mut attempts = 0usize;
        let reasoning_effort = DEFAULT_REASONING_EFFORT;
        let max_retries = self.retry_policy.max_retries();
        let mut last_error = String::new();

        while attempts <= max_retries {
            on_stream(StreamNote {
                phase: "openai.request.start".to_string(),
                detail: format!("attempt={} effort={reasoning_effort}", attempts + 1),
            });

            let input_messages = prompt_messages
                .iter()
                .map(|message| {
                    json!({
                        "role": message.role,
                        "content": [
                            {
                                "type": "input_text",
                                "text": message.content,
                            }
                        ],
                    })
                })
                .collect::<Vec<_>>();
            let body = json!({
                "model": DEFAULT_MODEL,
                "stream": true,
                "input": input_messages,
                "reasoning": { "effort": reasoning_effort },
                "tools": environment_registry.openai_action_definitions(),
                "tool_choice": "auto"
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
                        .parse_stream(
                            response,
                            &mut on_stream,
                            &mut on_action,
                            &mut on_action_args_delta,
                            &mut on_action_args_done,
                            &mut on_assistant_delta,
                            &mut on_assistant_done,
                        )
                        .await;
                    match result {
                        Ok(outcome) => return Ok(outcome),
                        Err(error) => {
                            if is_non_retryable_stream_error(&error) {
                                return Err(error);
                            }
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

    #[allow(clippy::too_many_arguments)]
    async fn parse_stream<FS, FA, FD, FN, FT, FC>(
        &self,
        response: reqwest::Response,
        on_stream: &mut FS,
        on_action: &mut FA,
        on_action_args_delta: &mut FD,
        on_action_args_done: &mut FN,
        on_assistant_delta: &mut FT,
        on_assistant_done: &mut FC,
    ) -> Result<OpenAiStreamOutcome, String>
    where
        FS: FnMut(StreamNote),
        FA: FnMut(ActionInvocation),
        FD: FnMut(ActionArgDeltaNote),
        FN: FnMut(ActionArgDoneNote),
        FT: FnMut(String),
        FC: FnMut(String),
    {
        let mut stream = response.bytes_stream();
        let mut line_buffer = String::new();
        let mut partial_calls: HashMap<String, PartialActionCall> = HashMap::new();
        let mut dispatched_keys: HashSet<String> = HashSet::new();
        let mut action_call_count = 0usize;
        let mut diagnostics = Vec::new();
        let mut active_assistant_output = String::new();
        let mut assistant_outputs = Vec::new();

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
                    flush_assistant_output(
                        &mut active_assistant_output,
                        &mut assistant_outputs,
                        on_assistant_done,
                    );
                    return Ok(OpenAiStreamOutcome {
                        action_call_count,
                        assistant_outputs,
                        diagnostics,
                    });
                }

                let value: Value = serde_json::from_str(payload)
                    .map_err(|error| format!("invalid stream json payload: {error}"))?;
                handle_stream_event(
                    value,
                    on_stream,
                    on_action,
                    on_action_args_delta,
                    on_action_args_done,
                    on_assistant_delta,
                    on_assistant_done,
                    &mut partial_calls,
                    &mut dispatched_keys,
                    &mut action_call_count,
                    &mut diagnostics,
                    &mut active_assistant_output,
                    &mut assistant_outputs,
                )?;
            }
        }

        flush_assistant_output(
            &mut active_assistant_output,
            &mut assistant_outputs,
            on_assistant_done,
        );

        Ok(OpenAiStreamOutcome {
            action_call_count,
            assistant_outputs,
            diagnostics,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_stream_event<FS, FA, FD, FN, FT, FC>(
    value: Value,
    on_stream: &mut FS,
    on_action: &mut FA,
    on_action_args_delta: &mut FD,
    on_action_args_done: &mut FN,
    on_assistant_delta: &mut FT,
    on_assistant_done: &mut FC,
    partial_calls: &mut HashMap<String, PartialActionCall>,
    dispatched_keys: &mut HashSet<String>,
    action_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
    active_assistant_output: &mut String,
    assistant_outputs: &mut Vec<String>,
) -> Result<(), String>
where
    FS: FnMut(StreamNote),
    FA: FnMut(ActionInvocation),
    FD: FnMut(ActionArgDeltaNote),
    FN: FnMut(ActionArgDoneNote),
    FT: FnMut(String),
    FC: FnMut(String),
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
                    on_action,
                    partial_calls,
                    dispatched_keys,
                    action_call_count,
                    diagnostics,
                )?;
                maybe_capture_assistant_from_item(
                    item,
                    on_assistant_delta,
                    on_assistant_done,
                    active_assistant_output,
                    assistant_outputs,
                );
            }
        }
        "response.output_text.delta" => {
            let delta = value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !delta.is_empty() {
                active_assistant_output.push_str(delta);
                on_assistant_delta(delta.to_string());
            }
        }
        "response.output_text.done" => {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if text.is_empty() {
                flush_assistant_output(
                    active_assistant_output,
                    assistant_outputs,
                    on_assistant_done,
                );
            } else {
                finalize_assistant_output(
                    text,
                    on_assistant_delta,
                    on_assistant_done,
                    active_assistant_output,
                    assistant_outputs,
                );
            }
        }
        "response.function_call_arguments.delta" => {
            let key = extract_call_key(&value).unwrap_or_else(|| "unknown_call".to_string());
            let delta = value
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let partial = partial_calls
                .entry(key.clone())
                .or_insert(PartialActionCall {
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
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                partial.name = Some(name.to_string());
            }
            partial.arguments.push_str(delta);

            if !delta.is_empty() {
                on_action_args_delta(ActionArgDeltaNote {
                    call_key: key,
                    call_id: partial.call_id.clone(),
                    action_id: partial.name.clone(),
                    args_delta: delta.to_string(),
                });
            }
        }
        "response.function_call_arguments.done" => {
            let key = extract_call_key(&value).unwrap_or_else(|| "unknown_call".to_string());
            let arguments = value
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or_default();

            let partial = partial_calls
                .entry(key.clone())
                .or_insert(PartialActionCall {
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
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                partial.name = Some(name.to_string());
            }
            partial.arguments = arguments.to_string();

            on_action_args_done(ActionArgDoneNote {
                call_key: key.clone(),
                call_id: partial.call_id.clone(),
                action_id: partial.name.clone(),
                args_json: partial.arguments.clone(),
            });

            if let Some(name) = partial.name.clone() {
                maybe_dispatch_partial(
                    key,
                    name,
                    partial.arguments.clone(),
                    partial.call_id.clone(),
                    on_action,
                    dispatched_keys,
                    action_call_count,
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
fn maybe_finalize_item<FA>(
    item: &Value,
    on_action: &mut FA,
    partial_calls: &mut HashMap<String, PartialActionCall>,
    dispatched_keys: &mut HashSet<String>,
    action_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<(), String>
where
    FA: FnMut(ActionInvocation),
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

    let entry = partial_calls
        .entry(key.clone())
        .or_insert(PartialActionCall {
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
            on_action,
            dispatched_keys,
            action_call_count,
            diagnostics,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_dispatch_partial<FA>(
    key: String,
    raw_action_id: String,
    arguments_raw: String,
    call_id: Option<String>,
    on_action: &mut FA,
    dispatched_keys: &mut HashSet<String>,
    action_call_count: &mut usize,
    diagnostics: &mut Vec<String>,
) -> Result<(), String>
where
    FA: FnMut(ActionInvocation),
{
    if arguments_raw.trim().is_empty() {
        return Ok(());
    }

    let dispatch_key = call_id.clone().unwrap_or_else(|| key.clone());
    if dispatched_keys.contains(&dispatch_key) {
        return Ok(());
    }

    let args_value: Value = serde_json::from_str(&arguments_raw).map_err(|error| {
        format!(
            "invalid arguments JSON for action `{raw_action_id}`: {error}; payload={arguments_raw}"
        )
    })?;

    let canonical_action_id =
        EnvironmentRegistry::validate(&raw_action_id, &args_value).map_err(|error| {
            format!(
                "action `{raw_action_id}` validation failed: {error}; args={}",
                truncate_for_log(&arguments_raw)
            )
        })?;

    let args_json = serde_json::to_string(&args_value)
        .map_err(|error| format!("failed to canonicalize action args: {error}"))?;

    on_action(ActionInvocation {
        action_id: canonical_action_id.clone(),
        args_json,
        call_key: key.clone(),
        call_id: call_id.clone(),
    });

    diagnostics.push(format!(
        "dispatched action_call={} name={canonical_action_id}",
        dispatch_key
    ));
    dispatched_keys.insert(dispatch_key);
    *action_call_count += 1;

    Ok(())
}

fn maybe_capture_assistant_from_item<FT, FC>(
    item: &Value,
    on_assistant_delta: &mut FT,
    on_assistant_done: &mut FC,
    active_assistant_output: &mut String,
    assistant_outputs: &mut Vec<String>,
) where
    FT: FnMut(String),
    FC: FnMut(String),
{
    if item.get("type").and_then(Value::as_str) != Some("message") {
        return;
    }

    let text = extract_message_text(item);
    if text.trim().is_empty() {
        return;
    }

    finalize_assistant_output(
        text,
        on_assistant_delta,
        on_assistant_done,
        active_assistant_output,
        assistant_outputs,
    );
}

fn extract_message_text(item: &Value) -> String {
    let Some(contents) = item.get("content").and_then(Value::as_array) else {
        return String::new();
    };

    contents
        .iter()
        .filter_map(|content| {
            let is_output_text = content.get("type").and_then(Value::as_str) == Some("output_text");
            if !is_output_text {
                return None;
            }
            content
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>()
        .join("")
}

fn finalize_assistant_output<FT, FC>(
    text: String,
    on_assistant_delta: &mut FT,
    on_assistant_done: &mut FC,
    active_assistant_output: &mut String,
    assistant_outputs: &mut Vec<String>,
) where
    FT: FnMut(String),
    FC: FnMut(String),
{
    if text.starts_with(active_assistant_output.as_str()) {
        let delta = text[active_assistant_output.len()..].to_string();
        if !delta.is_empty() {
            on_assistant_delta(delta.clone());
            active_assistant_output.push_str(&delta);
        }
    } else {
        if !active_assistant_output.is_empty() {
            push_assistant_output(
                assistant_outputs,
                active_assistant_output,
                on_assistant_done,
            );
            active_assistant_output.clear();
        }
        if !text.is_empty() {
            on_assistant_delta(text.clone());
            active_assistant_output.push_str(&text);
        }
    }

    flush_assistant_output(
        active_assistant_output,
        assistant_outputs,
        on_assistant_done,
    );
}

fn flush_assistant_output<FC>(
    active_assistant_output: &mut String,
    assistant_outputs: &mut Vec<String>,
    on_assistant_done: &mut FC,
) where
    FC: FnMut(String),
{
    if active_assistant_output.trim().is_empty() {
        active_assistant_output.clear();
        return;
    }

    push_assistant_output(
        assistant_outputs,
        active_assistant_output,
        on_assistant_done,
    );
    active_assistant_output.clear();
}

fn push_assistant_output<FC>(
    assistant_outputs: &mut Vec<String>,
    text: &str,
    on_assistant_done: &mut FC,
) where
    FC: FnMut(String),
{
    let output = text.to_string();
    if assistant_outputs.last().is_some_and(|last| last == &output) {
        return;
    }
    on_assistant_done(output.clone());
    assistant_outputs.push(output);
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
    status == 408 || status == 409 || status == 429 || status >= 500
}

fn should_retry_transport(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

fn is_non_retryable_stream_error(error: &str) -> bool {
    error.contains("validation failed")
        || error.contains("invalid arguments JSON for action")
        || error.contains("unknown action `")
}

fn truncate_for_log(value: &str) -> String {
    const MAX: usize = 1024;
    if value.len() <= MAX {
        return value.to_string();
    }

    format!("{}… ({} bytes omitted)", &value[..MAX], value.len() - MAX)
}
