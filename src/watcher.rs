use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::compiler::{CompileState, Compiler};

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn new(file_path: String, state: Arc<RwLock<CompileState>>) -> Result<Self> {
        let compiler = Compiler::new(state.clone());
        
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                    ) {
                        let compiler = compiler.clone();
                        tokio::spawn(async move {
                            let _ = compiler.compile().await;
                        });
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(Path::new(&file_path), RecursiveMode::NonRecursive)?;

        Ok(Self { _watcher: watcher })
    }
}

impl Compiler {
    pub fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}
