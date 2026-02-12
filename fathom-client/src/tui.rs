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
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tokio::sync::mpsc;

use crate::runtime::{
    ClientSession, attach_session_events, enqueue_heartbeat, enqueue_user_message,
    setup_default_session, wait_for_server,
};
use crate::view::render_event;

const MAX_LOG_LINES: usize = 1_000;
const MAX_VISIBLE_LINES: usize = 250;

struct App {
    session: ClientSession,
    input: String,
    logs: Vec<String>,
    status: String,
}

impl App {
    fn new(session: ClientSession) -> Self {
        Self {
            session,
            input: String::new(),
            logs: Vec::new(),
            status: "connected".to_string(),
        }
    }

    fn push_log(&mut self, line: String) {
        self.logs.push(line);
        if self.logs.len() > MAX_LOG_LINES {
            let overflow = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..overflow);
        }
    }

    fn visible_logs(&self) -> String {
        let start = self.logs.len().saturating_sub(MAX_VISIBLE_LINES);
        if start == self.logs.len() {
            "(no events yet)".to_string()
        } else {
            self.logs[start..].join("\n")
        }
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

    match enqueue_heartbeat(server, &session.session_id).await {
        Ok(trigger_id) => app.push_log(format!("[local] heartbeat queued id={trigger_id}")),
        Err(error) => app.push_log(format!("[local] failed to queue heartbeat: {error}")),
    }

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

        terminal.draw(|frame| {
            let area = frame.area();
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Length(3),
                    Constraint::Length(2),
                ])
                .split(area);

            let log_panel = Paragraph::new(app.visible_logs())
                .block(
                    Block::default()
                        .title("fathom-client events")
                        .borders(Borders::ALL),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(log_panel, rows[0]);

            let input_panel = Paragraph::new(app.input.as_str()).block(
                Block::default()
                    .title("Input (Enter=send)")
                    .borders(Borders::ALL),
            );
            frame.render_widget(input_panel, rows[1]);

            let footer = format!(
                "session={} | {} | q quit | /heartbeat",
                app.session.session_id, app.status
            );
            let footer_panel = Paragraph::new(footer).block(Block::default().borders(Borders::ALL));
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

        match key.code {
            KeyCode::Char('q') if app.input.trim().is_empty() => return Ok(()),
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
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
