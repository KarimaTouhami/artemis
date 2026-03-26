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
    widgets::{Block, Borders, BorderType, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tui_textarea::TextArea;

mod compiler;
mod watcher;

use compiler::{CompileState, Compiler};
use watcher::FileWatcher;

// The Artemis Palette
const VANTABLACK: Color = Color::Rgb(0, 0, 0);       // Pure Black
const NEON_GREEN: Color = Color::Rgb(0, 255, 65);    // Primary Text
const DIM_GREEN: Color = Color::Rgb(0, 100, 25);     // Inactive/Borders
const CYBER_CYAN: Color = Color::Rgb(0, 255, 255);   // Keywords/Registers
const ALERT_RED: Color = Color::Rgb(255, 0, 50);     // Segfaults/Errors

struct App<'a> {
    state: Arc<RwLock<CompileState>>,
    textarea: TextArea<'a>,
    source_path: String,
    should_quit: bool,
    c_scroll: u16,
    asm_scroll: u16,
    last_edit: Option<Instant>,
    needs_compile: bool,
}

impl<'a> App<'a> {
    fn new(state: Arc<RwLock<CompileState>>, source_path: String, initial_text: String) -> App<'a> {
        let mut textarea = TextArea::from(initial_text.lines());
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Thick)
                .border_style(Style::default().fg(DIM_GREEN))
                .title(Span::styled(" ⚡ SYSTEM_CORE: SOURCE.C ", Style::default().fg(NEON_GREEN).add_modifier(Modifier::BOLD))),
        );
        textarea.set_style(Style::default().fg(NEON_GREEN).bg(VANTABLACK));
        textarea.set_cursor_style(Style::default().bg(NEON_GREEN).fg(Color::Black));

        Self {
            state,
            textarea,
            source_path,
            should_quit: false,
            c_scroll: 0,
            asm_scroll: 0,
            last_edit: None,
            needs_compile: false,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if key.code == KeyCode::Char('q') {
            self.should_quit = true;
            return;
        }

        // Update text area input for editing
        self.textarea.input(key);
        self.needs_compile = true;
        self.last_edit = Some(Instant::now());

        match key.code {
            KeyCode::Up => self.c_scroll = self.c_scroll.saturating_sub(1),
            KeyCode::Down => self.c_scroll = self.c_scroll.saturating_add(1),
            KeyCode::PageUp => self.asm_scroll = self.asm_scroll.saturating_sub(10),
            KeyCode::PageDown => self.asm_scroll = self.asm_scroll.saturating_add(10),
            _ => {}
        }
    }

    async fn render(&mut self, frame: &mut Frame<'_>) {
        let state = self.state.read().await.clone();
        
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

    fn render_c_view(&mut self, frame: &mut Frame<'_>, area: Rect, _state: &CompileState) {
        self.textarea
            .set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .border_style(Style::default().fg(DIM_GREEN))
                    .title(Span::styled(" SOURCE [C] ", Style::default().fg(CYBER_CYAN).add_modifier(Modifier::BOLD)))
                    .style(Style::default().bg(VANTABLACK)),
            );

        let widget = self.textarea.widget();
        frame.render_widget(widget, area);
    }

    fn render_asm_view(&self, frame: &mut Frame<'_>, area: Rect, state: &CompileState) {
        let asm_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(DIM_GREEN))
            .title(Span::styled(" ASSEMBLY [ASM] ", Style::default().fg(CYBER_CYAN).add_modifier(Modifier::BOLD)))
            .style(Style::default().bg(VANTABLACK));

        let (cursor_row, _) = self.textarea.cursor();
        let highlight_map = state
            .line_map
            .get(&(cursor_row + 1))
            .cloned()
            .unwrap_or_default();
        let highlight_set: HashSet<usize> = highlight_map.into_iter().collect();

        let content = if state.asm_content.is_empty() {
            vec![Line::from(Span::styled("Waiting for compilation...", Style::default().fg(Color::DarkGray).bg(VANTABLACK)))]
        } else {
            state
                .asm_content
                .lines()
                .enumerate()
                .map(|(idx, line)| {
                    let trimmed = line.trim_start();
                    let base_style = if trimmed.starts_with("mov")
                        || trimmed.starts_with("push")
                        || trimmed.starts_with("pop")
                        || trimmed.starts_with("add")
                        || trimmed.starts_with("sub")
                    {
                        Style::default().fg(CYBER_CYAN).bg(VANTABLACK)
                    } else if trimmed.starts_with('.') {
                        Style::default().fg(Color::DarkGray).bg(VANTABLACK)
                    } else {
                        Style::default().fg(NEON_GREEN).bg(VANTABLACK)
                    };

                    let style = if highlight_set.contains(&idx) {
                        base_style
                            .bg(Color::Rgb(0, 50, 0))
                            .fg(NEON_GREEN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        base_style
                    };

                    Line::from(vec![Span::styled(line, style)])
                })
                .collect::<Vec<Line>>()
        };

        let paragraph = Paragraph::new(content)
            .block(asm_block)
            .style(Style::default().bg(VANTABLACK))
            .scroll((self.asm_scroll, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }

    fn render_footer(&self, frame: &mut Frame<'_>, area: Rect, state: &CompileState) {
        let status_style = match state.last_status.as_str() {
            "SUCCESS" => Style::default().fg(NEON_GREEN).bg(VANTABLACK),
            "ERROR" => Style::default().fg(ALERT_RED).bg(VANTABLACK).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(Color::Yellow).bg(VANTABLACK),
        };

        let pulse_line = Line::from(vec![
            Span::styled(" NORMAL ", Style::default().bg(NEON_GREEN).fg(VANTABLACK).add_modifier(Modifier::BOLD)),
            Span::raw(" | PATH: "),
            Span::styled(&state.file_path, Style::default().fg(NEON_GREEN).bg(VANTABLACK)),
            Span::raw(" | "),
            Span::styled(" STATUS: ", Style::default().bg(DIM_GREEN).fg(VANTABLACK).add_modifier(Modifier::BOLD)),
            Span::styled(&state.last_status, status_style),
            Span::raw(" | RSP: "),
            Span::styled(
                format!("0x{:016x}", state.mock_rsp),
                Style::default().fg(CYBER_CYAN).bg(VANTABLACK),
            ),
        ]);

        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(DIM_GREEN))
            .style(Style::default().bg(VANTABLACK));

        let paragraph = Paragraph::new(pulse_line)
            .block(block)
            .style(Style::default().bg(VANTABLACK));

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

    let initial_c_text = {
        let s = state.read().await.c_content.clone();
        if s.is_empty() {
            fs::read_to_string(target_file).unwrap_or_default()
        } else {
            s
        }
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(state.clone(), target_file.clone(), initial_c_text);

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

        if app.needs_compile {
            if let Some(last) = app.last_edit {
                if last.elapsed() >= Duration::from_millis(300) {
                    app.needs_compile = false;
                    // Persist textarea content to disk and recompile
                    let code = app.textarea.lines().join("\n");
                    if let Err(err) = fs::write(&app.source_path, &code) {
                        eprintln!("Failed to write source: {}", err);
                    } else {
                        // Compilation is synchronous here for predictable updates
                        if let Err(err) = compiler.compile().await {
                            eprintln!("Compile error: {}", err);
                        }
                    }
                }
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
