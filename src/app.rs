use crate::config::{Config, ProcessDef};
use crate::process::{OutputLine, ProcessHandle, is_group_alive};
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;

const MAX_OUTPUT_LINES: usize = 1000;

#[derive(Clone, Copy, PartialEq)]
pub enum ProcessStatus {
  Stopped,
  Running,
  Stopping,
  Failed,
}

pub struct ProcessEntry {
  pub key: String,
  pub def: ProcessDef,
  pub checked: bool,
  pub status: ProcessStatus,
  pub handle: Option<ProcessHandle>,
  pub stopping_pid: Option<u32>,
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
      processes.push(ProcessEntry { key: key.clone(), def, checked, status: ProcessStatus::Stopped, handle: None, stopping_pid: None, last_exit_code: None });
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
  pub fn toggle_selected(&mut self) {
    if self.processes.is_empty() {
      return;
    }

    let idx = self.selected;
    match self.processes[idx].status {
      ProcessStatus::Running => {
        // Running → send SIGTERM, mark as Stopping (checkbox stays until exit)
        if let Some(ref handle) = self.processes[idx].handle {
          handle.signal_term();
        }
        self.processes[idx].status = ProcessStatus::Stopping;
        let key = self.processes[idx].key.clone();
        self.output_buffers.entry(key).or_default().push_back("--- Stopping... ---".to_string());
      }
      ProcessStatus::Stopping => {
        // Stopping → force kill (handle or orphaned group)
        if let Some(ref handle) = self.processes[idx].handle {
          handle.force_kill();
        } else if let Some(pid) = self.processes[idx].stopping_pid {
          unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
        }
        let key = self.processes[idx].key.clone();
        self.output_buffers.entry(key).or_default().push_back("--- Force killing... ---".to_string());
      }
      ProcessStatus::Stopped | ProcessStatus::Failed => {
        if !self.processes[idx].checked {
          self.processes[idx].checked = true;
        }
        self.start_process(idx);
      }
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

  pub fn clear_selected_output(&mut self) {
    if let Some(key) = self.selected_key().map(str::to_owned) {
      if let Some(buf) = self.output_buffers.get_mut(&key) {
        buf.clear();
      }
      self.log_scroll = 0;
    }
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
            let was_stopping = entry.status == ProcessStatus::Stopping;
            let pid = handle.pid();
            let code = status.code();
            let success = status.success();
            entry.handle = None;
            if was_stopping {
              // Shell exited, but children in the group may still be alive
              entry.stopping_pid = Some(pid);
              // Don't transition yet — the group-alive check below will handle it
            } else if success {
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

      // Check if the process group is fully dead for Stopping processes
      if let Some(pid) = entry.stopping_pid {
        if !is_group_alive(pid) {
          entry.stopping_pid = None;
          entry.status = ProcessStatus::Stopped;
          entry.checked = false;
          entry.last_exit_code = None;
          self.output_buffers.entry(entry.key.clone()).or_default().push_back("--- Stopped ---".to_string());
        }
      }
    }
  }

  /// Get the key of the currently selected process.
  pub fn selected_key(&self) -> Option<&str> {
    self.processes.get(self.selected).map(|e| e.key.as_str())
  }

  /// Send SIGTERM to all running processes and mark them as Stopping.
  pub fn request_quit(&mut self) {
    self.should_quit = true;
    for entry in &mut self.processes {
      if entry.status == ProcessStatus::Running {
        if let Some(ref handle) = entry.handle {
          handle.signal_term();
        }
        entry.status = ProcessStatus::Stopping;
      }
    }
  }

  /// Force-kill all remaining processes.
  pub fn force_quit(&mut self) {
    for entry in &mut self.processes {
      if let Some(ref handle) = entry.handle {
        handle.force_kill();
      } else if let Some(pid) = entry.stopping_pid {
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
      }
    }
  }

  /// True when no process has a handle and no group is still alive.
  pub fn all_stopped(&self) -> bool {
    self.processes.iter().all(|e| e.handle.is_none() && e.stopping_pid.is_none())
  }
}
