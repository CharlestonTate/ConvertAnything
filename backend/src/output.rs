use crate::formats;
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

#[derive(Clone)]
pub struct OutputManager {
    root: PathBuf,
}

impl OutputManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn work_dir(&self, job_id: Uuid) -> PathBuf {
        self.root.join(job_id.to_string())
    }

    pub fn input_dir(&self, job_id: Uuid) -> PathBuf {
        self.work_dir(job_id).join("input")
    }

    pub fn output_dir(&self, job_id: Uuid) -> PathBuf {
        self.work_dir(job_id).join("output")
    }

    pub async fn prepare_job_dirs(&self, job_id: Uuid) -> std::io::Result<()> {
        fs::create_dir_all(self.input_dir(job_id)).await?;
        fs::create_dir_all(self.output_dir(job_id)).await?;
        Ok(())
    }

    pub fn input_path(&self, job_id: Uuid, stem: &str, ext: &str) -> PathBuf {
        self.input_dir(job_id)
            .join(format!("{stem}.{}", formats::normalize_ext(ext)))
    }

    pub async fn finalize(
        &self,
        job_id: Uuid,
        produced: &Path,
        stem: &str,
        target_ext: &str,
    ) -> std::io::Result<PathBuf> {
        let ext = formats::normalize_ext(target_ext);
        let dest = self.output_dir(job_id).join(format!("{stem}.{ext}"));
        if produced != dest {
            if dest.exists() {
                fs::remove_file(&dest).await?;
            }
            fs::rename(produced, &dest).await.or_else(|_| {
                // Cross-device fallback
                std::fs::copy(produced, &dest).map(|_| ())?;
                std::fs::remove_file(produced)?;
                Ok::<_, std::io::Error>(())
            })?;
        }
        Ok(dest)
    }

    pub fn zip_files(sources: &[PathBuf], dest: &Path) -> Result<PathBuf, String> {
        let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for source in sources {
            let name = source
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| "invalid output filename".to_string())?;
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut zip).map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok(dest.to_path_buf())
    }

    pub async fn cleanup_dir(&self, dir: &Path) {
        if let Err(err) = fs::remove_dir_all(dir).await {
            tracing::warn!(path = %dir.display(), %err, "failed to remove job directory");
        }
    }
}
