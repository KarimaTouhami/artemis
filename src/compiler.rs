use anyhow::Result;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::time::{sleep_until, Instant, Duration};

#[derive(Debug)]
pub struct CompileOutput {
    pub asm_text: String,
    pub line_map: HashMap<usize, Vec<usize>>,
}

fn extract_loc_line(line: &str) -> Option<usize> {
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with(".loc") {
        return None;
    }
    parts.next();
    parts.next()?.parse().ok()
}

pub async fn spawn_compiler_worker(
    mut source_rx: mpsc::Receiver<String>,
    asm_tx: mpsc::Sender<CompileOutput>,
) {
    let mut pending: Option<String> = None;
    let mut deadline: Option<Instant> = None;

    loop {
        tokio::select! {
            maybe = source_rx.recv() => {
                match maybe {
                    Some(src) => {
                        pending = Some(src);
                        deadline = Some(Instant::now() + Duration::from_millis(300));
                    }
                    None => break,
                }
            }
            _ = async {
                if let Some(dl) = deadline {
                    sleep_until(dl).await;
                    true
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(src) = pending.take() {
                    deadline = None;
                    let output = compile_to_asm(src).await.unwrap_or_else(|e| CompileOutput {
                        asm_text: format!("; compile failed: {}", e),
                        line_map: HashMap::new(),
                    });
                    let _ = asm_tx.send(output).await;
                }
            }
        }
    }
}

async fn compile_to_asm(src: String) -> Result<CompileOutput, String> {
    let mut child = tokio::process::Command::new("gcc")
        .args(["-x", "c", "-", "-S", "-masm=intel", "-fno-stack-protector", "-O0", "-g", "-o", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn error: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(src.as_bytes()).await.map_err(|e| format!("stdin write: {}", e))?;
    }

    let output = child.wait_with_output().await.map_err(|e| format!("wait error: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gcc failed: {}", stderr));
    }
    let out = String::from_utf8_lossy(&output.stdout).to_string();
    let (cleaned, index_map) = clean_assembly(&out);
    let adjusted_line_map = build_loc_instruction_map(&out, &index_map);

    Ok(CompileOutput {
        asm_text: cleaned,
        line_map: adjusted_line_map,
    })
}

fn build_loc_instruction_map(asm: &str, index_map: &HashMap<usize, usize>) -> HashMap<usize, Vec<usize>> {
    let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut current_c_line: Option<usize> = None;

    for (old_idx, line) in asm.lines().enumerate() {
        if let Some(c_line) = extract_loc_line(line) {
            current_c_line = Some(c_line);
            continue;
        }

        if let (Some(c_line), Some(&new_idx)) = (current_c_line, index_map.get(&old_idx)) {
            map.entry(c_line).or_default().push(new_idx);
        }
    }

    map
}

/// Filters out debug metadata and clutter from assembly output
/// Keeps: instructions, labels, and code-relevant comments
/// Removes: .loc, .file, .type, .globl, .size, .cfi_*, .p2align, etc.
/// Returns: (cleaned_asm, index_mapping) where mapping[old_idx] = new_idx
fn clean_assembly(asm: &str) -> (String, HashMap<usize, usize>) {
    let mut result = Vec::new();
    let mut index_map = HashMap::new();
    let mut new_line_idx = 0;

    for (old_idx, line) in asm.lines().enumerate() {
        let trimmed = line.trim();
        
        // Skip empty lines
        if trimmed.is_empty() {
            result.push(String::new());
            index_map.insert(old_idx, new_line_idx);
            new_line_idx += 1;
            continue;
        }

        // Skip all directives that start with '.'
        if trimmed.starts_with('.') {
            continue;
        }

        // Skip pure comment lines (starting with semicolon/comment)
        if trimmed.starts_with(';') && !trimmed.contains(':') {
            continue;
        }

        // Normalize tabs/control chars to keep terminal rendering stable while scrolling.
        let normalized = line
            .replace('\t', "    ")
            .chars()
            .filter(|c| *c == '\n' || *c == '\r' || !c.is_control())
            .collect::<String>();

        // Keep everything else: instructions, labels, and inline comments
        result.push(normalized);
        index_map.insert(old_idx, new_line_idx);
        new_line_idx += 1;
    }

    // Remove trailing empty lines
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }

    (result.join("\n"), index_map)
}
