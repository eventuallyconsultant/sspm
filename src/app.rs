use crate::config::{Config, ProcessDef};
use crate::process::{OutputLine, ProcessHandle};
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;

const MAX_OUTPUT_LINES: usize = 1000;

#[derive(Clone, Copy, PartialEq)]
pub enum ProcessStatus {
  Stopped,
  Running,
  Failed,
}

pub struct ProcessEntry {
  pub key: String,
  pub def: ProcessDef,
  pub checked: bool,
  pub status: ProcessStatus,
  pub handle: Option<ProcessHandle>,
  pub last_exit_code: Option<i32>,
}

pub struct App {
  pub processes: Vec<ProcessEntry>,
  pub selected: usize,
  pub output_buffers: HashMap<String, VecDeque<String>>,
  pub output_tx: mpsc::UnboundedSender<OutputLine>,
  pub output_rx: mpsc::UnboundedReceiver<OutputLine>,
  pub should_quit: bool,
  pub log_scroll: usize,
  pub frozen: bool,
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
      processes.push(ProcessEntry { key: key.clone(), def, checked, status: ProcessStatus::Stopped, handle: None, last_exit_code: None });
    }

    Ok(Self { processes, selected: 0, output_buffers, output_tx, output_rx, should_quit: false, log_scroll: 0, frozen: false })
  }

  fn start_process(&mut self, idx: usize) {
    let entry = &mut self.processes[idx];
    entry.last_exit_code = None;
    match ProcessHandle::spawn(&entry.key, &entry.def.command, self.output_tx.clone()) {
      Ok(handle) => {
        entry.handle = Some(handle);
        entry.status = ProcessStatus::Running;
        self
          .output_buffers
          .entry(entry.key.clone())
          .or_default()
          .push_back(format!("--- Started: {} ---", entry.def.command));
      }
      Err(e) => {
        entry.status = ProcessStatus::Failed;
        self
          .output_buffers
          .entry(entry.key.clone())
          .or_default()
          .push_back(format!("--- Failed to start: {} ---", e));
      }
    }
  }

  /// Start all processes that are checked but not running.
  pub fn start_checked(&mut self) {
    let indices: Vec<usize> = self
      .processes
      .iter()
      .enumerate()
      .filter(|(_, e)| e.checked && e.handle.is_none())
      .map(|(i, _)| i)
      .collect();
    for idx in indices {
      self.start_process(idx);
    }
  }

  /// Toggle the currently selected process.
  pub async fn toggle_selected(&mut self) {
    if self.processes.is_empty() {
      return;
    }

    let is_running = self.processes[self.selected].handle.is_some();

    if is_running {
      // Running → stop and uncheck
      let entry = &mut self.processes[self.selected];
      if let Some(mut handle) = entry.handle.take() {
        handle.stop().await;
      }
      entry.checked = false;
      entry.status = ProcessStatus::Stopped;
      self.output_buffers.entry(entry.key.clone()).or_default().push_back("--- Stopped ---".to_string());
    } else if self.processes[self.selected].checked {
      // Checked but dead (failed/exited) → restart
      self.start_process(self.selected);
    } else {
      // Unchecked → check and start
      self.processes[self.selected].checked = true;
      self.start_process(self.selected);
    }
  }

  pub fn move_up(&mut self) {
    if self.selected > 0 {
      self.selected -= 1;
      self.log_scroll = 0;
    }
  }

  pub fn move_down(&mut self) {
    if self.selected + 1 < self.processes.len() {
      self.selected += 1;
      self.log_scroll = 0;
    }
  }

  pub fn scroll_logs_up(&mut self) {
    self.log_scroll = self.log_scroll.saturating_add(3);
  }

  pub fn scroll_logs_down(&mut self) {
    self.log_scroll = self.log_scroll.saturating_sub(3);
  }

  /// Drain any pending output lines into buffers, and poll child exit status.
  pub fn drain_output(&mut self) {
    while let Ok(msg) = self.output_rx.try_recv() {
      let buf = self.output_buffers.entry(msg.process_key).or_default();
      buf.push_back(msg.line);
      while buf.len() > MAX_OUTPUT_LINES {
        buf.pop_front();
      }
    }

    // Poll running processes for exit
    for entry in &mut self.processes {
      if let Some(ref mut handle) = entry.handle {
        match handle.try_wait() {
          Ok(Some(status)) => {
            let code = status.code();
            let success = status.success();
            entry.handle = None;
            if success {
              entry.status = ProcessStatus::Stopped;
              entry.last_exit_code = None;
              self
                .output_buffers
                .entry(entry.key.clone())
                .or_default()
                .push_back("--- Exited (0) ---".to_string());
            } else {
              entry.status = ProcessStatus::Failed;
              entry.last_exit_code = code;
              self
                .output_buffers
                .entry(entry.key.clone())
                .or_default()
                .push_back(format!("--- Failed (exit {}) ---", code.map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string())));
            }
          }
          Ok(None) => {} // still running
          Err(_) => {}
        }
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
