use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(60);

pub fn convert_blocking(input: &Path, output: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let input_s = input.to_string_lossy().into_owned();
    let output_s = output.to_string_lossy().into_owned();

    match run("vips", &["copy", &input_s, &output_s]) {
        Ok(()) if output.exists() => return Ok(output.to_path_buf()),
        Ok(()) => return Err("vips did not write an output file".into()),
        Err(err) if err.not_found => {
            tracing::warn!("vips not found; converting image with ffmpeg");
        }
        Err(err) => return Err(err.message),
    }

    run("ffmpeg", &["-y", "-hide_banner", "-loglevel", "error", "-i", &input_s, &output_s])
        .map_err(|e| e.message)?;
    if !output.exists() {
        return Err("ffmpeg did not write an output file".into());
    }
    Ok(output.to_path_buf())
}

struct ToolError {
    not_found: bool,
    message: String,
}

fn run(program: &str, args: &[&str]) -> Result<(), ToolError> {
    tracing::info!(program, ?args, "running image conversion tool");
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| ToolError {
            not_found: err.kind() == ErrorKind::NotFound,
            message: format!("failed to start {program}: {err}"),
        })?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut err = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut err);
                }
                return Err(ToolError {
                    not_found: false,
                    message: format!("{program} failed: {}", err.trim()),
                });
            }
            Ok(None) => {
                if start.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    return Err(ToolError {
                        not_found: false,
                        message: format!("{program} timed out"),
                    });
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => {
                return Err(ToolError {
                    not_found: false,
                    message: format!("{program} wait failed: {err}"),
                });
            }
        }
    }
}
