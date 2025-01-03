use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct ProcessManager {
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

    /// starting the sing-box process
    pub async fn start(&self) -> io::Result<()> {
        let mut child = Command::new("sing-box")
            .args(&["run", "-c", self.config_path.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        info!("sing-box process started");

        let stdout = child.stdout.take().expect("Failed to take stdout");
        let stderr = child.stderr.take().expect("Failed to take stderr");

        // save the child process handle for later use
        {
            let mut lock = self.child.lock().unwrap();
            *lock = Some(child);
        }

        // create a clone of self.child to be used in the background tasks
        let child_ref = self.child.clone();

        let logout = self.logout;
        let _stdout_task = tokio::spawn(async move {
            let mut stdout_reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = stdout_reader.next_line().await {
                match logout {
                    Some(true) => {
                        info!("sing-box STDOUT: {}", line);
                    }
                    _ => {}
                }
            }
            debug!("stdout_task finished reading");
        });

        // sing box log default output to stderr???
        let logout = self.logout;
        let _stderr_task = tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr_reader.next_line().await {
                match logout {
                    Some(true) => {
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
                    _ => {}
                }
            }
            debug!("stderr_task finished reading");
        });

        // create a background task to wait for the child process to exit
        tokio::spawn(async move {
            // get the child process handle from child_ref and wait for it to exit
            let maybe_child = { child_ref.lock().unwrap().take() };
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

    pub async fn stop(&self) -> io::Result<()> {
        let mut lock = self.child.lock().unwrap();
        if let Some(child) = lock.as_mut() {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;
                if let Some(id) = child.id() {
                    let pid = Pid::from_raw(id as i32);
                    let _ = kill(pid, Signal::SIGTERM);
                }
            }

            match child.wait().await {
                Ok(status) => {
                    info!("sing-box process exited with status: {}", status);
                }
                Err(e) => {
                    error!("Failed to wait on sing-box: {}", e);
                }
            }

            *lock = None;
        }
        Ok(())
    }

    pub async fn reload(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            // https://github.com/SagerNet/sing-box/commit/88469d4aaa3eff427bd5a719a3132dbedfabcc32
            let lock = self.child.lock().unwrap();
            if let Some(child) = lock.as_ref() {
                if let Some(id) = child.id() {
                    use nix::sys::signal::{Signal, kill};
                    use nix::unistd::Pid;
                    let pid = Pid::from_raw(id as i32);
                    // Send SIGHUP signal to trigger reload
                    if let Err(e) = kill(pid, Signal::SIGHUP) {
                        error!("Failed to send SIGHUP to sing-box: {}", e);
                        return Err(io::Error::new(io::ErrorKind::Other, e));
                    }
                    info!("Sent reload signal to sing-box");
                    return Ok(());
                }
            }
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No running sing-box process found",
            ))
        }
        #[cfg(not(unix))]
        {
            // For non-unix systems, not supported, stop and start the service instead of reload
            self.stop().await?;
            self.start().await
        }
    }

    pub fn is_running(&self) -> bool {
        let lock = self.child.lock().unwrap();
        if let Some(child) = lock.as_ref() {
            child.id().is_some()
        } else {
            false
        }
    }
}
