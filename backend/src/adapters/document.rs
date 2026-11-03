use super::{run_cmd, CmdError};
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(120);

fn convert_to_arg(target_ext: &str) -> String {
    match target_ext {
        "txt" => "txt:Text".to_string(),
        "doc" => "doc:MS Word 97".to_string(),
        other => other.to_string(),
    }
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn soffice_candidates() -> Vec<PathBuf> {
    let mut bins = Vec::new();
    if let Ok(path) = std::env::var("SOFFICE_PATH") {
        bins.push(PathBuf::from(path));
    }
    bins.push(PathBuf::from("soffice"));
    bins.push(PathBuf::from("libreoffice"));
    bins.push(PathBuf::from(r"C:\Program Files\LibreOffice\program\soffice.exe"));
    bins.push(PathBuf::from(r"C:\Program Files (x86)\LibreOffice\program\soffice.exe"));
    bins
}

pub async fn convert(input: &Path, staging_dir: &Path, target_ext: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    let target_ext = target_ext.to_ascii_lowercase();
    let source_ext = ext_of(input);
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let dest = staging_dir.join(format!("{stem}.{target_ext}"));

    if source_ext == target_ext {
        tokio::fs::copy(input, &dest)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(dest);
    }

    match convert_with_libreoffice(input, staging_dir, &target_ext).await {
        Ok(path) => Ok(path),
        Err(err) if err == "LibreOffice not found" => {
            tracing::warn!("LibreOffice not found; converting document with Microsoft Office");
            convert_with_msoffice(input, &dest, &source_ext, &target_ext).await
        }
        Err(err) => Err(err),
    }
}

async fn convert_with_libreoffice(
    input: &Path,
    staging_dir: &Path,
    target_ext: &str,
) -> Result<PathBuf, String> {
    let home = staging_dir
        .parent()
        .unwrap_or(staging_dir)
        .join(".lo-home");
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;

    let input_s = input.to_string_lossy();
    let out_s = staging_dir.to_string_lossy();
    let convert_to = convert_to_arg(target_ext);
    let home_s = home.to_string_lossy();
    let args = [
        "--headless",
        "--nologo",
        "--nofirststartwizard",
        "--norestore",
        "--convert-to",
        convert_to.as_str(),
        "--outdir",
        out_s.as_ref(),
        input_s.as_ref(),
    ];
    let env = [("HOME", home_s.as_ref()), ("SAL_USE_VCLPLUGIN", "svp")];

    for bin in soffice_candidates() {
        if bin.is_absolute() && !bin.exists() {
            continue;
        }
        let program = bin.to_string_lossy();
        match run_cmd(program.as_ref(), &args, &env, TIMEOUT).await {
            Ok(_) => {
                return find_output(staging_dir, target_ext);
            }
            Err(CmdError::NotFound { .. }) => continue,
            Err(err) => return Err(err.to_string()),
        }
    }
    Err("LibreOffice not found".into())
}

fn find_output(staging_dir: &Path, target_ext: &str) -> Result<PathBuf, String> {
    std::fs::read_dir(staging_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case(target_ext))
                    .unwrap_or(false)
        })
        .ok_or_else(|| "LibreOffice produced no output file".to_string())
}

struct OfficePlan {
    app: &'static str,
    format: i32,
    export_pdf: bool,
}

fn office_plan(source_ext: &str, target_ext: &str) -> Result<OfficePlan, String> {
    let app = match source_ext {
        "xlsx" => "excel",
        "pptx" => "ppt",
        _ => "word",
    };

    let (format, export_pdf) = match (app, target_ext) {
        (_, "pdf") => (17, true),
        ("word", "docx") => (16, false),
        ("word", "doc") => (0, false),
        ("word", "odt") => (23, false),
        ("word", "rtf") => (6, false),
        ("word", "txt") => (2, false),
        ("excel", "xlsx") => (51, false),
        ("excel", "txt") => (42, false),
        ("ppt", "pptx") => (24, false),
        _ => {
            return Err(format!(
                "Microsoft Office cannot convert {source_ext} to {target_ext} without LibreOffice"
            ));
        }
    };

    Ok(OfficePlan {
        app,
        format,
        export_pdf,
    })
}

async fn convert_with_msoffice(
    input: &Path,
    dest: &Path,
    source_ext: &str,
    target_ext: &str,
) -> Result<PathBuf, String> {
    let plan = office_plan(source_ext, target_ext)?;
    let script = dest
        .parent()
        .unwrap_or(dest)
        .join("msoffice_convert.ps1");
    tokio::fs::write(&script, include_str!("msoffice_convert.ps1"))
        .await
        .map_err(|e| e.to_string())?;

    let mut args = vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-STA".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        script.to_string_lossy().into_owned(),
        "-InputPath".to_string(),
        input.to_string_lossy().into_owned(),
        "-OutputPath".to_string(),
        dest.to_string_lossy().into_owned(),
        "-App".to_string(),
        plan.app.to_string(),
        "-Format".to_string(),
        plan.format.to_string(),
    ];
    if plan.export_pdf {
        args.push("-ExportPdf".to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cmd("powershell", &arg_refs, &[], TIMEOUT)
        .await
        .map_err(String::from)?;

    if !dest.exists() {
        return Err("Microsoft Office did not write an output file".into());
    }
    Ok(dest.to_path_buf())
}
