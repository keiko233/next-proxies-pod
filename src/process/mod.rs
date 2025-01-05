use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct ProcessManager {
    pid: Arc<Mutex<Option<u32>>>,
    config_path: PathBuf,
    logout: Option<bool>,
}

impl ProcessManager {
    pub fn new(config_path: PathBuf, logout: Option<bool>) -> Self {
        Self {
            pid: Arc::new(Mutex::new(None)),
            config_path,
            logout,
        }
    }

    /// Starts the sing-box process.
    pub async fn start(&self) -> io::Result<()> {
        let current_dir_singbox = std::env::current_dir()?.join("sing-box");

        let mut command = if current_dir_singbox.exists() {
            Command::new(current_dir_singbox)
        } else {
            Command::new("sing-box")
        };

        let mut child = command
            .args(&["run", "-c", self.config_path.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        info!("sing-box process started");

        let pid = child.id();
        {
            let mut pid_guard = self.pid.lock().await;
            *pid_guard = pid;
        }

        let stdout = child.stdout.take().expect("Failed to take stdout");
        let stderr = child.stderr.take().expect("Failed to take stderr");

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

        // -------------------------------------------------------------------------
        // Background task that periodically checks if the child is still alive
        // without calling .take() or .wait().
        // -------------------------------------------------------------------------
        let child_arc = Arc::new(Mutex::new(Some(child)));
        let pid_ref = self.pid.clone();
        tokio::spawn(async move {
            // hold the unique ownership of child
            let mut guard = child_arc.lock().await;
            if let Some(mut ch) = guard.take() {
                match ch.wait().await {
                    Ok(status) => info!("sing-box process exited with status: {}", status),
                    Err(e) => error!("Failed to wait on sing-box: {}", e),
                }
                // process has exited, clean up the PID
                let mut pid_guard = pid_ref.lock().await;
                *pid_guard = None;
            }
        });

        Ok(())
    }

    /// Stops the sing-box process.
    pub async fn stop(&self) -> io::Result<()> {
        let pid = *self.pid.lock().await;
        if let Some(pid) = pid {
            info!("Stopping sing-box process (pid={}) ...", pid);

            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            }

            #[cfg(windows)]
            {
                // use Windows native API TerminateProcess
                use winapi::um::handleapi::CloseHandle;
                use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
                use winapi::um::winnt::PROCESS_TERMINATE;

                unsafe {
                    let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
                    if handle.is_null() {
                        error!("OpenProcess failed (PID={}), maybe it's already gone.", pid);
                    } else {
                        if TerminateProcess(handle, 1) == 0 {
                            error!(
                                "TerminateProcess failed, last_error={}",
                                std::io::Error::last_os_error()
                            );
                        } else {
                            info!("TerminateProcess success for PID={}", pid);
                        }
                        CloseHandle(handle);
                    }
                }
            }
        } else {
            info!("stop() called, but no sing-box process is running");
        }

        Ok(())
    }

    /// Reloads sing-box by sending a SIGHUP signal on Unix systems.
    /// For non-Unix, it stops and restarts the process.
    #[cfg(unix)]
    pub async fn reload(&self) -> io::Result<()> {
        let pid = *self.pid.lock().await;
        if let Some(pid) = pid {
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                if let Err(e) = kill(Pid::from_raw(pid as i32), Signal::SIGHUP) {
                    error!("Failed to send SIGHUP: {}", e);
                    return Err(io::Error::new(io::ErrorKind::Other, e));
                }
                info!("Sent reload signal (SIGHUP) to sing-box");
                Ok(())
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No running sing-box process found",
            ))
        }
    }
    /// Reloads sing-box by sending a SIGHUP signal on Unix systems.
    /// For WIndows, not SIGHUP, use stop + start
    #[cfg(windows)]
    pub async fn reload(&self) -> io::Result<()> {
        info!("Reload on Windows -> stop + start");
        self.stop().await?;
        // Add a small delay to ensure the previous process is fully stopped
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        self.start().await
    }

    pub async fn is_running(&self) -> bool {
        let pid = *self.pid.lock().await;
        pid.is_some()
    }
}
