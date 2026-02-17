use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tabs::Tab;
use crate::view::{EventRecord, SessionEventRecordKind};

#[derive(Debug, Clone)]
struct RunningTask {
    action_id: String,
    status: String,
    args_preview: String,
}

pub(crate) struct RunningTasksTab {
    tasks: BTreeMap<String, RunningTask>,
    scroll: u16,
    follow: bool,
}

impl RunningTasksTab {
    pub(crate) fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            scroll: 0,
            follow: true,
        }
    }

    fn on_task_state_changed(
        &mut self,
        task_id: &str,
        action_id: &str,
        status: &str,
        args_preview: &str,
    ) {
        if is_active_status(status) {
            self.tasks.insert(
                task_id.to_string(),
                RunningTask {
                    action_id: action_id.to_string(),
                    status: status.to_string(),
                    args_preview: args_preview.to_string(),
                },
            );
            return;
        }

        self.tasks.remove(task_id);
    }

    fn text(&self) -> String {
        if self.tasks.is_empty() {
            return "(no running tasks)".to_string();
        }

        self.tasks
            .iter()
            .map(|(task_id, task)| {
                format!(
                    "{task_id} {} -> {} args={}",
                    task.action_id, task.status, task.args_preview
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn max_scroll(&self, viewport_height: u16, viewport_width: u16) -> u16 {
        if viewport_height == 0 {
            return 0;
        }

        wrapped_line_count(self.text().as_str(), viewport_width).saturating_sub(viewport_height)
    }
}

impl Tab for RunningTasksTab {
    fn on_event(&mut self, event: &EventRecord) {
        let EventRecord::Session { kind, .. } = event else {
            return;
        };

        let SessionEventRecordKind::TaskStateChanged {
            task_id,
            action_id,
            status,
            args_preview,
            ..
        } = kind
        else {
            return;
        };

        self.on_task_state_changed(task_id, action_id, status, args_preview);
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str) {
        let mode = if self.follow { "follow" } else { "scroll" };
        let panel = Paragraph::new(self.text())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(
                        "tasks:running [{}] ({mode}) count={}",
                        session_id,
                        self.tasks.len()
                    ))
                    .borders(Borders::ALL),
            )
            .scroll((self.scroll, 0));
        frame.render_widget(panel, area);
    }

    fn viewport_height(&self, area: Rect) -> u16 {
        area.height.saturating_sub(2)
    }

    fn viewport_width(&self, area: Rect) -> u16 {
        area.width.saturating_sub(2).max(1)
    }

    fn sync_scroll(&mut self, viewport_height: u16, viewport_width: u16) {
        let max_scroll = self.max_scroll(viewport_height, viewport_width);
        if self.follow || self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    fn scroll_up(&mut self, amount: u16) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(amount);
    }

    fn scroll_down(&mut self, amount: u16, viewport_height: u16, viewport_width: u16) {
        let max_scroll = self.max_scroll(viewport_height, viewport_width);
        self.scroll = self.scroll.saturating_add(amount).min(max_scroll);
        self.follow = self.scroll == max_scroll;
    }

    fn scroll_to_top(&mut self) {
        self.follow = false;
        self.scroll = 0;
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16, viewport_width: u16) {
        self.scroll = self.max_scroll(viewport_height, viewport_width);
        self.follow = true;
    }
}

fn is_active_status(status: &str) -> bool {
    status == "pending" || status == "running"
}

fn wrapped_line_count(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }

    let wrapped = text
        .lines()
        .map(|line| {
            let chars = line.chars().count().max(1) as u16;
            chars.saturating_sub(1) / width + 1
        })
        .sum::<u16>();
    wrapped.max(1)
}

#[cfg(test)]
mod tests {
    use super::RunningTasksTab;
    use crate::tabs::Tab;
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn tracks_only_active_tasks() {
        let mut tab = RunningTasksTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "running".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                args_preview: r#"{"path":"."}"#.to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });

        assert!(tab.text().contains("task-1 filesystem__list -> running"));

        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "succeeded".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                args_preview: r#"{"path":"."}"#.to_string(),
                result_message: "{}".to_string(),
                result_preview: "{}".to_string(),
            },
        });

        assert_eq!(tab.text(), "(no running tasks)");
    }

    #[test]
    fn ignores_non_task_events() {
        let mut tab = RunningTasksTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnStarted {
                turn_id: 1,
                trigger_count: 1,
            },
        });

        assert_eq!(tab.text(), "(no running tasks)");
    }

    #[test]
    fn scroll_up_disables_follow() {
        let mut tab = RunningTasksTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "running".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                args_preview: r#"{"path":"."}"#.to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });

        tab.scroll_up(1);
        assert!(!tab.follow);
    }
}
