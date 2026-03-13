use crate::config::{Config, ProcessDef};
use crate::process::{OutputLine, ProcessHandle};
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;

const MAX_OUTPUT_LINES: usize = 1000;

pub struct ProcessEntry {
    pub key: String,
    pub def: ProcessDef,
    pub checked: bool,
    pub handle: Option<ProcessHandle>,
}

pub struct App {
    pub processes: Vec<ProcessEntry>,
    pub selected: usize,
    pub output_buffers: HashMap<String, VecDeque<String>>,
    pub output_tx: mpsc::UnboundedSender<OutputLine>,
    pub output_rx: mpsc::UnboundedReceiver<OutputLine>,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: &Config, profile: &str) -> anyhow::Result<Self> {
        let active = config.profile_processes(profile)?;
        let keys = config.ordered_keys();

        let (output_tx, output_rx) = mpsc::unbounded_channel();

        let mut processes = Vec::new();
        let mut output_buffers = HashMap::new();

        for key in &keys {
            let def = config.processes[key].clone();
            let checked = active.contains(key);
            output_buffers.insert(key.clone(), VecDeque::new());
            processes.push(ProcessEntry {
                key: key.clone(),
                def,
                checked,
                handle: None,
            });
        }

        Ok(Self {
            processes,
            selected: 0,
            output_buffers,
            output_tx,
            output_rx,
            should_quit: false,
        })
    }

    /// Start all processes that are checked but not running.
    pub fn start_checked(&mut self) {
        for entry in &mut self.processes {
            if entry.checked && entry.handle.is_none() {
                match ProcessHandle::spawn(&entry.key, &entry.def.command, self.output_tx.clone()) {
                    Ok(handle) => {
                        entry.handle = Some(handle);
                        self.output_buffers
                            .entry(entry.key.clone())
                            .or_default()
                            .push_back(format!("--- Started: {} ---", entry.def.command));
                    }
                    Err(e) => {
                        self.output_buffers
                            .entry(entry.key.clone())
                            .or_default()
                            .push_back(format!("--- Failed to start: {} ---", e));
                    }
                }
            }
        }
    }

    /// Toggle the currently selected process.
    pub async fn toggle_selected(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let entry = &mut self.processes[self.selected];
        entry.checked = !entry.checked;

        if entry.checked {
            // Start the process
            match ProcessHandle::spawn(&entry.key, &entry.def.command, self.output_tx.clone()) {
                Ok(handle) => {
                    entry.handle = Some(handle);
                    self.output_buffers
                        .entry(entry.key.clone())
                        .or_default()
                        .push_back(format!("--- Started: {} ---", entry.def.command));
                }
                Err(e) => {
                    self.output_buffers
                        .entry(entry.key.clone())
                        .or_default()
                        .push_back(format!("--- Failed to start: {} ---", e));
                }
            }
        } else {
            // Stop the process
            if let Some(mut handle) = entry.handle.take() {
                handle.stop().await;
                self.output_buffers
                    .entry(entry.key.clone())
                    .or_default()
                    .push_back("--- Stopped ---".to_string());
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.processes.len() {
            self.selected += 1;
        }
    }

    /// Drain any pending output lines into buffers.
    pub fn drain_output(&mut self) {
        while let Ok(msg) = self.output_rx.try_recv() {
            let buf = self.output_buffers.entry(msg.process_key).or_default();
            buf.push_back(msg.line);
            while buf.len() > MAX_OUTPUT_LINES {
                buf.pop_front();
            }
        }
    }

    /// Get the key of the currently selected process.
    pub fn selected_key(&self) -> Option<&str> {
        self.processes.get(self.selected).map(|e| e.key.as_str())
    }

    /// Stop all running processes.
    pub async fn stop_all(&mut self) {
        for entry in &mut self.processes {
            if let Some(mut handle) = entry.handle.take() {
                handle.stop().await;
            }
        }
    }
}
