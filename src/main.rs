use anyhow::Result;
use std::error::Error;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, BorderType, Paragraph},
    Terminal,
};
use tokio::sync::mpsc;

use tui_textarea::{Input, TextArea};

mod compiler;
mod highlighter;

const VANTABLACK: Color = Color::Rgb(0, 0, 0);
const NEON_GREEN: Color = Color::Rgb(0, 255, 65);
const CYBER_CYAN: Color = Color::Rgb(0, 255, 255);
const DIM_GREEN: Color = Color::Rgb(0, 100, 25);
const ALERT_RED: Color = Color::Rgb(255, 0, 50);
const YELLOW: Color = Color::Rgb(255, 255, 0);
const ORANGE: Color = Color::Rgb(255, 100, 0);
const DARK_GRAY: Color = Color::Rgb(80, 80, 80);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut textarea = TextArea::default();
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
            .title(" SOURCE [C] "),
    );
    textarea.set_style(Style::default().fg(NEON_GREEN).bg(VANTABLACK));

    let (source_tx, source_rx) = mpsc::channel::<String>(8);
    let (asm_tx, mut asm_rx) = mpsc::channel::<String>(8);
    tokio::spawn(async move {
        compiler::spawn_compiler_worker(source_rx, asm_tx).await;
    });

    let mut asm_text = String::new();

    source_tx.send(textarea.lines().join("\n")).await.ok();

    loop {
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(size.height - 3), Constraint::Length(3)].as_ref())
                .split(size);

            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(chunks[0]);

            let asm_lines = highlighter::highlight_asm(&asm_text);
            let asm_text_view = Text::from(asm_lines);

            let asm_pane = Paragraph::new(asm_text_view)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Thick)
                        .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
                        .title(" ASSEMBLY [ASM] "),
                )
                .style(Style::default().bg(VANTABLACK));

            f.render_widget(&textarea, top[0]);
            f.render_widget(asm_pane, top[1]);

            let status = Paragraph::new(Text::from(" MODE: EDIT  |  RUNTIME: OK "))
                .style(Style::default().fg(VANTABLACK).bg(NEON_GREEN).add_modifier(Modifier::BOLD))
                .block(Block::default().style(Style::default().bg(VANTABLACK)));

            f.render_widget(status, chunks[1]);
        })?;

        if let Ok(new_asm) = tokio::time::timeout(Duration::from_millis(10), asm_rx.recv()).await {
            if let Some(s) = new_asm {
                asm_text = s;
            }
        }

        if event::poll(Duration::from_millis(20))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.code == KeyCode::Char('q') && key_event.modifiers.is_empty() {
                    break;
                }

                textarea.input(Input::from(key_event));

                source_tx.send(textarea.lines().join("\n")).await.ok();
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
