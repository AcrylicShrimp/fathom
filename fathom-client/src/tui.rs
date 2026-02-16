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
use ratatui::widgets::{Block, Borders, Paragraph};
use tokio::sync::mpsc;

use crate::runtime::{
    ClientSession, attach_session_events, enqueue_heartbeat, enqueue_user_message,
    setup_default_session, wait_for_server,
};
use crate::tabs::{ConversationTab, EventsTab, Tab};
use crate::view::{EventRecord, session_event_to_record};

enum AppEvent {
    Record(EventRecord),
    Status(String),
}

struct App {
    session: ClientSession,
    input: String,
    status: String,
    tabs: Vec<Box<dyn Tab>>,
    active_tab_index: usize,
}

impl App {
    fn new(session: ClientSession) -> Self {
        Self {
            session,
            input: String::new(),
            status: "connected".to_string(),
            tabs: vec![Box::new(ConversationTab::new()), Box::new(EventsTab::new())],
            active_tab_index: 0,
        }
    }

    fn push_event(&mut self, event: EventRecord) {
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

    fn tab_label_row(&self) -> String {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                if index == self.active_tab_index {
                    format!("[{}]", tab.title())
                } else {
                    tab.title().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
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

        let rows = main_layout(terminal.size()?.into());
        let viewport_height = app.active_tab().viewport_height(rows[0]);
        let viewport_width = app.active_tab().viewport_width(rows[0]);
        app.active_tab_mut()
            .sync_scroll(viewport_height, viewport_width);

        terminal.draw(|frame| {
            let rows = main_layout(frame.area());
            app.active_tab().render(frame, rows[0], &app.session.session_id);

            let input_panel = Paragraph::new(app.input.as_str()).block(
                Block::default()
                    .title(format!("Input ({})", app.status))
                    .borders(Borders::ALL),
            );
            frame.render_widget(input_panel, rows[1]);

            let footer = format!(
                "Tabs: {} | Keys: Shift+Tab switch | Enter send | q quit (empty input) | Ctrl+C quit | Esc clear | /hb | ↑/↓ line | PgUp/PgDn page | Home/End",
                app.tab_label_row()
            );
            frame.render_widget(Paragraph::new(footer), rows[2]);

            let x = rows[1]
                .x
                .saturating_add(1)
                .saturating_add(app.input.chars().count() as u16);
            let y = rows[1].y.saturating_add(1);
            frame.set_cursor_position((x, y));
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

        match key.code {
            KeyCode::Char('q') if app.input.trim().is_empty() => return Ok(()),
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
            KeyCode::Enter => {
                let text = app.input.trim().to_string();
                app.input.clear();
                if text.is_empty() {
                    continue;
                }

                if text == "/q" {
                    return Ok(());
                }

                if text == "/heartbeat" || text == "/hb" {
                    app.status = "queueing heartbeat...".to_string();
                    let server = server.to_string();
                    let session_id = app.session.session_id.clone();
                    let event_tx = event_tx.clone();
                    tokio::spawn(async move {
                        match enqueue_heartbeat(&server, &session_id).await {
                            Ok(trigger_id) => {
                                let _ = event_tx.send(AppEvent::Status(format!(
                                    "heartbeat queued ({trigger_id})"
                                )));
                                let _ = event_tx.send(AppEvent::Record(EventRecord::local(
                                    format!("[local] heartbeat queued id={trigger_id}"),
                                )));
                            }
                            Err(error) => {
                                let _ = event_tx
                                    .send(AppEvent::Status(format!("heartbeat failed: {error}")));
                                let _ = event_tx.send(AppEvent::Record(EventRecord::local(
                                    format!("[local] heartbeat failed: {error}"),
                                )));
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
            }
            KeyCode::Char(ch) => {
                app.input.push(ch);
            }
            KeyCode::Esc => {
                app.input.clear();
            }
            _ => {}
        }
    }
}

fn main_layout(area: Rect) -> [Rect; 3] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);
    [rows[0], rows[1], rows[2]]
}
