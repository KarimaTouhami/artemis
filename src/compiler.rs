use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep_until, Instant, Duration};

#[derive(Clone)]
pub struct CompileState {
    pub file_path: String,
    pub c_content: String,
    pub asm_content: String,
    pub last_status: String,
    pub mock_rsp: u64,
    pub line_map: HashMap<usize, Vec<usize>>,
}

impl CompileState {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
            c_content: String::new(),
            asm_content: String::new(),
            last_status: "IDLE".to_string(),
            mock_rsp: 0x7fffffffe000,
            line_map: HashMap::new(),
        }
    }
}

pub struct Compiler {
    pub state: Arc<RwLock<CompileState>>,
}

impl Compiler {
    pub fn new(state: Arc<RwLock<CompileState>>) -> Self {
        Self { state }
    }

    pub async fn compile(&self) -> Result<()> {
        let file_path = {
            let state = self.state.read().await;
            state.file_path.clone()
        };

        let c_content = fs::read_to_string(&file_path)
            .context("Failed to read C source file")?;

        let base_name = Path::new(&file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid file name")?;
        
        let asm_path = format!("{}.s", base_name);

        let output = Command::new("gcc")
            .args([
                "-S",
                "-masm=intel",
                "-fno-stack-protector",
                "-g",
                "-O0",
                &file_path,
                "-o",
                &asm_path,
            ])
            .output()
            .context("Failed to execute GCC")?;

        let status = if output.status.success() {
            "SUCCESS"
        } else {
            "ERROR"
        };

        let asm_content = if output.status.success() {
            fs::read_to_string(&asm_path)
                .context("Failed to read assembly output")?
        } else {
            String::from_utf8_lossy(&output.stderr).to_string()
        };

        let line_map = if output.status.success() {
            Self::parse_loc_directives(&asm_content)
        } else {
            HashMap::new()
        };

        let mut state = self.state.write().await;
        state.c_content = c_content;
        state.asm_content = asm_content;
        state.last_status = status.to_string();
        state.line_map = line_map;
        state.mock_rsp = state.mock_rsp.wrapping_sub(8);

        Ok(())
    }

    fn parse_loc_directives(asm_content: &str) -> HashMap<usize, Vec<usize>> {
        let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
        
        for (asm_line_idx, line) in asm_content.lines().enumerate() {
            if let Some(c_line) = Self::extract_loc_line(line) {
                map.entry(c_line)
                    .or_insert_with(Vec::new)
                    .push(asm_line_idx);
            }
        }
        
        map
    }

    fn extract_loc_line(line: &str) -> Option<usize> {
        let trimmed = line.trim();
        
        if trimmed.starts_with(".loc") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                return parts[2].parse::<usize>().ok();
            }
        }
        
        None
    }
}

pub async fn spawn_compiler_worker(
    mut source_rx: mpsc::Receiver<String>,
    asm_tx: mpsc::Sender<String>,
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
                    let asm = compile_to_asm(src).await.unwrap_or_else(|e| format!("; compile failed: {}", e));
                    let _ = asm_tx.send(asm).await;
                }
            }
        }
    }
}

async fn compile_to_asm(src: String) -> Result<String, String> {
    let mut child = tokio::process::Command::new("gcc")
        .arg("-x")
        .arg("c")
        .arg("-")
        .arg("-S")
        .arg("-masm=intel")
        .arg("-fno-stack-protector")
        .arg("-O0")
        .arg("-g")
        .arg("-o")
        .arg("-")
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
    Ok(out)
}
