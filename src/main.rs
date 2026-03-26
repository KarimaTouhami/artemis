use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::Arc;
use tokio::sync::RwLock;

mod compiler;
mod watcher;

use compiler::{CompileState, Compiler};
use watcher::FileWatcher;

const BG: Color = Color::Rgb(0, 0, 0);
const BORDER: Color = Color::Rgb(51, 51, 51);
const HIGHLIGHT: Color = Color::Rgb(0, 255, 65);

struct App {
    state: Arc<RwLock<CompileState>>,
    should_quit: bool,
    c_scroll: u16,
    asm_scroll: u16,
}

impl App {
    fn new(state: Arc<RwLock<CompileState>>) -> Self {
        Self {
            state,
            should_quit: false,
            c_scroll: 0,
            asm_scroll: 0,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up => self.c_scroll = self.c_scroll.saturating_sub(1),
            KeyCode::Down => self.c_scroll = self.c_scroll.saturating_add(1),
            KeyCode::PageUp => self.asm_scroll = self.asm_scroll.saturating_sub(10),
            KeyCode::PageDown => self.asm_scroll = self.asm_scroll.saturating_add(10),
            _ => {}
        }
    }

    async fn render(&mut self, frame: &mut Frame) {
        let state = self.state.read().await;
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(frame.size());

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[0]);

        self.render_c_view(frame, main_chunks[0], &state);
        self.render_asm_view(frame, main_chunks[1], &state);
        self.render_footer(frame, chunks[1], &state);
    }

    fn render_c_view(&self, frame: &mut Frame, area: Rect, state: &CompileState) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(Span::styled("C Source", Style::default().fg(HIGHLIGHT)))
            .style(Style::default().bg(BG));

        let content = if state.c_content.is_empty() {
            "No file loaded".to_string()
        } else {
            state.c_content.clone()
        };

        let paragraph = Paragraph::new(content)
            .block(block)
            .style(Style::default().fg(Color::White).bg(BG))
            .scroll((self.c_scroll, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn render_asm_view(&self, frame: &mut Frame, area: Rect, state: &CompileState) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(Span::styled("Intel Assembly", Style::default().fg(HIGHLIGHT)))
            .style(Style::default().bg(BG));

        let content = if state.asm_content.is_empty() {
            "Waiting for compilation...".to_string()
        } else {
            state.asm_content.clone()
        };

        let paragraph = Paragraph::new(content)
            .block(block)
            .style(Style::default().fg(HIGHLIGHT).bg(BG))
            .scroll((self.asm_scroll, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect, state: &CompileState) {
        let status_style = match state.last_status.as_str() {
            "SUCCESS" => Style::default().fg(HIGHLIGHT),
            "ERROR" => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::Yellow),
        };

        let spans = vec![
            Span::styled("PATH: ", Style::default().fg(BORDER)),
            Span::styled(&state.file_path, Style::default().fg(Color::White)),
            Span::raw(" | "),
            Span::styled("STATUS: ", Style::default().fg(BORDER)),
            Span::styled(&state.last_status, status_style),
            Span::raw(" | "),
            Span::styled("RSP: ", Style::default().fg(BORDER)),
            Span::styled(
                format!("0x{:016x}", state.mock_rsp),
                Style::default().fg(HIGHLIGHT).add_modifier(Modifier::BOLD),
            ),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(BG));

        let paragraph = Paragraph::new(Line::from(spans))
            .block(block)
            .style(Style::default().bg(BG));

        frame.render_widget(paragraph, area);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: artemis <file.c>");
        std::process::exit(1);
    }

    let target_file = &args[1];
    
    let state = Arc::new(RwLock::new(CompileState::new(target_file.clone())));
    let compiler = Compiler::new(state.clone());
    
    compiler.compile().await?;
    
    let _watcher = FileWatcher::new(target_file.clone(), state.clone())?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(state.clone());

    loop {
        terminal.draw(|frame| {
            let app_clone = &mut app;
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    app_clone.render(frame).await;
                });
            });
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(())
}
