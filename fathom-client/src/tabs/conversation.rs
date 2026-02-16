use std::collections::{BTreeMap, HashMap, VecDeque};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tabs::{LineBuffer, Tab};
use crate::view::{EventRecord, SessionEventRecordKind};

const FINALIZED_STREAM_CACHE_SIZE: usize = 256;
const LOCAL_USER_PREFIX: &str = "[local] -> ";
const LOCAL_SEND_FAILED_PREFIX: &str = "[local] send failed: ";
const INTERNAL_ASSISTANT_PREFIXES: [&str; 7] = [
    "queued tool `",
    "dispatched tool_call=",
    "tool_calls_dispatched=",
    "no tool call generated on attempt ",
    "openai request failed: ",
    "turn failed [",
    "agent dispatched ",
];

pub(crate) struct ConversationTab {
    lines: LineBuffer,
    active_streams: BTreeMap<String, String>,
    stream_line_indices: HashMap<String, usize>,
    finalized_stream_ids: VecDeque<String>,
}

impl ConversationTab {
    pub(crate) fn new() -> Self {
        Self {
            lines: LineBuffer::new(),
            active_streams: BTreeMap::new(),
            stream_line_indices: HashMap::new(),
            finalized_stream_ids: VecDeque::new(),
        }
    }

    fn append_line(&mut self, line: String) -> usize {
        let outcome = self.lines.push_line(line);
        self.rebase_stream_line_indices(outcome.dropped_prefix);
        outcome.index
    }

    fn update_line(&mut self, line_index: usize, line: String) -> bool {
        self.lines.replace_line(line_index, line)
    }

    fn rebase_stream_line_indices(&mut self, dropped_prefix: usize) {
        if dropped_prefix == 0 {
            return;
        }

        let mut stale_stream_ids = Vec::new();
        for (stream_id, line_index) in &mut self.stream_line_indices {
            if *line_index < dropped_prefix {
                stale_stream_ids.push(stream_id.clone());
            } else {
                *line_index -= dropped_prefix;
            }
        }

        for stream_id in stale_stream_ids {
            self.stream_line_indices.remove(&stream_id);
        }
    }

    fn is_finalized_stream(&self, stream_id: &str) -> bool {
        self.finalized_stream_ids
            .iter()
            .any(|seen| seen == stream_id)
    }

    fn mark_finalized_stream(&mut self, stream_id: &str) {
        self.finalized_stream_ids.push_back(stream_id.to_string());
        if self.finalized_stream_ids.len() > FINALIZED_STREAM_CACHE_SIZE {
            self.finalized_stream_ids.pop_front();
        }
    }

    fn maybe_render_local_user_line(&mut self, message: &str) {
        if let Some(text) = message.strip_prefix(LOCAL_USER_PREFIX) {
            self.append_line(format!("you: {text}"));
            return;
        }
        if let Some(error) = message.strip_prefix(LOCAL_SEND_FAILED_PREFIX) {
            self.append_line(format!("system: send failed: {error}"));
        }
    }

    fn on_assistant_stream(&mut self, stream_id: &str, delta: &str) {
        if self.is_finalized_stream(stream_id) {
            return;
        }

        if !delta.is_empty() {
            self.active_streams
                .entry(stream_id.to_string())
                .or_default()
                .push_str(delta);
        }

        let content = self
            .active_streams
            .get(stream_id)
            .cloned()
            .unwrap_or_default();
        let line = format!("assistant: {content}");

        if let Some(line_index) = self.stream_line_indices.get(stream_id).copied()
            && self.update_line(line_index, line.clone())
        {
            return;
        }

        let line_index = self.append_line(line);
        self.stream_line_indices
            .insert(stream_id.to_string(), line_index);
    }

    fn on_assistant_output(&mut self, content: &str, stream_id: &str) {
        let rendered = format!("assistant: {content}");
        if stream_id.is_empty() {
            if self.is_internal_assistant_output(content) {
                return;
            }
            self.append_line(rendered);
            return;
        }

        if self.is_finalized_stream(stream_id) {
            return;
        }

        let replaced = self
            .stream_line_indices
            .remove(stream_id)
            .is_some_and(|line_index| self.update_line(line_index, rendered.clone()));
        if !replaced {
            self.append_line(rendered);
        }

        self.active_streams.remove(stream_id);
        self.mark_finalized_stream(stream_id);
    }

    fn is_internal_assistant_output(&self, content: &str) -> bool {
        INTERNAL_ASSISTANT_PREFIXES
            .iter()
            .any(|prefix| content.starts_with(prefix))
            || content == "profile copies refreshed for this session"
    }
}

impl Tab for ConversationTab {
    fn title(&self) -> &'static str {
        "Conversation"
    }

    fn on_event(&mut self, event: &EventRecord) {
        match event {
            EventRecord::Local { message } => {
                self.maybe_render_local_user_line(message);
            }
            EventRecord::Session { kind, .. } => match kind {
                SessionEventRecordKind::AssistantOutput { content, stream_id } => {
                    self.on_assistant_output(content, stream_id);
                }
                SessionEventRecordKind::AssistantStream {
                    stream_id, delta, ..
                } => {
                    self.on_assistant_stream(stream_id, delta);
                }
                _ => {}
            },
        }
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str) {
        let mode = if self.lines.is_following() {
            "follow"
        } else {
            "scroll"
        };
        let history = Paragraph::new(self.lines.text())
            .block(
                Block::default()
                    .title(format!("conversation [{}] ({mode})", session_id))
                    .borders(Borders::ALL),
            )
            .scroll((self.lines.scroll_value(), 0));
        frame.render_widget(history, area);
    }

    fn viewport_height(&self, area: Rect) -> u16 {
        LineBuffer::viewport_height(area)
    }

    fn sync_scroll(&mut self, viewport_height: u16) {
        self.lines.sync_scroll(viewport_height);
    }

    fn scroll_up(&mut self, amount: u16) {
        self.lines.scroll_up(amount);
    }

    fn scroll_down(&mut self, amount: u16, viewport_height: u16) {
        self.lines.scroll_down(amount, viewport_height);
    }

    fn scroll_to_top(&mut self) {
        self.lines.scroll_to_top();
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16) {
        self.lines.scroll_to_bottom(viewport_height);
    }
}

#[cfg(test)]
mod tests {
    use super::ConversationTab;
    use crate::tabs::Tab;
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn filters_non_chat_events() {
        let mut tab = ConversationTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnStarted {
                turn_id: 1,
                trigger_count: 1,
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "x".to_string(),
                detail: "y".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 0);
    }

    #[test]
    fn streams_inline_and_finalizes_without_duplicates() {
        let mut tab = ConversationTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantStream {
                stream_id: "t1:c1".to_string(),
                delta: "hel".to_string(),
                done: false,
                user_id: String::new(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantStream {
                stream_id: "t1:c1".to_string(),
                delta: "lo".to_string(),
                done: false,
                user_id: String::new(),
            },
        });
        assert_eq!(tab.lines.line_count(), 1);
        assert_eq!(tab.lines.text(), "assistant: hello");

        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantOutput {
                content: "hello".to_string(),
                stream_id: "t1:c1".to_string(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantOutput {
                content: "hello".to_string(),
                stream_id: "t1:c1".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
        assert_eq!(tab.lines.text(), "assistant: hello");
    }

    #[test]
    fn maps_local_user_line() {
        let mut tab = ConversationTab::new();
        tab.on_event(&EventRecord::Local {
            message: "[local] -> hi".to_string(),
        });

        assert_eq!(tab.lines.text(), "you: hi");
    }

    #[test]
    fn filters_internal_assistant_output_lines() {
        let mut tab = ConversationTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantOutput {
                content: "queued tool `send_message` as task-1 (running)".to_string(),
                stream_id: String::new(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantOutput {
                content: "agent dispatched 1 tool call(s)".to_string(),
                stream_id: String::new(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AssistantOutput {
                content: "hello human".to_string(),
                stream_id: String::new(),
            },
        });

        assert_eq!(tab.lines.text(), "assistant: hello human");
    }
}
