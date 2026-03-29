use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use regex::Regex;

const VANTABLACK: Color = Color::Rgb(0, 0, 0);
const NEON_GREEN: Color = Color::Rgb(0, 255, 65);
const CYBER_CYAN: Color = Color::Rgb(0, 255, 255);
const YELLOW: Color = Color::Rgb(255, 255, 0);
const ORANGE: Color = Color::Rgb(255, 100, 0);
const DARK_GRAY: Color = Color::Rgb(80, 80, 80);

lazy_static::lazy_static! {
    static ref TOKEN_RE: Regex = Regex::new(
        r"(?x)
        (?P<label>^[[:space:]]*[\.\w]+:)
        |(?P<directive>\.[a-zA-Z_]\w*)
        |(?P<instr>\b(?:mov|push|pop|add|sub|ret|lea|cmp|jmp|je|jne|call|movl|movq|xor)\b)
        |(?P<reg>\b(?:rax|rbp|rsp|rbx|rcx|rdx|rsi|rdi|r8|r9|r10|r11|r12|r13|r14|r15)\b)
        |(?P<const>(?:0x[0-9A-Fa-f]+|\b\d+\b))
        "
    ).unwrap();
}

pub fn highlight_asm(asm: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for line in asm.lines() {
        let mut spans = Vec::new();
        let mut last = 0;

        for caps in TOKEN_RE.captures_iter(line) {
            let m = caps.get(0).unwrap();
            if m.start() > last {
                spans.push(Span::styled(
                    line[last..m.start()].to_string(),
                    Style::default().fg(NEON_GREEN).bg(VANTABLACK),
                ));
            }

            let style = if caps.name("label").is_some() {
                Style::default().fg(NEON_GREEN).bg(VANTABLACK).add_modifier(Modifier::BOLD)
            } else if caps.name("directive").is_some() {
                Style::default().fg(DARK_GRAY).bg(VANTABLACK)
            } else if caps.name("instr").is_some() {
                Style::default().fg(CYBER_CYAN).bg(VANTABLACK)
            } else if caps.name("reg").is_some() {
                Style::default().fg(YELLOW).bg(VANTABLACK)
            } else if caps.name("const").is_some() {
                Style::default().fg(ORANGE).bg(VANTABLACK)
            } else {
                Style::default().fg(NEON_GREEN).bg(VANTABLACK)
            };

            spans.push(Span::styled(m.as_str().to_string(), style));
            last = m.end();
        }

        if last < line.len() {
            spans.push(Span::styled(
                line[last..].to_string(),
                Style::default().fg(NEON_GREEN).bg(VANTABLACK),
            ));
        }

        if spans.is_empty() {
            spans.push(Span::styled("".to_string(), Style::default().bg(VANTABLACK)));
        }

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled("".to_string(), Style::default().bg(VANTABLACK))]));
    }

    lines
}