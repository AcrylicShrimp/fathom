use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tabs::{LineBuffer, Tab};
use crate::view::{EventRecord, SessionEventRecordKind, render_event_record};

pub(crate) struct ToolsEventsTab {
    lines: LineBuffer,
}

impl ToolsEventsTab {
    pub(crate) fn new() -> Self {
        Self {
            lines: LineBuffer::new(),
        }
    }

    fn should_render(event: &EventRecord) -> bool {
        matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::AgentStream { phase, .. },
                ..
            } if phase == "action.queued"
        ) || matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::TaskStateChanged { .. },
                ..
            }
        ) || matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::AgentStream { phase, detail },
                ..
            } if phase == "agent.diagnostic" && is_action_validation_error(detail)
        ) || matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::TurnFailure { .. },
                ..
            }
        )
    }
}

fn is_action_validation_error(detail: &str) -> bool {
    detail.contains("validation failed")
        || detail.contains("invalid arguments JSON for action")
        || detail.contains("unknown action `")
}

impl Tab for ToolsEventsTab {
    fn on_event(&mut self, event: &EventRecord) {
        if Self::should_render(event) {
            let _ = self.lines.push_line(render_event_record(event));
        }
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str) {
        let mode = if self.lines.is_following() {
            "follow"
        } else {
            "scroll"
        };
        let panel = Paragraph::new(self.lines.rendered_text(self.viewport_width(area)))
            .block(
                Block::default()
                    .title(format!("events:tools [{}] ({mode})", session_id))
                    .borders(Borders::ALL),
            )
            .scroll((self.lines.scroll_value(), 0));
        frame.render_widget(panel, area);
    }

    fn viewport_height(&self, area: Rect) -> u16 {
        LineBuffer::viewport_height(area)
    }

    fn viewport_width(&self, area: Rect) -> u16 {
        LineBuffer::viewport_width(area)
    }

    fn sync_scroll(&mut self, viewport_height: u16, viewport_width: u16) {
        self.lines.sync_scroll(viewport_height, viewport_width);
    }

    fn scroll_up(&mut self, amount: u16) {
        self.lines.scroll_up(amount);
    }

    fn scroll_down(&mut self, amount: u16, viewport_height: u16, viewport_width: u16) {
        self.lines
            .scroll_down(amount, viewport_height, viewport_width);
    }

    fn scroll_to_top(&mut self) {
        self.lines.scroll_to_top();
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16, viewport_width: u16) {
        self.lines.scroll_to_bottom(viewport_height, viewport_width);
    }
}

#[cfg(test)]
mod tests {
    use super::ToolsEventsTab;
    use crate::tabs::Tab;
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn keeps_tool_trigger_and_result_events() {
        let mut tab = ToolsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "action.queued".to_string(),
                detail: "queued action `filesystem__list` as task-1 (running)".to_string(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                status: "succeeded".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 2);
    }

    #[test]
    fn filters_openai_stream_events() {
        let mut tab = ToolsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "openai.stream.event".to_string(),
                detail: "response.completed".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 0);
    }

    #[test]
    fn keeps_validation_failure_diagnostics() {
        let mut tab = ToolsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "agent.diagnostic".to_string(),
                detail:
                    "openai request failed: action `filesystem__list` validation failed: missing path"
                        .to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
    }

    #[test]
    fn filters_non_tool_lifecycle_events() {
        let mut tab = ToolsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnStarted {
                turn_id: 1,
                trigger_count: 1,
            },
        });

        assert_eq!(tab.lines.line_count(), 0);
    }

    #[test]
    fn keeps_turn_failure_for_tool_error_context() {
        let mut tab = ToolsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnFailure {
                turn_id: 2,
                reason_code: "openai_error".to_string(),
                message: "action validation failed".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
    }
}
