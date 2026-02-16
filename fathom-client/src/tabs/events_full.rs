use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tabs::{LineBuffer, Tab};
use crate::view::{EventRecord, render_event_record};

pub(crate) struct FullEventsTab {
    lines: LineBuffer,
}

impl FullEventsTab {
    pub(crate) fn new() -> Self {
        Self {
            lines: LineBuffer::new(),
        }
    }
}

impl Tab for FullEventsTab {
    fn on_event(&mut self, event: &EventRecord) {
        let _ = self.lines.push_line(render_event_record(event));
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
                    .title(format!("events:full [{}] ({mode})", session_id))
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
    use super::FullEventsTab;
    use crate::tabs::Tab;
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn keeps_openai_stream_events() {
        let mut tab = FullEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "openai.stream.event".to_string(),
                detail: "response.output_item.added".to_string(),
            },
        });

        assert_eq!(tab.lines.line_count(), 1);
    }
}
