use anyhow::Result;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Text, Line},
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Focus {
    Source,
    Assembly,
}

fn asm_max_scroll(asm_text: &str) -> u16 {
    asm_text.lines().count().saturating_sub(1) as u16
}

fn parse_loc_directives(asm_text: &str) -> HashMap<usize, Vec<usize>> {
    let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, line) in asm_text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(".loc") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Ok(line_num) = parts[2].parse::<usize>() {
                    map.entry(line_num).or_default().push(idx);
                }
            }
        }
    }
    map
}

fn handle_asm_navigation(key_event: crossterm::event::KeyEvent, asm_text: &str, asm_scroll: &mut u16) -> bool {
    let max_scroll = asm_max_scroll(asm_text);

    match key_event.code {
        KeyCode::Up => {
            *asm_scroll = asm_scroll.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            *asm_scroll = (*asm_scroll).saturating_add(1).min(max_scroll);
            true
        }
        KeyCode::PageUp => {
            *asm_scroll = asm_scroll.saturating_sub(10);
            true
        }
        KeyCode::PageDown => {
            *asm_scroll = (*asm_scroll).saturating_add(10).min(max_scroll);
            true
        }
        KeyCode::Home => {
            *asm_scroll = 0;
            true
        }
        KeyCode::End => {
            *asm_scroll = max_scroll;
            true
        }
        KeyCode::Char('k') if key_event.modifiers.is_empty() => {
            *asm_scroll = asm_scroll.saturating_sub(1);
            true
        }
        KeyCode::Char('j') if key_event.modifiers.is_empty() => {
            *asm_scroll = (*asm_scroll).saturating_add(1).min(max_scroll);
            true
        }
        KeyCode::Char('u') if key_event.modifiers.is_empty() => {
            *asm_scroll = asm_scroll.saturating_sub(5);
            true
        }
        KeyCode::Char('d') if key_event.modifiers.is_empty() => {
            *asm_scroll = (*asm_scroll).saturating_add(5).min(max_scroll);
            true
        }
        KeyCode::Char('b') if key_event.modifiers.is_empty() => {
            *asm_scroll = asm_scroll.saturating_sub(10);
            true
        }
        KeyCode::Char('f') if key_event.modifiers.is_empty() => {
            *asm_scroll = (*asm_scroll).saturating_add(10).min(max_scroll);
            true
        }
        KeyCode::Char('g') if key_event.modifiers.is_empty() => {
            *asm_scroll = 0;
            true
        }
        KeyCode::Char('G') if key_event.modifiers == KeyModifiers::SHIFT => {
            *asm_scroll = max_scroll;
            true
        }
        _ => false,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let source_path = std::env::args().nth(1).unwrap_or_else(|| "example.c".into());
    let source_content = std::fs::read_to_string(&source_path).unwrap_or_default();

    let mut textarea = TextArea::from(source_content.lines().map(String::from));
    textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
            .title(format!(" SOURCE [C] ({}) ", source_path)),
    );
    textarea.set_style(Style::default().fg(NEON_GREEN).bg(VANTABLACK));

    let (source_tx, source_rx) = mpsc::channel::<String>(8);
    let (asm_tx, mut asm_rx) = mpsc::channel::<String>(8);
    tokio::spawn(async move {
        compiler::spawn_compiler_worker(source_rx, asm_tx).await;
    });

    let mut asm_text = String::new();
    let mut asm_loc_map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut status_msg = "MODE: EDIT | RUNTIME: OK".to_string();
    let mut focus = Focus::Source;
    let mut show_help = false;
    let mut asm_scroll: u16 = 0;
    let mut follow_mode = true;
    let mut focus_switch_armed = false;

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

            let mut asm_lines = highlighter::highlight_asm(&asm_text);

            let source_cursor_line = (textarea.cursor().0 + 1) as usize;
            // Resolve the source line used for ASM mapping.
            // If the exact cursor line has no `.loc`, fall back to the nearest previous mapped line.
            let mapped_source_line = if asm_loc_map.contains_key(&source_cursor_line) {
                Some(source_cursor_line)
            } else {
                let mut mapped = None;
                for offset in 1..=source_cursor_line {
                    let candidate = source_cursor_line.saturating_sub(offset);
                    if asm_loc_map.contains_key(&candidate) {
                        mapped = Some(candidate);
                        break;
                    }
                }
                mapped
            };

            let selected_asm_lines = mapped_source_line.and_then(|line| asm_loc_map.get(&line));

            // Highlight all assembly lines that correspond to the current C line
            if let Some(lines) = selected_asm_lines {
                for &asm_line_idx in lines {
                    if (asm_line_idx as usize) < asm_lines.len() {
                        for span in &mut asm_lines[asm_line_idx as usize].spans {
                            span.style = span.style.bg(Color::Rgb(0, 200, 100)).add_modifier(Modifier::BOLD);
                        }
                    }
                }
            }

            if follow_mode {
                if let Some(line) = selected_asm_lines.and_then(|lines| lines.first()).cloned() {
                    asm_scroll = (line as u16).min(asm_max_scroll(&asm_text));
                }
            }

            let asm_text_view = Text::from(asm_lines);

            let source_border_style = if focus == Focus::Source {
                Style::default().fg(CYBER_CYAN)
            } else {
                Style::default().fg(DIM_GREEN)
            };
            let assembly_border_style = if focus == Focus::Assembly {
                Style::default().fg(CYBER_CYAN)
            } else {
                Style::default().fg(DIM_GREEN)
            };

            textarea.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
                    .border_style(source_border_style)
                    .title(format!(" SOURCE [C] ({}) ", source_path)),
            );

            let asm_pane = Paragraph::new(asm_text_view)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Thick)
                        .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
                        .border_style(assembly_border_style)
                        .title(" ASSEMBLY [ASM] "),
                )
                .style(Style::default().bg(VANTABLACK))
                .scroll((asm_scroll, 0));

            f.render_widget(&textarea, top[0]);
            f.render_widget(asm_pane, top[1]);

            if show_help {
                let help_text = Text::from(vec![
                    Line::from("Controls:"),
                    Line::from("  q / Ctrl+C: Quit"),
                    Line::from("  ? : Toggle help"),
                    Line::from("  Esc then Tab / Shift+Tab: Switch pane"),
                    Line::from("  ASM nav: Up/Down/PgUp/PgDn/Home/End or j/k/u/d/b/f/g/G"),
                    Line::from("  Ctrl+S: Save"),
                    Line::from("  r: Reload file"),
                    Line::from("  F5: Toggle follow mode"),
                    Line::from("  /: Search mode (placeholder currently)")
                ]);
                let overlay = Paragraph::new(help_text)
                    .style(Style::default().bg(VANTABLACK).fg(NEON_GREEN))
                    .block(Block::default().borders(Borders::ALL).title(" HELP ").border_style(Style::default().fg(CYBER_CYAN)));
                f.render_widget(overlay, chunks[0]);
            }

            let status_line = format!(
                "{} | FOCUS: {:?} | FOLLOW: {} | ASM scroll: {}",
                status_msg, focus, follow_mode, asm_scroll
            );

            let status = Paragraph::new(Text::from(status_line))
                .style(Style::default().fg(VANTABLACK).bg(NEON_GREEN).add_modifier(Modifier::BOLD))
                .block(Block::default().style(Style::default().bg(VANTABLACK)));

            f.render_widget(status, chunks[1]);
        })?;

        if let Ok(new_asm) = tokio::time::timeout(Duration::from_millis(10), asm_rx.recv()).await {
            if let Some(s) = new_asm {
                asm_text = s;
                asm_loc_map = parse_loc_directives(&asm_text);

                if follow_mode {
                    let source_cursor_line = textarea.cursor().0 + 1; // convert to 1-based location
                    asm_scroll = if let Some(asm_lines) = asm_loc_map.get(&source_cursor_line) {
                        *asm_lines.first().unwrap_or(&0) as u16
                    } else {
                        // fallback: keep source cursor line if no mapping available
                        (source_cursor_line.saturating_sub(1) as u16).min(asm_max_scroll(&asm_text))
                    };
                } else {
                    asm_scroll = asm_scroll.min(asm_max_scroll(&asm_text));
                }
            }
        }

        if event::poll(Duration::from_millis(20))? {
            if let Event::Key(key_event) = event::read()? {
                if (key_event.code == KeyCode::Char('q') && key_event.modifiers.is_empty())
                    || (key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }

                if key_event.code == KeyCode::Char('?') && key_event.modifiers.is_empty() {
                    show_help = !show_help;
                    status_msg = if show_help {
                        "HELP: press ? to close".to_string()
                    } else {
                        "MODE: EDIT | RUNTIME: OK".to_string()
                    };
                    continue;
                }

                if show_help {
                    if key_event.code == KeyCode::Esc {
                        show_help = false;
                        status_msg = "MODE: EDIT | RUNTIME: OK".to_string();
                    }
                    continue;
                }

                if key_event.code == KeyCode::Char('/') && key_event.modifiers.is_empty() {
                    status_msg = "SEARCH mode is not implemented; use source editor techniques".to_string();
                    continue;
                }

                if key_event.code == KeyCode::Esc && key_event.modifiers.is_empty() {
                    focus_switch_armed = !focus_switch_armed;
                    status_msg = if focus_switch_armed {
                        "FOCUS switch armed: press Tab/Shift+Tab".to_string()
                    } else {
                        "FOCUS switch canceled".to_string()
                    };
                    continue;
                }

                if key_event.code == KeyCode::F(5) {
                    follow_mode = !follow_mode;
                    status_msg = format!("FOLLOW mode: {}", if follow_mode { "ON" } else { "OFF" });
                    continue;
                }

                if key_event.code == KeyCode::Char('r') && key_event.modifiers.is_empty() {
                    match std::fs::read_to_string(&source_path) {
                        Ok(content) => {
                            textarea = TextArea::from(content.lines().map(String::from));
                            textarea.set_style(Style::default().fg(NEON_GREEN).bg(VANTABLACK));
                            textarea.set_block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .border_type(BorderType::Thick)
                                    .style(Style::default().fg(NEON_GREEN).bg(VANTABLACK))
                                    .title(format!(" SOURCE [C] ({}) ", source_path)),
                            );
                            source_tx.send(textarea.lines().join("\n")).await.ok();
                            status_msg = format!("RELOADED {}", source_path);
                        }
                        Err(e) => status_msg = format!("RELOAD FAILED: {}", e),
                    }
                    continue;
                }

                if key_event.code == KeyCode::Tab {
                    if focus_switch_armed {
                        focus = if focus == Focus::Source { Focus::Assembly } else { Focus::Source };
                        status_msg = format!("FOCUS -> {:?}", focus);
                        focus_switch_armed = false;
                    } else {
                        status_msg = "Press Esc first, then Tab to switch pane".to_string();
                    }
                    continue;
                } else if key_event.code == KeyCode::BackTab {
                    if focus_switch_armed {
                        focus = if focus == Focus::Assembly { Focus::Source } else { Focus::Assembly };
                        status_msg = format!("FOCUS -> {:?}", focus);
                        focus_switch_armed = false;
                    } else {
                        status_msg = "Press Esc first, then Shift+Tab to switch pane".to_string();
                    }
                    continue;
                }

                if key_event.code == KeyCode::Char('s') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    match std::fs::write(&source_path, textarea.lines().join("\n")) {
                        Ok(_) => status_msg = format!("SAVED to {}", source_path),
                        Err(e) => status_msg = format!("SAVE FAILED: {}", e),
                    }
                    continue;
                }

                match focus {
                    Focus::Source => {
                        textarea.input(Input::from(key_event));
                        status_msg = "MODE: EDIT | RUNTIME: OK".to_string();
                        source_tx.send(textarea.lines().join("\n")).await.ok();
                    }
                    Focus::Assembly => {
                        let handled = handle_asm_navigation(key_event, &asm_text, &mut asm_scroll);
                        if handled {
                            status_msg = format!(
                                "MODE: VIEW ASM | SCROLL: {}/{}",
                                asm_scroll,
                                asm_max_scroll(&asm_text)
                            );
                        } else {
                            status_msg = "MODE: VIEW ASM | use j/k/u/d/b/f/g/G or arrows".to_string();
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
