use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct ProcessHandle {
    child: Child,
}

/// Message sent from a process reader task to the main app.
pub struct OutputLine {
    pub process_key: String,
    pub line: String,
}

impl ProcessHandle {
    /// Spawn a process and start reading its stdout+stderr, sending lines to `tx`.
    pub fn spawn(
        key: &str,
        command: &str,
        tx: mpsc::UnboundedSender<OutputLine>,
    ) -> std::io::Result<Self> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Read stdout
        if let Some(stdout) = child.stdout.take() {
            let tx_clone = tx.clone();
            let key_clone = key.to_string();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx_clone.send(OutputLine {
                        process_key: key_clone.clone(),
                        line,
                    });
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
                    let _ = tx_clone.send(OutputLine {
                        process_key: key_clone.clone(),
                        line,
                    });
                }
            });
        }

        Ok(Self { child })
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    pub async fn stop(&mut self) {
        let _ = self.child.kill().await;
    }
}
