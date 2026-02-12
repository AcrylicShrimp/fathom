use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::{Block, Borders, Paragraph};
use tonic::transport::Channel;
use tracing::info;

pub mod pb {
    tonic::include_proto!("fathom.v1");
}

use pb::PingRequest;
use pb::agent_service_client::AgentServiceClient;

pub async fn ping(server: &str, message: impl Into<String>) -> Result<String> {
    let endpoint = Channel::from_shared(server.to_string())?;
    let channel = endpoint.connect().await?;
    let mut client = AgentServiceClient::new(channel);

    let response = client
        .ping(PingRequest {
            message: message.into(),
        })
        .await?;

    Ok(response.into_inner().message)
}

pub async fn run_tui(server: &str) -> Result<()> {
    let response = ping(server, "hello from fathom-client").await?;
    info!(%response, "received ping response");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = loop {
        terminal.draw(|frame| {
            let paragraph = Paragraph::new(format!("Server replied: {response}\nPress q to quit."))
                .block(
                    Block::default()
                        .title("fathom-client")
                        .borders(Borders::ALL),
                );
            frame.render_widget(paragraph, frame.area());
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break Ok(());
                }
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}
