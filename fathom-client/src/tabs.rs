mod conversation;
mod events_full;
mod events_tools;

pub(crate) use conversation::ConversationTab;
pub(crate) use events_full::FullEventsTab;
pub(crate) use events_tools::ToolsEventsTab;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::EventRecord;

const MAX_LINES_PER_TAB: usize = 10_000;

#[derive(Debug, Clone)]
pub(crate) struct TaskDetail {
    pub(crate) session_id: String,
    pub(crate) task_id: String,
    pub(crate) action_id: String,
    pub(crate) status: String,
    pub(crate) args_json: String,
    pub(crate) result_message: String,
}

pub(crate) enum TabKeyResult {
    Ignored,
    Handled,
    OpenTaskDetail(TaskDetail),
}

pub(crate) trait Tab {
    fn on_event(&mut self, event: &EventRecord);
    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str);
    fn viewport_height(&self, area: Rect) -> u16;
    fn viewport_width(&self, area: Rect) -> u16;
    fn sync_scroll(&mut self, viewport_height: u16, viewport_width: u16);
    fn scroll_up(&mut self, amount: u16);
    fn scroll_down(&mut self, amount: u16, viewport_height: u16, viewport_width: u16);
    fn scroll_to_top(&mut self);
    fn scroll_to_bottom(&mut self, viewport_height: u16, viewport_width: u16);
    fn handle_key(
        &mut self,
        _key: &KeyEvent,
        _input_is_empty: bool,
        _viewport_height: u16,
        _viewport_width: u16,
    ) -> TabKeyResult {
        TabKeyResult::Ignored
    }
}

pub(super) struct PushOutcome {
    pub(super) index: usize,
    pub(super) dropped_prefix: usize,
}

#[derive(Default)]
pub(super) struct LineBuffer {
    lines: Vec<String>,
    scroll: u16,
    follow: bool,
}

impl LineBuffer {
    pub(super) fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
            follow: true,
        }
    }

    pub(super) fn push_line(&mut self, line: String) -> PushOutcome {
        let old_len = self.lines.len();
        self.lines.push(line);
        let mut dropped_prefix = 0usize;
        if self.lines.len() > MAX_LINES_PER_TAB {
            dropped_prefix = self.lines.len() - MAX_LINES_PER_TAB;
            self.lines.drain(0..dropped_prefix);
            self.scroll = self.scroll.saturating_sub(dropped_prefix as u16);
        }

        let index = old_len.saturating_sub(dropped_prefix);
        PushOutcome {
            index,
            dropped_prefix,
        }
    }

    pub(super) fn replace_line(&mut self, index: usize, line: String) -> bool {
        let Some(slot) = self.lines.get_mut(index) else {
            return false;
        };
        *slot = line;
        true
    }

    #[cfg(test)]
    pub(super) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn lines(&self) -> &[String] {
        &self.lines
    }

    pub(super) fn text(&self) -> String {
        if self.lines.is_empty() {
            "(no events yet)".to_string()
        } else {
            self.lines.join("\n")
        }
    }

    pub(super) fn rendered_text(&self, viewport_width: u16) -> String {
        wrap_text_lines(&self.text(), viewport_width).join("\n")
    }

    pub(super) fn is_following(&self) -> bool {
        self.follow
    }

    pub(super) fn scroll_value(&self) -> u16 {
        self.scroll
    }

    pub(super) fn viewport_height(area: Rect) -> u16 {
        area.height.saturating_sub(2)
    }

    pub(super) fn viewport_width(area: Rect) -> u16 {
        area.width.saturating_sub(2).max(1)
    }

    fn max_scroll(&self, viewport_height: u16, viewport_width: u16) -> u16 {
        if viewport_height == 0 {
            return 0;
        }

        wrapped_line_count(&self.text(), viewport_width)
            .saturating_sub(viewport_height as usize)
            .min(u16::MAX as usize) as u16
    }

    pub(super) fn sync_scroll(&mut self, viewport_height: u16, viewport_width: u16) {
        let max_scroll = self.max_scroll(viewport_height, viewport_width);
        if self.follow || self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub(super) fn scroll_up(&mut self, amount: u16) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub(super) fn scroll_down(&mut self, amount: u16, viewport_height: u16, viewport_width: u16) {
        let max_scroll = self.max_scroll(viewport_height, viewport_width);
        self.scroll = self.scroll.saturating_add(amount).min(max_scroll);
        self.follow = self.scroll == max_scroll;
    }

    pub(super) fn scroll_to_top(&mut self) {
        self.follow = false;
        self.scroll = 0;
    }

    pub(super) fn scroll_to_bottom(&mut self, viewport_height: u16, viewport_width: u16) {
        self.scroll = self.max_scroll(viewport_height, viewport_width);
        self.follow = true;
    }

    pub(super) fn ensure_line_visible(
        &mut self,
        line_index: usize,
        viewport_height: u16,
        viewport_width: u16,
    ) {
        if viewport_height == 0 || line_index >= self.lines.len() {
            return;
        }

        let width = usize::from(viewport_width.max(1));
        let mut visual_start = 0usize;
        let mut visual_end = 0usize;
        let mut visual_cursor = 0usize;

        for (index, line) in self.lines.iter().enumerate() {
            let wraps = line.chars().count().max(1).div_ceil(width);
            if index == line_index {
                visual_start = visual_cursor;
                visual_end = visual_cursor + wraps.saturating_sub(1);
                break;
            }
            visual_cursor = visual_cursor.saturating_add(wraps);
        }

        let viewport_start = self.scroll as usize;
        let viewport_end = viewport_start + viewport_height.saturating_sub(1) as usize;
        let mut updated = None;
        if visual_start < viewport_start {
            updated = Some(visual_start);
        } else if visual_end > viewport_end {
            updated = Some(visual_end + 1 - viewport_height as usize);
        }

        if let Some(new_scroll) = updated {
            let max_scroll = self.max_scroll(viewport_height, viewport_width) as usize;
            self.scroll = new_scroll.min(max_scroll) as u16;
            self.follow = false;
        }
    }
}

fn wrapped_line_count(source: &str, viewport_width: u16) -> usize {
    let width = usize::from(viewport_width.max(1));
    let mut total = 0usize;
    for logical_line in source.split('\n') {
        let char_len = logical_line.chars().count();
        total = total.saturating_add(if char_len == 0 {
            1
        } else {
            char_len.div_ceil(width)
        });
    }
    total.max(1)
}

fn wrap_text_lines(source: &str, viewport_width: u16) -> Vec<String> {
    let width = usize::from(viewport_width.max(1));
    let mut wrapped = Vec::new();

    for logical_line in source.split('\n') {
        if logical_line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut count = 0usize;
        for ch in logical_line.chars() {
            if count == width {
                wrapped.push(current);
                current = String::new();
                count = 0;
            }
            current.push(ch);
            count += 1;
        }
        wrapped.push(current);
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

#[cfg(test)]
mod tests {
    use super::LineBuffer;

    #[test]
    fn wraps_long_lines_for_render() {
        let mut lines = LineBuffer::new();
        let _ = lines.push_line("abcdef".to_string());

        assert_eq!(lines.rendered_text(3), "abc\ndef");
    }

    #[test]
    fn keeps_internal_newlines_and_wraps_each_line() {
        let mut lines = LineBuffer::new();
        let _ = lines.push_line("abc\ndefgh".to_string());

        assert_eq!(lines.rendered_text(4), "abc\ndefg\nh");
    }

    #[test]
    fn computes_scroll_from_wrapped_visual_lines() {
        let mut lines = LineBuffer::new();
        let _ = lines.push_line("abcdefghij".to_string());
        lines.sync_scroll(3, 4);

        assert_eq!(lines.scroll_value(), 0);
        lines.scroll_down(1, 3, 4);
        assert_eq!(lines.scroll_value(), 0);

        lines.scroll_to_bottom(2, 4);
        assert_eq!(lines.scroll_value(), 1);
    }
}
