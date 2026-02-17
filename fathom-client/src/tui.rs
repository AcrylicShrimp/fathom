use std::collections::BTreeMap;
use std::io::{self, IsTerminal};
use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use tokio::sync::mpsc;

use crate::commands::{
    CommandSpec, SlashExecution, completion_items, completion_query, execute_slash_command,
};
use crate::runtime::{
    ClientSession, attach_session_events, enqueue_user_message, setup_default_session,
    wait_for_server,
};
use crate::tabs::{
    ConversationTab, FullEventsTab, RunningTasksTab, Tab, TabKeyResult, TaskDetail, ToolsEventsTab,
};
use crate::view::{EventRecord, SessionEventRecordKind, session_event_to_record};

const MAX_COMPLETION_ROWS: usize = 8;

enum AppEvent {
    Record(EventRecord),
    Status(String),
}

#[derive(Clone)]
struct TaskDetailModal {
    detail: TaskDetail,
    scroll: u16,
}

#[derive(Default)]
struct SlashCompletionState {
    query: String,
    items: Vec<CommandSpec>,
    selected_index: usize,
}

impl SlashCompletionState {
    fn refresh_from_input(&mut self, input: &str) {
        let Some(query) = completion_query(input) else {
            self.close();
            return;
        };

        let normalized = query.to_ascii_lowercase();
        if self.query != normalized {
            self.selected_index = 0;
        }

        self.query = normalized.clone();
        self.items = completion_items(normalized.as_str());
        if self.items.is_empty() {
            self.close();
            return;
        }

        if self.selected_index >= self.items.len() {
            self.selected_index = self.items.len().saturating_sub(1);
        }
    }

    fn is_visible(&self) -> bool {
        !self.items.is_empty()
    }

    fn close(&mut self) {
        self.query.clear();
        self.items.clear();
        self.selected_index = 0;
    }

    fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected_index = self
            .selected_index
            .saturating_add(1)
            .min(self.items.len().saturating_sub(1));
    }

    fn selected(&self) -> Option<CommandSpec> {
        self.items.get(self.selected_index).copied()
    }
}

struct App {
    session: ClientSession,
    input: String,
    status: String,
    activity: ActivityState,
    completion: SlashCompletionState,
    task_detail: Option<TaskDetailModal>,
    tabs: Vec<Box<dyn Tab>>,
    active_tab_index: usize,
}

impl App {
    fn new(session: ClientSession) -> Self {
        Self {
            session,
            input: String::new(),
            status: "connected".to_string(),
            activity: ActivityState::default(),
            completion: SlashCompletionState::default(),
            task_detail: None,
            tabs: vec![
                Box::new(ConversationTab::new()),
                Box::new(RunningTasksTab::new()),
                Box::new(ToolsEventsTab::new()),
                Box::new(FullEventsTab::new()),
            ],
            active_tab_index: 0,
        }
    }

    fn push_event(&mut self, event: EventRecord) {
        self.activity.on_event(&event);
        for tab in &mut self.tabs {
            tab.on_event(&event);
        }
    }

    fn active_tab(&self) -> &dyn Tab {
        self.tabs[self.active_tab_index].as_ref()
    }

    fn active_tab_mut(&mut self) -> &mut dyn Tab {
        self.tabs[self.active_tab_index].as_mut()
    }

    fn switch_tab(&mut self) {
        self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
    }

    fn refresh_completion(&mut self) {
        self.completion.refresh_from_input(self.input.as_str());
    }

    fn completion_is_visible(&self) -> bool {
        self.completion.is_visible()
    }

    fn completion_prev(&mut self) {
        self.completion.select_prev();
    }

    fn completion_next(&mut self) {
        self.completion.select_next();
    }

    fn close_completion(&mut self) {
        self.completion.close();
    }

    fn accept_completion(&mut self) -> bool {
        let Some(selected) = self.completion.selected() else {
            return false;
        };
        self.input = format!("/{} ", selected.name);
        self.refresh_completion();
        true
    }

    fn open_task_detail(&mut self, detail: TaskDetail) {
        self.task_detail = Some(TaskDetailModal { detail, scroll: 0 });
    }

    fn close_task_detail(&mut self) {
        self.task_detail = None;
    }

    fn task_detail(&self) -> Option<&TaskDetailModal> {
        self.task_detail.as_ref()
    }

    fn task_detail_mut(&mut self) -> Option<&mut TaskDetailModal> {
        self.task_detail.as_mut()
    }

    fn footer_text(&self) -> &'static str {
        if self.completion_is_visible() {
            "Commands: ↑/↓ select | Tab/Enter accept | Esc close"
        } else {
            "Keys: Shift+Tab switch | Enter send | Ctrl+Enter task detail (tools; Ctrl+J/M fallback) | / opens commands | ↑/↓ scroll/select | Esc clear input | Ctrl+C quit"
        }
    }

    fn activity_text(&self) -> String {
        self.activity.render_line()
    }
}

#[derive(Default)]
struct ActivityState {
    agent_invoking: bool,
    active_tasks: BTreeMap<String, ActiveTask>,
}

#[derive(Debug, Clone)]
struct ActiveTask {
    action_id: String,
    status: String,
}

impl ActivityState {
    fn on_event(&mut self, event: &EventRecord) {
        let EventRecord::Session { kind, .. } = event else {
            return;
        };

        match kind {
            SessionEventRecordKind::AgentStream { phase, .. } => {
                if phase == "agent.turn.attempt" || phase == "openai.request.start" {
                    self.agent_invoking = true;
                }
            }
            SessionEventRecordKind::TurnEnded { .. }
            | SessionEventRecordKind::TurnFailure { .. } => {
                self.agent_invoking = false;
            }
            SessionEventRecordKind::TaskStateChanged {
                task_id,
                action_id,
                status,
                ..
            } => {
                if status == "pending" || status == "running" {
                    self.active_tasks.insert(
                        task_id.clone(),
                        ActiveTask {
                            action_id: action_id.clone(),
                            status: status.clone(),
                        },
                    );
                } else {
                    self.active_tasks.remove(task_id);
                }
            }
            _ => {}
        }
    }

    fn render_line(&self) -> String {
        let agent = if self.agent_invoking {
            "invoking"
        } else {
            "idle"
        };
        let active_count = self.active_tasks.len();

        if active_count == 0 {
            return format!("agent={agent} | active_tasks=0");
        }

        let mut tasks = self
            .active_tasks
            .iter()
            .take(2)
            .map(|(task_id, task)| format!("{task_id} {} ({})", task.action_id, task.status))
            .collect::<Vec<_>>();
        if active_count > 2 {
            tasks.push(format!("+{} more", active_count - 2));
        }

        format!(
            "agent={agent} | active_tasks={active_count} | {}",
            tasks.join(" | ")
        )
    }
}

pub async fn run_tui(server: &str) -> Result<()> {
    if !io::stdout().is_terminal() {
        return Err(anyhow!(
            "interactive TUI requires a real terminal (TTY); run `cargo run` directly in your shell"
        ));
    }

    wait_for_server(server, Duration::from_secs(12)).await?;
    let session = setup_default_session(server).await?;
    run_interactive(server, session).await
}

async fn run_interactive(server: &str, session: ClientSession) -> Result<()> {
    let mut app = App::new(session.clone());
    app.push_event(EventRecord::local(format!(
        "[local] session={} agent={} user={}",
        session.session_id, session.agent_id, session.user_id
    )));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut stream = attach_session_events(server, &session.session_id).await?;
    let stream_event_tx = event_tx.clone();

    tokio::spawn(async move {
        loop {
            match stream.message().await {
                Ok(Some(event)) => {
                    if stream_event_tx
                        .send(AppEvent::Record(session_event_to_record(&event)))
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = stream_event_tx.send(AppEvent::Record(EventRecord::local(
                        "[stream] session event stream closed".to_string(),
                    )));
                    break;
                }
                Err(status) => {
                    let _ = stream_event_tx.send(AppEvent::Record(EventRecord::local(format!(
                        "[stream] session event stream error: {}",
                        status.message()
                    ))));
                    break;
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_loop(server, &mut app, &event_tx, &mut event_rx, &mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

async fn run_loop(
    server: &str,
    app: &mut App,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    event_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    loop {
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Record(record) => app.push_event(record),
                AppEvent::Status(status) => app.status = status,
            }
        }

        let terminal_area: Rect = terminal.size()?.into();
        let footer_height = wrapped_line_count(app.footer_text(), terminal_area.width);
        let rows = main_layout(terminal_area, footer_height);
        let viewport_height = app.active_tab().viewport_height(rows[0]);
        let viewport_width = app.active_tab().viewport_width(rows[0]);
        app.active_tab_mut()
            .sync_scroll(viewport_height, viewport_width);
        if let Some(detail) = app.task_detail_mut() {
            let popup = task_detail_popup_area(terminal_area);
            let max_scroll = task_detail_max_scroll(detail, popup);
            detail.scroll = detail.scroll.min(max_scroll);
        }

        terminal.draw(|frame| {
            let footer_height = wrapped_line_count(app.footer_text(), frame.area().width);
            let rows = main_layout(frame.area(), footer_height);
            app.active_tab()
                .render(frame, rows[0], &app.session.session_id);

            let activity_panel = Paragraph::new(app.activity_text())
                .wrap(Wrap { trim: false })
                .block(Block::default().title("Activity").borders(Borders::ALL));
            frame.render_widget(activity_panel, rows[1]);

            let input_panel = Paragraph::new(app.input.as_str()).block(
                Block::default()
                    .title(format!("Input ({})", app.status))
                    .borders(Borders::ALL),
            );
            frame.render_widget(input_panel, rows[2]);

            if app.completion_is_visible() {
                render_completion_popup(frame, rows[0], &app.completion);
            }

            if let Some(detail) = app.task_detail() {
                render_task_detail_popup(frame, frame.area(), detail);
            }

            frame.render_widget(
                Paragraph::new(app.footer_text()).wrap(Wrap { trim: false }),
                rows[3],
            );

            if app.task_detail().is_none() {
                let x = rows[2]
                    .x
                    .saturating_add(1)
                    .saturating_add(app.input.chars().count() as u16);
                let y = rows[2].y.saturating_add(1);
                frame.set_cursor_position((x, y));
            }
        })?;

        if !event::poll(Duration::from_millis(60))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let page_size = viewport_height.max(1);

        if app.task_detail().is_some() {
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }

            let mut close_modal = false;
            if let Some(detail) = app.task_detail_mut() {
                let popup = task_detail_popup_area(terminal_area);
                let max_scroll = task_detail_max_scroll(detail, popup);
                match key.code {
                    KeyCode::Esc => {
                        close_modal = true;
                    }
                    KeyCode::Up => {
                        detail.scroll = detail.scroll.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        detail.scroll = detail.scroll.saturating_add(1).min(max_scroll);
                    }
                    KeyCode::PageUp => {
                        detail.scroll = detail.scroll.saturating_sub(page_size);
                    }
                    KeyCode::PageDown => {
                        detail.scroll = detail.scroll.saturating_add(page_size).min(max_scroll);
                    }
                    KeyCode::Home => {
                        detail.scroll = 0;
                    }
                    KeyCode::End => {
                        detail.scroll = max_scroll;
                    }
                    _ => {}
                }
            }
            if close_modal {
                app.close_task_detail();
            }
            continue;
        }

        if app.completion_is_visible() {
            match key.code {
                KeyCode::Up => {
                    app.completion_prev();
                    continue;
                }
                KeyCode::Down => {
                    app.completion_next();
                    continue;
                }
                KeyCode::Enter if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = app.accept_completion();
                    continue;
                }
                KeyCode::Tab if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                    let _ = app.accept_completion();
                    continue;
                }
                KeyCode::Esc => {
                    app.close_completion();
                    continue;
                }
                _ => {}
            }
        }

        let input_is_empty = app.input.trim().is_empty();
        match app
            .active_tab_mut()
            .handle_key(&key, input_is_empty, viewport_height, viewport_width)
        {
            TabKeyResult::Handled => continue,
            TabKeyResult::OpenTaskDetail(detail) => {
                app.open_task_detail(detail);
                continue;
            }
            TabKeyResult::Ignored => {}
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
            KeyCode::BackTab => app.switch_tab(),
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => app.switch_tab(),
            KeyCode::Up => app.active_tab_mut().scroll_up(1),
            KeyCode::Down => app
                .active_tab_mut()
                .scroll_down(1, viewport_height, viewport_width),
            KeyCode::PageUp => app.active_tab_mut().scroll_up(page_size),
            KeyCode::PageDown => {
                app.active_tab_mut()
                    .scroll_down(page_size, viewport_height, viewport_width)
            }
            KeyCode::Home => app.active_tab_mut().scroll_to_top(),
            KeyCode::End => app
                .active_tab_mut()
                .scroll_to_bottom(viewport_height, viewport_width),
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let text = normalized_submit_text(app.input.as_str());
                app.input.clear();
                app.refresh_completion();
                let Some(text) = text else {
                    continue;
                };

                if text.starts_with('/') {
                    app.status = "running command...".to_string();
                    let server = server.to_string();
                    let session = app.session.clone();
                    let event_tx = event_tx.clone();
                    tokio::spawn(async move {
                        match execute_slash_command(&text, &server, &session).await {
                            SlashExecution::NotSlashInput => {}
                            SlashExecution::Handled { status, local_log } => {
                                let _ = event_tx.send(AppEvent::Status(status));
                                if let Some(local_log) = local_log {
                                    let _ = event_tx
                                        .send(AppEvent::Record(EventRecord::local(local_log)));
                                }
                            }
                        }
                    });
                    continue;
                }

                app.status = "queueing message...".to_string();
                let server = server.to_string();
                let session_id = app.session.session_id.clone();
                let user_id = app.session.user_id.clone();
                let event_tx = event_tx.clone();
                tokio::spawn(async move {
                    match enqueue_user_message(&server, &session_id, &user_id, &text).await {
                        Ok(trigger_id) => {
                            let _ = event_tx
                                .send(AppEvent::Status(format!("message queued ({trigger_id})")));
                            let _ = event_tx.send(AppEvent::Record(EventRecord::local(format!(
                                "[local] -> {text}"
                            ))));
                        }
                        Err(error) => {
                            let _ =
                                event_tx.send(AppEvent::Status(format!("send failed: {error}")));
                            let _ = event_tx.send(AppEvent::Record(EventRecord::local(format!(
                                "[local] send failed: {error}"
                            ))));
                        }
                    }
                });
            }
            KeyCode::Backspace => {
                app.input.pop();
                app.refresh_completion();
            }
            KeyCode::Char(ch) => {
                app.input.push(ch);
                app.refresh_completion();
            }
            KeyCode::Esc => {
                app.input.clear();
                app.refresh_completion();
            }
            _ => {}
        }
    }
}

fn render_completion_popup(
    frame: &mut ratatui::Frame<'_>,
    history_area: Rect,
    completion: &SlashCompletionState,
) {
    if completion.items.is_empty() {
        return;
    }

    let visible_rows = completion.items.len().min(MAX_COMPLETION_ROWS);
    let selected = completion
        .selected_index
        .min(completion.items.len().saturating_sub(1));
    let start_index = selected.saturating_sub(visible_rows.saturating_sub(1));
    let end_index = start_index
        .saturating_add(visible_rows)
        .min(completion.items.len());

    let visible_items = &completion.items[start_index..end_index];
    let lines = visible_items
        .iter()
        .map(|spec| format!("/{} - {}", spec.name, spec.description))
        .collect::<Vec<_>>();
    let max_content_width = lines
        .iter()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(24);

    let available_width = history_area.width.saturating_sub(2).max(1);
    let width = max_content_width
        .saturating_add(4)
        .max(24)
        .min(available_width);
    let height = (visible_items.len() as u16)
        .saturating_add(2)
        .min(history_area.height.max(1));

    let popup = Rect::new(
        history_area.x.saturating_add(1),
        history_area
            .y
            .saturating_add(history_area.height.saturating_sub(height)),
        width,
        height,
    );

    frame.render_widget(Clear, popup);
    let items = lines.into_iter().map(ListItem::new).collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().title("Commands").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = ListState::default();
    state.select(Some(selected.saturating_sub(start_index)));
    frame.render_stateful_widget(list, popup, &mut state);
}

fn render_task_detail_popup(frame: &mut ratatui::Frame<'_>, area: Rect, detail: &TaskDetailModal) {
    let popup = task_detail_popup_area(area);
    frame.render_widget(Clear, popup);

    let body = task_detail_body(detail);
    let panel = Paragraph::new(body)
        .block(
            Block::default()
                .title(format!(
                    "Task Detail [{}] {}",
                    detail.detail.task_id, detail.detail.action_id
                ))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false })
        .scroll((detail.scroll, 0));
    frame.render_widget(panel, popup);
}

fn task_detail_popup_area(area: Rect) -> Rect {
    let width = area.width.saturating_mul(4).saturating_div(5).max(40);
    let height = area.height.saturating_mul(4).saturating_div(5).max(12);
    let width = width.min(area.width.max(1));
    let height = height.min(area.height.max(1));
    let x = area.x.saturating_add(area.width.saturating_sub(width) / 2);
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    Rect::new(x, y, width, height)
}

fn task_detail_max_scroll(detail: &TaskDetailModal, popup: Rect) -> u16 {
    let content = task_detail_body(detail);
    let content_width = popup.width.saturating_sub(2).max(1);
    let content_height = popup.height.saturating_sub(2).max(1);
    wrapped_line_count(&content, content_width).saturating_sub(content_height)
}

fn task_detail_body(detail: &TaskDetailModal) -> String {
    let args = pretty_json_or_raw(&detail.detail.args_json);
    let result = if detail.detail.result_message.trim().is_empty() {
        "(empty)".to_string()
    } else {
        pretty_json_or_raw(&detail.detail.result_message)
    };

    format!(
        "session_id: {}\n\
task_id: {}\n\
action_id: {}\n\
status: {}\n\
\n\
args_json:\n{}\n\
\n\
result_message:\n{}",
        detail.detail.session_id,
        detail.detail.task_id,
        detail.detail.action_id,
        detail.detail.status,
        args,
        result
    )
}

fn pretty_json_or_raw(source: &str) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }

    match serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
    {
        Some(pretty) => pretty,
        None => trimmed.to_string(),
    }
}

fn main_layout(area: Rect, footer_height: u16) -> [Rect; 4] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(footer_height.max(1)),
        ])
        .split(area);
    [rows[0], rows[1], rows[2], rows[3]]
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

fn normalized_submit_text(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{ActivityState, App, SlashCompletionState, normalized_submit_text};
    use crate::runtime::ClientSession;
    use crate::view::{EventRecord, SessionEventRecordKind};

    fn test_session() -> ClientSession {
        ClientSession {
            session_id: "session-test".to_string(),
            agent_id: "agent-default".to_string(),
            user_id: "user-default".to_string(),
        }
    }

    #[test]
    fn completion_opens_for_slash_prefix() {
        let mut completion = SlashCompletionState::default();
        completion.refresh_from_input("/");
        assert!(completion.is_visible());
        assert_eq!(completion.items[0].name, "heartbeat");

        completion.refresh_from_input("/he");
        assert!(completion.is_visible());
        assert_eq!(completion.items[0].name, "heartbeat");

        completion.refresh_from_input("/zzz");
        assert!(!completion.is_visible());
    }

    #[test]
    fn completion_accept_inserts_command_with_trailing_space() {
        let mut app = App::new(test_session());
        app.input = "/".to_string();
        app.refresh_completion();
        assert!(app.completion_is_visible());

        assert!(app.accept_completion());
        assert_eq!(app.input, "/heartbeat ");
        assert!(!app.completion_is_visible());
    }

    #[test]
    fn normalized_submit_text_rejects_blank_and_trims() {
        assert_eq!(normalized_submit_text(""), None);
        assert_eq!(normalized_submit_text(" \t\n "), None);
        assert_eq!(
            normalized_submit_text("  hello world  "),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn activity_line_updates_from_agent_and_task_events() {
        let mut activity = ActivityState::default();
        assert_eq!(activity.render_line(), "agent=idle | active_tasks=0");

        activity.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::AgentStream {
                phase: "agent.turn.attempt".to_string(),
                detail: "semantic_attempt=1".to_string(),
            },
        });
        assert!(activity.render_line().contains("agent=invoking"));

        activity.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "running".to_string(),
                args_json: "{}".to_string(),
                args_preview: "{}".to_string(),
                result_message: String::new(),
                result_preview: String::new(),
            },
        });
        assert!(activity.render_line().contains("active_tasks=1"));

        activity.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TaskStateChanged {
                task_id: "task-1".to_string(),
                action_id: "filesystem__list".to_string(),
                status: "succeeded".to_string(),
                args_json: "{}".to_string(),
                args_preview: "{}".to_string(),
                result_message: "{}".to_string(),
                result_preview: "{}".to_string(),
            },
        });
        assert!(activity.render_line().contains("active_tasks=0"));

        activity.on_event(&EventRecord::Session {
            session_id: "s1".to_string(),
            kind: SessionEventRecordKind::TurnEnded {
                turn_id: 1,
                reason: "done".to_string(),
                history_size: 0,
            },
        });
        assert_eq!(activity.render_line(), "agent=idle | active_tasks=0");
    }
}
