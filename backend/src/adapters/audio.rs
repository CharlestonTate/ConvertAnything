use super::run_cmd;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(60);

pub async fn convert(input: &Path, output: &Path) -> Result<PathBuf, String> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let input = input.to_string_lossy();
    let output_s = output.to_string_lossy();
    run_cmd(
        "ffmpeg",
        &[
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            input.as_ref(),
            output_s.as_ref(),
        ],
        &[],
        TIMEOUT,
    )
    .await
    .map_err(String::from)?;
    if !output.exists() {
        return Err("ffmpeg did not write an output file".into());
    }
    Ok(output.to_path_buf())
}
