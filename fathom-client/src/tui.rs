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
use crate::view::render_event;

const MAX_LOG_LINES: usize = 10_000;

struct App {
    session: ClientSession,
    input: String,
    logs: Vec<String>,
    status: String,
    log_scroll: u16,
    follow_logs: bool,
}

impl App {
    fn new(session: ClientSession) -> Self {
        Self {
            session,
            input: String::new(),
            logs: Vec::new(),
            status: "connected".to_string(),
            log_scroll: 0,
            follow_logs: true,
        }
    }

    fn push_log(&mut self, line: String) {
        self.logs.push(line);
        if self.logs.len() > MAX_LOG_LINES {
            let overflow = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..overflow);
            self.log_scroll = self.log_scroll.saturating_sub(overflow as u16);
        }
    }

    fn logs_text(&self) -> String {
        if self.logs.is_empty() {
            "(no events yet)".to_string()
        } else {
            self.logs.join("\n")
        }
    }

    fn max_scroll(&self, viewport_height: u16) -> u16 {
        if viewport_height == 0 {
            return 0;
        }

        self.logs
            .len()
            .saturating_sub(viewport_height as usize)
            .min(u16::MAX as usize) as u16
    }

    fn sync_scroll(&mut self, viewport_height: u16) {
        let max_scroll = self.max_scroll(viewport_height);
        if self.follow_logs || self.log_scroll > max_scroll {
            self.log_scroll = max_scroll;
        }
    }

    fn scroll_up(&mut self, amount: u16) {
        self.follow_logs = false;
        self.log_scroll = self.log_scroll.saturating_sub(amount);
    }

    fn scroll_down(&mut self, amount: u16, viewport_height: u16) {
        let max_scroll = self.max_scroll(viewport_height);
        self.log_scroll = self.log_scroll.saturating_add(amount).min(max_scroll);
        self.follow_logs = self.log_scroll == max_scroll;
    }

    fn scroll_to_top(&mut self) {
        self.follow_logs = false;
        self.log_scroll = 0;
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16) {
        self.log_scroll = self.max_scroll(viewport_height);
        self.follow_logs = true;
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
    app.push_log(format!(
        "[local] session={} agent={} user={}",
        session.session_id, session.agent_id, session.user_id
    ));

    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<String>();
    let mut stream = attach_session_events(server, &session.session_id).await?;

    tokio::spawn(async move {
        loop {
            match stream.message().await {
                Ok(Some(event)) => {
                    if event_tx.send(render_event(&event)).is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    let _ = event_tx.send("[stream] session event stream closed".to_string());
                    break;
                }
                Err(status) => {
                    let _ = event_tx.send(format!(
                        "[stream] session event stream error: {}",
                        status.message()
                    ));
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

    let run_result = run_loop(server, &mut app, &mut event_rx, &mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

async fn run_loop(
    server: &str,
    app: &mut App,
    event_rx: &mut mpsc::UnboundedReceiver<String>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    loop {
        while let Ok(line) = event_rx.try_recv() {
            app.push_log(line);
        }

        let log_viewport_height = log_viewport_height(terminal.size()?.into());
        app.sync_scroll(log_viewport_height);

        terminal.draw(|frame| {
            let area = frame.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);

            let log_title = if app.follow_logs {
                format!("fathom-client events [{}] (follow)", app.session.session_id)
            } else {
                format!("fathom-client events [{}] (scroll)", app.session.session_id)
            };
            let log_panel = Paragraph::new(app.logs_text())
                .block(
                    Block::default()
                        .title(log_title)
                        .borders(Borders::ALL),
                )
                .scroll((app.log_scroll, 0));
            frame.render_widget(log_panel, rows[0]);

            let input_panel = Paragraph::new(app.input.as_str()).block(
                Block::default()
                    .title(format!("Input ({})", app.status))
                    .borders(Borders::ALL),
            );
            frame.render_widget(input_panel, rows[1]);

            let footer = "Keys: Enter send | q quit (empty input) | Ctrl+C quit | Esc clear | /hb | ↑/↓ line | PgUp/PgDn page | Home/End";
            let footer_panel = Paragraph::new(footer);
            frame.render_widget(footer_panel, rows[2]);

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

        let page_size = log_viewport_height.max(1);

        match key.code {
            KeyCode::Char('q') if app.input.trim().is_empty() => return Ok(()),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
            KeyCode::Up => app.scroll_up(1),
            KeyCode::Down => app.scroll_down(1, log_viewport_height),
            KeyCode::PageUp => app.scroll_up(page_size),
            KeyCode::PageDown => app.scroll_down(page_size, log_viewport_height),
            KeyCode::Home => app.scroll_to_top(),
            KeyCode::End => app.scroll_to_bottom(log_viewport_height),
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
                    match enqueue_heartbeat(server, &app.session.session_id).await {
                        Ok(trigger_id) => {
                            app.status = format!("heartbeat queued ({trigger_id})");
                            app.push_log(format!("[local] heartbeat queued id={trigger_id}"));
                        }
                        Err(error) => {
                            app.status = format!("heartbeat failed: {error}");
                            app.push_log(format!("[local] heartbeat failed: {error}"));
                        }
                    }
                    continue;
                }

                match enqueue_user_message(
                    server,
                    &app.session.session_id,
                    &app.session.user_id,
                    &text,
                )
                .await
                {
                    Ok(trigger_id) => {
                        app.status = format!("message queued ({trigger_id})");
                        app.push_log(format!("[local] -> {text}"));
                    }
                    Err(error) => {
                        app.status = format!("send failed: {error}");
                        app.push_log(format!("[local] send failed: {error}"));
                    }
                }
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

fn log_viewport_height(area: Rect) -> u16 {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);
    rows[0].height.saturating_sub(2)
}
