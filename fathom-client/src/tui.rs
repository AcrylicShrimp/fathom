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
use tracing::info;

use crate::runtime::bootstrap_demo;

pub async fn run_tui(server: &str) -> Result<()> {
    let lines = bootstrap_demo(server).await?;
    info!(events = lines.len(), "received bootstrap events");
    let content = format!(
        "{}\n\nPress q to quit.",
        lines
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    );

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = loop {
        terminal.draw(|frame| {
            let paragraph = Paragraph::new(content.as_str()).block(
                Block::default()
                    .title("fathom-client")
                    .borders(Borders::ALL),
            );
            frame.render_widget(paragraph, frame.area());
        })?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && key.code == KeyCode::Char('q')
        {
            break Ok(());
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}
