use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    state: Arc<RwLock<CompileState>>,
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
