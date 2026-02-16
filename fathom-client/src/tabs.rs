mod conversation;
mod events;

pub(crate) use conversation::ConversationTab;
pub(crate) use events::EventsTab;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::view::EventRecord;

const MAX_LINES_PER_TAB: usize = 10_000;

pub(crate) trait Tab {
    fn title(&self) -> &'static str;
    fn on_event(&mut self, event: &EventRecord);
    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str);
    fn viewport_height(&self, area: Rect) -> u16;
    fn sync_scroll(&mut self, viewport_height: u16);
    fn scroll_up(&mut self, amount: u16);
    fn scroll_down(&mut self, amount: u16, viewport_height: u16);
    fn scroll_to_top(&mut self);
    fn scroll_to_bottom(&mut self, viewport_height: u16);
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

    pub(super) fn text(&self) -> String {
        if self.lines.is_empty() {
            "(no events yet)".to_string()
        } else {
            self.lines.join("\n")
        }
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

    fn max_scroll(&self, viewport_height: u16) -> u16 {
        if viewport_height == 0 {
            return 0;
        }

        self.lines
            .len()
            .saturating_sub(viewport_height as usize)
            .min(u16::MAX as usize) as u16
    }

    pub(super) fn sync_scroll(&mut self, viewport_height: u16) {
        let max_scroll = self.max_scroll(viewport_height);
        if self.follow || self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub(super) fn scroll_up(&mut self, amount: u16) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub(super) fn scroll_down(&mut self, amount: u16, viewport_height: u16) {
        let max_scroll = self.max_scroll(viewport_height);
        self.scroll = self.scroll.saturating_add(amount).min(max_scroll);
        self.follow = self.scroll == max_scroll;
    }

    pub(super) fn scroll_to_top(&mut self) {
        self.follow = false;
        self.scroll = 0;
    }

    pub(super) fn scroll_to_bottom(&mut self, viewport_height: u16) {
        self.scroll = self.max_scroll(viewport_height);
        self.follow = true;
    }
}
