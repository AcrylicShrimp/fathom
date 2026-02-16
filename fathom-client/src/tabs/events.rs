use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tabs::{LineBuffer, Tab};
use crate::view::{EventRecord, SessionEventRecordKind, render_event_record};

pub(crate) struct EventsTab {
    lines: LineBuffer,
}

impl EventsTab {
    pub(crate) fn new() -> Self {
        Self {
            lines: LineBuffer::new(),
        }
    }

    fn should_render(event: &EventRecord) -> bool {
        !matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::AssistantStream { .. },
                ..
            }
        ) && !matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::AgentStream { phase, detail },
                ..
            } if phase == "openai.stream.event" && detail.ends_with(".delta")
        )
    }
}

impl Tab for EventsTab {
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
                    .title(format!("events [{}] ({mode})", session_id))
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
    use super::EventsTab;
    use crate::tabs::Tab;
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn filters_assistant_stream_delta_events() {
        let mut tab = EventsTab::new();
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
                delta: String::new(),
                done: true,
                user_id: String::new(),
            },
        });

        assert_eq!(tab.lines.line_count(), 0);
    }

    #[test]
    fn keeps_non_stream_events() {
        let mut tab = EventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnStarted {
                turn_id: 1,
                trigger_count: 1,
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
        assert_eq!(tab.lines.text(), "[s1] turn 1 started (1 trigger(s))");
    }

    #[test]
    fn filters_openai_stream_delta_agent_events() {
        let mut tab = EventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "openai.stream.event".to_string(),
                detail: "response.output_text.delta".to_string(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "openai.stream.event".to_string(),
                detail: "response.output_text.done".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
        assert_eq!(
            tab.lines.text(),
            "[s1] agent stream [openai.stream.event] response.output_text.done"
        );
    }
}
