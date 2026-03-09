use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tabs::{ExecutionDetail, LineBuffer, Tab, TabKeyResult};
use crate::view::{EventRecord, SessionEventRecordKind, render_event_record};

pub(crate) struct ExecutionsEventsTab {
    lines: LineBuffer,
    execution_lines: Vec<ExecutionLine>,
    selected_execution_line: Option<usize>,
}

#[derive(Debug, Clone)]
struct ExecutionLine {
    line_index: usize,
    detail: ExecutionDetail,
}

impl ExecutionsEventsTab {
    pub(crate) fn new() -> Self {
        Self {
            lines: LineBuffer::new(),
            execution_lines: Vec::new(),
            selected_execution_line: None,
        }
    }

    fn should_render(event: &EventRecord) -> bool {
        matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::ExecutionUpdate { phase, .. },
                ..
            } if matches!(
                phase.as_str(),
                "arguments.ready"
                    | "awaited_execution_succeeded"
                    | "awaited_execution_failed"
                    | "execution_detached"
                    | "detached_execution_succeeded"
                    | "detached_execution_failed"
                    | "execution_rejected"
            )
        ) || matches!(
            event,
            EventRecord::Session {
                kind: SessionEventRecordKind::ExecutionStateChanged { .. },
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

    fn extract_execution_detail(event: &EventRecord) -> Option<ExecutionDetail> {
        let EventRecord::Session { session_id, kind } = event else {
            return None;
        };

        let SessionEventRecordKind::ExecutionStateChanged {
            execution_id,
            action_id,
            status,
            args_json,
            result_message,
            ..
        } = kind
        else {
            return None;
        };

        Some(ExecutionDetail {
            session_id: session_id.clone(),
            execution_id: execution_id.clone(),
            action_id: action_id.clone(),
            status: status.clone(),
            args_json: args_json.clone(),
            result_message: result_message.clone(),
        })
    }

    fn rebase_execution_lines(&mut self, dropped_prefix: usize) {
        if dropped_prefix == 0 {
            return;
        }

        let selected_line_index = self
            .selected_execution_line
            .and_then(|index| self.execution_lines.get(index))
            .map(|line| line.line_index);

        let mut rebased = Vec::with_capacity(self.execution_lines.len());
        for mut line in self.execution_lines.drain(..) {
            if line.line_index < dropped_prefix {
                continue;
            }
            line.line_index -= dropped_prefix;
            rebased.push(line);
        }

        self.selected_execution_line = selected_line_index.and_then(|line_index| {
            rebased
                .iter()
                .position(|line| line.line_index == line_index)
        });
        self.execution_lines = rebased;
    }

    fn selected_execution_detail(&self) -> Option<ExecutionDetail> {
        self.selected_execution_line
            .and_then(|index| self.execution_lines.get(index))
            .map(|line| line.detail.clone())
    }

    fn select_prev(&mut self, viewport_height: u16, viewport_width: u16) -> bool {
        if self.execution_lines.is_empty() {
            return false;
        }
        let current = match self.selected_execution_line {
            Some(index) => index,
            None => {
                let index = self.execution_lines.len().saturating_sub(1);
                self.selected_execution_line = Some(index);
                self.ensure_selected_visible(viewport_height, viewport_width);
                return true;
            }
        };
        let next = current.saturating_sub(1);
        if next == current {
            return false;
        }
        self.selected_execution_line = Some(next);
        self.ensure_selected_visible(viewport_height, viewport_width);
        true
    }

    fn select_next(&mut self, viewport_height: u16, viewport_width: u16) -> bool {
        if self.execution_lines.is_empty() {
            return false;
        }
        let current = match self.selected_execution_line {
            Some(index) => index,
            None => {
                self.selected_execution_line = Some(0);
                self.ensure_selected_visible(viewport_height, viewport_width);
                return true;
            }
        };
        let next = current
            .saturating_add(1)
            .min(self.execution_lines.len().saturating_sub(1));
        if next == current {
            return false;
        }
        self.selected_execution_line = Some(next);
        self.ensure_selected_visible(viewport_height, viewport_width);
        true
    }

    fn ensure_selected_visible(&mut self, viewport_height: u16, viewport_width: u16) {
        let Some(line_index) = self
            .selected_execution_line
            .and_then(|selected| self.execution_lines.get(selected))
            .map(|line| line.line_index)
        else {
            return;
        };
        self.lines
            .ensure_line_visible(line_index, viewport_height, viewport_width);
    }

    fn selected_render_line_index(&self) -> Option<usize> {
        self.selected_execution_line
            .and_then(|selected| self.execution_lines.get(selected))
            .map(|line| line.line_index)
    }

    fn render_text(&self) -> Text<'static> {
        if self.lines.lines().is_empty() {
            return Text::from(Line::raw("(no events yet)"));
        }

        let selected_line = self.selected_render_line_index();
        let lines = self
            .lines
            .lines()
            .iter()
            .enumerate()
            .map(|(index, line)| {
                if Some(index) == selected_line {
                    Line::styled(
                        line.clone(),
                        Style::default().add_modifier(Modifier::REVERSED),
                    )
                } else {
                    Line::raw(line.clone())
                }
            })
            .collect::<Vec<_>>();

        Text::from(lines)
    }
}

fn is_action_validation_error(detail: &str) -> bool {
    detail.contains("validation failed")
        || detail.contains("invalid arguments JSON for action")
        || detail.contains("unknown action `")
}

fn is_ctrl_enter_like(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            key.code,
            KeyCode::Enter | KeyCode::Char('j') | KeyCode::Char('m')
        )
}

impl Tab for ExecutionsEventsTab {
    fn on_event(&mut self, event: &EventRecord) {
        if Self::should_render(event) {
            let was_following = self.lines.is_following();
            let outcome = self.lines.push_line(render_event_record(event));
            self.rebase_execution_lines(outcome.dropped_prefix);

            if let Some(detail) = Self::extract_execution_detail(event) {
                self.execution_lines.push(ExecutionLine {
                    line_index: outcome.index,
                    detail,
                });
                if self.selected_execution_line.is_none() || was_following {
                    self.selected_execution_line =
                        Some(self.execution_lines.len().saturating_sub(1));
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect, session_id: &str) {
        let mode = if self.lines.is_following() {
            "follow"
        } else {
            "scroll"
        };
        let selected = self
            .selected_execution_line
            .and_then(|index| self.execution_lines.get(index))
            .map(|line| {
                format!(
                    " selected={} {}",
                    line.detail.execution_id, line.detail.action_id
                )
            })
            .unwrap_or_default();
        let panel = Paragraph::new(self.render_text())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(
                        "events:executions [{}] ({mode}){selected}",
                        session_id
                    ))
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

    fn handle_key(
        &mut self,
        key: &KeyEvent,
        _input_is_empty: bool,
        viewport_height: u16,
        viewport_width: u16,
    ) -> TabKeyResult {
        match key.code {
            KeyCode::Up => {
                if self.select_prev(viewport_height, viewport_width) {
                    TabKeyResult::Handled
                } else {
                    TabKeyResult::Ignored
                }
            }
            KeyCode::Down => {
                if self.select_next(viewport_height, viewport_width) {
                    TabKeyResult::Handled
                } else {
                    TabKeyResult::Ignored
                }
            }
            KeyCode::Enter => {
                if !is_ctrl_enter_like(key) {
                    return TabKeyResult::Ignored;
                }

                if let Some(detail) = self.selected_execution_detail() {
                    TabKeyResult::OpenExecutionDetail(detail)
                } else {
                    TabKeyResult::Handled
                }
            }
            KeyCode::Char('j') | KeyCode::Char('m') if is_ctrl_enter_like(key) => {
                if let Some(detail) = self.selected_execution_detail() {
                    TabKeyResult::OpenExecutionDetail(detail)
                } else {
                    TabKeyResult::Handled
                }
            }
            _ => TabKeyResult::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionsEventsTab;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::style::Modifier;

    use crate::tabs::{Tab, TabKeyResult};
    use crate::view::{EventRecord, SessionEventRecordKind};

    #[test]
    fn keeps_execution_update_and_result_events() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionUpdate {
                phase: "execution_detached".to_string(),
                call_key: "call-1".to_string(),
                call_id: "fc_1".to_string(),
                action_id: "shell__run".to_string(),
                execution_id: "execution-1".to_string(),
                args_preview: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
                detail: "submitted action `shell__run` as execution-1 (running) mode=detach"
                    .to_string(),
            },
        });
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "shell__run".to_string(),
                status: "succeeded".to_string(),
                args_json: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
                args_preview: r#"{"command":"pwd","execution_mode":"detach"}"#.to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });

        assert_eq!(tab.lines.line_count(), 2);
    }

    #[test]
    fn filters_openai_stream_events() {
        let mut tab = ExecutionsEventsTab::new();
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
    fn filters_execution_argument_delta_events() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionUpdate {
                phase: "arguments.delta".to_string(),
                call_key: "call-1".to_string(),
                call_id: "fc_1".to_string(),
                action_id: "filesystem__list".to_string(),
                execution_id: String::new(),
                args_preview: r#"{"pat"#.to_string(),
                detail: String::new(),
            },
        });

        assert_eq!(tab.lines.line_count(), 0);
    }

    #[test]
    fn keeps_validation_failure_diagnostics() {
        let mut tab = ExecutionsEventsTab::new();
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
    fn filters_non_execution_lifecycle_events() {
        let mut tab = ExecutionsEventsTab::new();
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
    fn keeps_turn_failure_for_execution_error_context() {
        let mut tab = ExecutionsEventsTab::new();
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

    #[test]
    fn opens_execution_detail_with_ctrl_enter() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__read".to_string(),
                status: "failed".to_string(),
                args_json: r#"{"path":"notes.txt"}"#.to_string(),
                args_preview: r#"{"path":"notes.txt"}"#.to_string(),
                result_message: "not found".to_string(),
                result_preview: "not found".to_string(),
            },
        });

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL);
        let result = tab.handle_key(&key, true, 10, 80);
        assert!(matches!(result, TabKeyResult::OpenExecutionDetail(_)));
    }

    #[test]
    fn plain_enter_is_ignored() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__read".to_string(),
                status: "failed".to_string(),
                args_json: r#"{"path":"notes.txt"}"#.to_string(),
                args_preview: r#"{"path":"notes.txt"}"#.to_string(),
                result_message: "not found".to_string(),
                result_preview: "not found".to_string(),
            },
        });

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = tab.handle_key(&key, true, 10, 80);
        assert!(matches!(result, TabKeyResult::Ignored));
    }

    #[test]
    fn opens_execution_detail_with_ctrl_j_alias() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__read".to_string(),
                status: "failed".to_string(),
                args_json: r#"{"path":"notes.txt"}"#.to_string(),
                args_preview: r#"{"path":"notes.txt"}"#.to_string(),
                result_message: "not found".to_string(),
                result_preview: "not found".to_string(),
            },
        });

        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        let result = tab.handle_key(&key, false, 10, 80);
        assert!(matches!(result, TabKeyResult::OpenExecutionDetail(_)));
    }

    #[test]
    fn opens_execution_detail_with_ctrl_m_alias() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__read".to_string(),
                status: "failed".to_string(),
                args_json: r#"{"path":"notes.txt"}"#.to_string(),
                args_preview: r#"{"path":"notes.txt"}"#.to_string(),
                result_message: "not found".to_string(),
                result_preview: "not found".to_string(),
            },
        });

        let key = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL);
        let result = tab.handle_key(&key, false, 10, 80);
        assert!(matches!(result, TabKeyResult::OpenExecutionDetail(_)));
    }

    #[test]
    fn up_down_with_single_execution_does_not_consume_when_selection_cannot_move() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "running".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                args_preview: r#"{"path":"."}"#.to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(matches!(
            tab.handle_key(&up, true, 10, 80),
            TabKeyResult::Ignored
        ));
        assert!(matches!(
            tab.handle_key(&down, true, 10, 80),
            TabKeyResult::Ignored
        ));
    }

    #[test]
    fn up_down_with_multiple_executions_moves_selection() {
        let mut tab = ExecutionsEventsTab::new();
        for (execution_id, path) in [("execution-1", "."), ("execution-2", "src")] {
            tab.on_event(&EventRecord::Session {
                session_id: "s1".to_string(),
                kind: SessionEventRecordKind::ExecutionStateChanged {
                    execution_id: execution_id.to_string(),
                    action_id: "filesystem__list".to_string(),
                    status: "running".to_string(),
                    args_json: format!(r#"{{"path":"{path}"}}"#),
                    args_preview: format!(r#"{{"path":"{path}"}}"#),
                    result_message: String::new(),
                    result_preview: String::new(),
                },
            });
        }

        assert_eq!(tab.selected_execution_line, Some(1));
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert!(matches!(
            tab.handle_key(&up, true, 10, 80),
            TabKeyResult::Handled
        ));
        assert_eq!(tab.selected_execution_line, Some(0));
        assert!(matches!(
            tab.handle_key(&up, true, 10, 80),
            TabKeyResult::Ignored
        ));
    }

    #[test]
    fn render_text_marks_selected_execution_line() {
        let mut tab = ExecutionsEventsTab::new();
        tab.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::ExecutionStateChanged {
                execution_id: "execution-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "running".to_string(),
                args_json: r#"{"path":"."}"#.to_string(),
                args_preview: r#"{"path":"."}"#.to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });

        let text = tab.render_text();
        assert_eq!(text.lines.len(), 1);
        assert!(
            text.lines[0]
                .style
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }
}
