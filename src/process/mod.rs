use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct ProcessManager {
    /// Use tokio::sync::Mutex to ensure thread-safe access in an async environment.
    pub child: Arc<Mutex<Option<Child>>>,
    config_path: PathBuf,
    logout: Option<bool>,
}

impl ProcessManager {
    pub fn new(config_path: PathBuf, logout: Option<bool>) -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            config_path,
            logout,
        }
    }

    /// Starts the sing-box process.
    pub async fn start(&self) -> io::Result<()> {
        let mut child = Command::new("sing-box")
            .args(&["run", "-c", self.config_path.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        info!("sing-box process started");

        let stdout = child.stdout.take().expect("Failed to take stdout");
        let stderr = child.stderr.take().expect("Failed to take stderr");

        {
            // Lock the mutex asynchronously to store the Child
            let mut guard = self.child.lock().await;
            *guard = Some(child);
        }

        // Copy logout value for logging tasks
        let logout = self.logout;
        let _stdout_task = tokio::spawn(async move {
            let mut stdout_reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                if let Some(true) = logout {
                    info!("sing-box STDOUT: {}", line);
                }
            }
            debug!("stdout_task finished reading");
        });

        let logout = self.logout;
        let _stderr_task = tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                if let Some(true) = logout {
                    // Simple string checks to categorize logs
                    if line.contains("INFO") {
                        info!("sing-box: {}", line);
                    } else if line.contains("WARN") {
                        warn!("sing-box: {}", line);
                    } else if line.contains("ERROR") {
                        error!("sing-box: {}", line);
                    } else if line.contains("DEBUG") {
                        debug!("sing-box: {}", line);
                    } else {
                        info!("sing-box: {}", line);
                    }
                }
            }
            debug!("stderr_task finished reading");
        });

        // Background task to wait for the child process to exit
        let child_ref = self.child.clone();
        tokio::spawn(async move {
            // Take the process handle out of the mutex to wait on it
            let maybe_child = child_ref.lock().await.take();
            if let Some(mut ch) = maybe_child {
                match ch.wait().await {
                    Ok(status) => {
                        info!("sing-box process exited with status: {}", status);
                    }
                    Err(e) => {
                        error!("Failed to wait on sing-box: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Stops the sing-box process.
    pub async fn stop(&self) -> io::Result<()> {
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            #[cfg(unix)]
            {
                // Send SIGTERM on Unix
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                if let Some(id) = child.id() {
                    let pid = Pid::from_raw(id as i32);
                    let _ = kill(pid, Signal::SIGTERM);
                }
            }

            // Wait for the process to exit
            match child.wait().await {
                Ok(status) => {
                    info!("sing-box process exited with status: {}", status);
                }
                Err(e) => {
                    error!("Failed to wait on sing-box: {}", e);
                }
            }

            *guard = None;
        }
        Ok(())
    }

    /// Reloads sing-box by sending a SIGHUP signal on Unix systems.
    /// For non-Unix, it stops and restarts the process.
    pub async fn reload(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let guard = self.child.lock().await;
            if let Some(child) = guard.as_ref() {
                if let Some(id) = child.id() {
                    use nix::sys::signal::{Signal, kill};
                    use nix::unistd::Pid;
                    let pid = Pid::from_raw(id as i32);
                    // Send SIGHUP to trigger reload on Unix
                    if let Err(e) = kill(pid, Signal::SIGHUP) {
                        error!("Failed to send SIGHUP to sing-box: {}", e);
                        return Err(io::Error::new(io::ErrorKind::Other, e));
                    }
                    info!("Sent reload signal to sing-box");
                    return Ok(());
                }
            }
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No running sing-box process found",
            ));
        }
        #[cfg(not(unix))]
        {
            // For non-Unix platforms, reload is not supported. Stop and then start again.
            self.stop().await?;
            self.start().await
        }
    }

    /// Checks if sing-box is running.
    ///
    /// Using `try_lock()` to avoid blocking; returns false if the mutex is currently locked.
    /// If you need guaranteed accuracy in async context, change to an async fn using `lock().await`.
    pub fn is_running(&self) -> bool {
        if let Ok(guard) = self.child.try_lock() {
            guard.as_ref().map(|c| c.id().is_some()).unwrap_or(false)
        } else {
            false
        }
    }
}
