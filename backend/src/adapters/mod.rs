pub mod audio;
pub mod document;
pub mod image;

use std::fmt;
use std::io::ErrorKind;
use std::process::Output;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug)]
pub enum CmdError {
    NotFound { program: String, message: String },
    Failed(String),
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmdError::NotFound { program, message } => {
                write!(f, "failed to start {program}: {message}")
            }
            CmdError::Failed(message) => write!(f, "{message}"),
        }
    }
}

impl From<CmdError> for String {
    fn from(err: CmdError) -> Self {
        err.to_string()
    }
}

pub async fn run_cmd(
    program: &str,
    args: &[&str],
    extra_env: &[(&str, &str)],
    max: Duration,
) -> Result<Output, CmdError> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    tracing::info!(program, ?args, "running conversion tool");
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Err(CmdError::NotFound {
                program: program.to_string(),
                message: err.to_string(),
            });
        }
        Err(err) => {
            return Err(CmdError::Failed(format!("failed to start {program}: {err}")));
        }
    };
    match timeout(max, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(output)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(CmdError::Failed(
                    format!("{program} failed (status {}): {} {}", output.status, stderr, stdout)
                        .trim()
                        .to_string(),
                ))
            }
        }
        Ok(Err(err)) => Err(CmdError::Failed(format!("failed to wait for {program}: {err}"))),
        Err(_) => Err(CmdError::Failed(format!("{program} timed out"))),
    }
}
