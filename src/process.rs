use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct ProcessHandle {
  child: Child,
  pid: u32,
}

/// Message sent from a process reader task to the main app.
pub struct OutputLine {
  pub process_key: String,
  pub line: String,
}

impl ProcessHandle {
  /// Spawn a process and start reading its stdout+stderr, sending lines to `tx`.
  pub fn spawn(key: &str, command: &str, tx: mpsc::UnboundedSender<OutputLine>) -> std::io::Result<Self> {
    let mut child = Command::new("sh")
      .arg("-c")
      .arg(command)
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .process_group(0) // own process group so we can signal the whole tree
      .spawn()?;

    // Read stdout
    if let Some(stdout) = child.stdout.take() {
      let tx_clone = tx.clone();
      let key_clone = key.to_string();
      tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
          let _ = tx_clone.send(OutputLine { process_key: key_clone.clone(), line });
        }
      });
    }

    // Read stderr
    if let Some(stderr) = child.stderr.take() {
      let tx_clone = tx;
      let key_clone = key.to_string();
      tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
          let _ = tx_clone.send(OutputLine { process_key: key_clone.clone(), line });
        }
      });
    }

    let pid = child.id().expect("child should have a pid");
    Ok(Self { child, pid })
  }

  pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
    self.child.try_wait()
  }

  pub fn pid(&self) -> u32 {
    self.pid
  }

  /// Send SIGTERM to the process group (graceful shutdown).
  pub fn signal_term(&self) {
    unsafe {
      libc::kill(-(self.pid as i32), libc::SIGTERM);
    }
  }

  /// Send SIGKILL to the process group (force kill).
  pub fn force_kill(&self) {
    unsafe {
      libc::kill(-(self.pid as i32), libc::SIGKILL);
    }
  }
}

/// Check if a process group is still alive (any member running).
pub fn is_group_alive(pid: u32) -> bool {
  unsafe { libc::kill(-(pid as i32), 0) == 0 }
}
