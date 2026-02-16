use std::collections::HashMap;

use serde_json::Value;

use crate::agent::{ToolArgDeltaNote, ToolArgDoneNote};
use crate::pb;
use crate::util::now_unix_ms;

const STREAM_BATCH_WINDOW_MS: i64 = 40;
const SEND_MESSAGE_TOOL_NAME: &str = "send_message";
const SEND_MESSAGE_CONTENT_FIELD: &str = "content";
const SEND_MESSAGE_USER_ID_FIELD: &str = "user_id";

pub(super) struct TurnAssistantStreamEmitter {
    turn_id: u64,
    streams: HashMap<String, SendMessageStreamState>,
}

impl TurnAssistantStreamEmitter {
    pub(super) fn new(turn_id: u64) -> Self {
        Self {
            turn_id,
            streams: HashMap::new(),
        }
    }

    pub(super) fn stream_id_for_invocation(
        &self,
        tool_name: &str,
        call_key: &str,
        call_id: Option<&str>,
    ) -> Option<String> {
        if tool_name != SEND_MESSAGE_TOOL_NAME {
            return None;
        }
        Some(self.stream_id_for_call(call_key, call_id))
    }

    pub(super) fn on_tool_args_delta<F>(&mut self, note: &ToolArgDeltaNote, mut emit: F)
    where
        F: FnMut(pb::session_event::Kind),
    {
        if note.tool_name.as_deref() != Some(SEND_MESSAGE_TOOL_NAME) {
            return;
        }

        let stream_id = self.stream_id_for_call(&note.call_key, note.call_id.as_deref());
        let state = self
            .streams
            .entry(stream_id.clone())
            .or_insert_with(SendMessageStreamState::new);

        let content_delta = state.parser.consume_delta(&note.args_delta);
        if !content_delta.is_empty() {
            state.pending_delta.push_str(&content_delta);
        }

        self.flush_if_due(&stream_id, false, &mut emit);
    }

    pub(super) fn on_tool_args_done<F>(&mut self, note: &ToolArgDoneNote, mut emit: F)
    where
        F: FnMut(pb::session_event::Kind),
    {
        let stream_id = self.stream_id_for_call(&note.call_key, note.call_id.as_deref());
        let stream_exists = self.streams.contains_key(&stream_id);
        if note.tool_name.as_deref() != Some(SEND_MESSAGE_TOOL_NAME) && !stream_exists {
            return;
        }

        let state = self
            .streams
            .entry(stream_id.clone())
            .or_insert_with(SendMessageStreamState::new);
        let done = state.parser.consume_done(&note.args_json);
        if !done.content_delta.is_empty() {
            state.pending_delta.push_str(&done.content_delta);
        }
        if done.user_id.is_some() {
            state.user_id = done.user_id;
        }

        self.flush_if_due(&stream_id, true, &mut emit);
    }

    fn stream_id_for_call(&self, call_key: &str, call_id: Option<&str>) -> String {
        let suffix = call_id.unwrap_or(call_key);
        format!("{}:{suffix}", self.turn_id)
    }

    fn flush_if_due<F>(&mut self, stream_id: &str, force_done: bool, emit: &mut F)
    where
        F: FnMut(pb::session_event::Kind),
    {
        let mut delta_to_emit = String::new();
        let mut emit_done = false;
        let mut user_id = String::new();

        {
            let Some(state) = self.streams.get_mut(stream_id) else {
                return;
            };

            let now = now_unix_ms();
            let should_flush =
                force_done || now - state.last_flush_unix_ms >= STREAM_BATCH_WINDOW_MS;
            if should_flush {
                if !state.pending_delta.is_empty() {
                    delta_to_emit = std::mem::take(&mut state.pending_delta);
                }
                user_id = state.user_id.clone().unwrap_or_default();
                state.last_flush_unix_ms = now;
            }

            if force_done {
                emit_done = true;
                if user_id.is_empty() {
                    user_id = state.user_id.clone().unwrap_or_default();
                }
            }
        }

        if !delta_to_emit.is_empty() {
            emit(pb::session_event::Kind::AssistantStream(
                pb::AssistantStreamEvent {
                    stream_id: stream_id.to_string(),
                    delta: delta_to_emit,
                    done: false,
                    created_at_unix_ms: now_unix_ms(),
                    user_id: user_id.clone(),
                },
            ));
        }

        if emit_done {
            emit(pb::session_event::Kind::AssistantStream(
                pb::AssistantStreamEvent {
                    stream_id: stream_id.to_string(),
                    delta: String::new(),
                    done: true,
                    created_at_unix_ms: now_unix_ms(),
                    user_id,
                },
            ));
            self.streams.remove(stream_id);
        }
    }
}

struct SendMessageStreamState {
    parser: SendMessageArgsParser,
    pending_delta: String,
    last_flush_unix_ms: i64,
    user_id: Option<String>,
}

impl SendMessageStreamState {
    fn new() -> Self {
        Self {
            parser: SendMessageArgsParser::default(),
            pending_delta: String::new(),
            last_flush_unix_ms: 0,
            user_id: None,
        }
    }
}

#[derive(Default)]
struct SendMessageArgsParser {
    raw_arguments: String,
    emitted_content: String,
}

struct SendMessageDone {
    content_delta: String,
    user_id: Option<String>,
}

impl SendMessageArgsParser {
    fn consume_delta(&mut self, args_delta: &str) -> String {
        self.raw_arguments.push_str(args_delta);
        let Some(prefix) =
            extract_string_field_prefix(&self.raw_arguments, SEND_MESSAGE_CONTENT_FIELD)
        else {
            return String::new();
        };
        self.take_new_content(prefix)
    }

    fn consume_done(&mut self, args_json: &str) -> SendMessageDone {
        if !args_json.trim().is_empty() {
            self.raw_arguments = args_json.to_string();
        }

        let value = serde_json::from_str::<Value>(&self.raw_arguments).ok();
        let full_content = value
            .as_ref()
            .and_then(|root| root.get(SEND_MESSAGE_CONTENT_FIELD))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                extract_string_field_prefix(&self.raw_arguments, SEND_MESSAGE_CONTENT_FIELD)
            });
        let content_delta = full_content
            .map(|content| self.take_new_content(content))
            .unwrap_or_default();
        let user_id = value
            .as_ref()
            .and_then(|root| root.get(SEND_MESSAGE_USER_ID_FIELD))
            .and_then(Value::as_str)
            .map(str::to_string);

        SendMessageDone {
            content_delta,
            user_id,
        }
    }

    fn take_new_content(&mut self, content: String) -> String {
        if content.starts_with(&self.emitted_content) {
            let suffix = content[self.emitted_content.len()..].to_string();
            self.emitted_content = content;
            return suffix;
        }

        self.emitted_content = content.clone();
        content
    }
}

fn extract_string_field_prefix(raw: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let key_index = raw.find(&key)?;
    let mut index = key_index + key.len();

    index += skip_ascii_whitespace(raw, index);
    if raw.as_bytes().get(index).copied() != Some(b':') {
        return None;
    }
    index += 1;

    index += skip_ascii_whitespace(raw, index);
    if raw.as_bytes().get(index).copied() != Some(b'"') {
        return None;
    }
    index += 1;

    decode_json_string_prefix(&raw[index..]).map(|(decoded, _)| decoded)
}

fn skip_ascii_whitespace(value: &str, start: usize) -> usize {
    value[start..]
        .bytes()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count()
}

fn decode_json_string_prefix(value: &str) -> Option<(String, bool)> {
    let mut decoded = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some((decoded, true)),
            '\\' => {
                let Some(escape) = chars.next() else {
                    return Some((decoded, false));
                };
                match escape {
                    '"' => decoded.push('"'),
                    '\\' => decoded.push('\\'),
                    '/' => decoded.push('/'),
                    'b' => decoded.push('\u{0008}'),
                    'f' => decoded.push('\u{000C}'),
                    'n' => decoded.push('\n'),
                    'r' => decoded.push('\r'),
                    't' => decoded.push('\t'),
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            let Some(digit) = chars.next() else {
                                return Some((decoded, false));
                            };
                            hex.push(digit);
                        }
                        let codepoint = u32::from_str_radix(&hex, 16).ok()?;
                        let parsed = char::from_u32(codepoint)?;
                        decoded.push(parsed);
                    }
                    _ => return None,
                }
            }
            _ => decoded.push(ch),
        }
    }

    Some((decoded, false))
}

#[cfg(test)]
mod tests {
    use super::{SendMessageArgsParser, TurnAssistantStreamEmitter};
    use crate::agent::{ToolArgDeltaNote, ToolArgDoneNote};
    use crate::pb;

    #[test]
    fn parser_emits_content_from_fragmented_deltas() {
        let mut parser = SendMessageArgsParser::default();

        let first = parser.consume_delta(r#"{"content":"hello "#);
        assert_eq!(first, "hello ");

        let second = parser.consume_delta(r#"world"}"#);
        assert_eq!(second, "world");
    }

    #[test]
    fn parser_decodes_escaped_characters() {
        let mut parser = SendMessageArgsParser::default();

        let first = parser.consume_delta(r#"{"content":"line1\nli"#);
        assert_eq!(first, "line1\nli");

        let second = parser.consume_delta(r#"ne2"}"#);
        assert_eq!(second, "ne2");
    }

    #[test]
    fn parser_uses_done_payload_for_tail_and_user_id() {
        let mut parser = SendMessageArgsParser::default();
        assert_eq!(parser.consume_delta(r#"{"content":"hel"#), "hel");

        let done = parser.consume_done(r#"{"content":"hello","user_id":"user-1"}"#);
        assert_eq!(done.content_delta, "lo");
        assert_eq!(done.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn emitter_emits_done_event_for_send_message_streams() {
        let mut emitter = TurnAssistantStreamEmitter::new(9);
        let mut events = Vec::new();

        emitter.on_tool_args_delta(
            &ToolArgDeltaNote {
                call_key: "call-key".to_string(),
                call_id: Some("call-123".to_string()),
                tool_name: Some("send_message".to_string()),
                args_delta: r#"{"content":"hello"}"#.to_string(),
            },
            |event| events.push(event),
        );
        emitter.on_tool_args_done(
            &ToolArgDoneNote {
                call_key: "call-key".to_string(),
                call_id: Some("call-123".to_string()),
                tool_name: Some("send_message".to_string()),
                args_json: r#"{"content":"hello"}"#.to_string(),
            },
            |event| events.push(event),
        );

        assert!(events.iter().any(|event| matches!(
            event,
            pb::session_event::Kind::AssistantStream(pb::AssistantStreamEvent {
                done: true,
                stream_id,
                ..
            }) if stream_id == "9:call-123"
        )));
    }
}
