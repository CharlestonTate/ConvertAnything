use crate::adapters;
use crate::formats::{self, Category};
use crate::jobs::{JobStatus, JobStore};
use crate::output::OutputManager;
use rayon::ThreadPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
pub struct ConversionEngine {
    jobs: JobStore,
    output: OutputManager,
    rayon: Arc<ThreadPool>,
}

pub struct InputFile {
    pub path: PathBuf,
    pub stem: String,
}

impl ConversionEngine {
    pub fn new(jobs: JobStore, output: OutputManager, rayon: Arc<ThreadPool>) -> Self {
        Self {
            jobs,
            output,
            rayon,
        }
    }

    pub fn spawn(&self, job_id: Uuid, inputs: Vec<InputFile>, target_ext: String) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.run(job_id, inputs, target_ext).await;
        });
    }

    async fn run(&self, job_id: Uuid, inputs: Vec<InputFile>, target_ext: String) {
        self.jobs
            .update(job_id, |job| job.status = JobStatus::Running)
            .await;

        let category = match self.jobs.get(job_id).await {
            Some(job) => job.category,
            None => return,
        };

        let result = self.convert_batch(job_id, category, inputs, &target_ext).await;

        match result {
            Ok(output_path) => {
                self.jobs
                    .update(job_id, |job| {
                        job.status = JobStatus::Done;
                        job.output_path = Some(output_path);
                        job.error = None;
                    })
                    .await;
            }
            Err(error) => {
                tracing::error!(%job_id, %error, "conversion failed");
                self.jobs
                    .update(job_id, |job| {
                        job.status = JobStatus::Failed;
                        job.error = Some(error);
                    })
                    .await;
            }
        }
    }

    async fn convert_batch(
        &self,
        job_id: Uuid,
        category: Category,
        inputs: Vec<InputFile>,
        target_ext: &str,
    ) -> Result<PathBuf, String> {
        let total = inputs.len();
        let mut outputs = Vec::with_capacity(total);

        if category == Category::Document {
            for input in inputs {
                let path = self.convert_one(job_id, category, &input, target_ext).await?;
                outputs.push(path);
                self.bump_progress(job_id).await;
            }
        } else {
            let sem = Arc::new(Semaphore::new(4));
            let mut tasks = Vec::new();
            for input in inputs {
                let engine = self.clone();
                let permit = sem.clone().acquire_owned().await.map_err(|e| e.to_string())?;
                let target = target_ext.to_string();
                tasks.push(tokio::spawn(async move {
                    let _permit = permit;
                    engine
                        .convert_one(job_id, category, &input, &target)
                        .await
                }));
            }
            for task in tasks {
                let path = task.await.map_err(|e| e.to_string())??;
                outputs.push(path);
                self.bump_progress(job_id).await;
            }
        }

        if outputs.len() == 1 {
            return Ok(outputs.remove(0));
        }

        let zip_name = self
            .jobs
            .get(job_id)
            .await
            .map(|job| job.output_name)
            .unwrap_or_else(|| "converted.zip".into());
        let zip_path = self.output.output_dir(job_id).join(zip_name);
        let dest = zip_path.clone();
        tokio::task::spawn_blocking(move || OutputManager::zip_files(&outputs, &dest))
            .await
            .map_err(|e| e.to_string())?
    }

    async fn bump_progress(&self, job_id: Uuid) {
        self.jobs
            .update(job_id, |job| {
                job.files_done = job.files_done.saturating_add(1).min(job.file_count);
            })
            .await;
    }

    async fn convert_one(
        &self,
        job_id: Uuid,
        category: Category,
        input: &InputFile,
        target_ext: &str,
    ) -> Result<PathBuf, String> {
        let ext = formats::normalize_ext(target_ext);
        let staging = self
            .output
            .work_dir(job_id)
            .join("staging")
            .join(&input.stem);
        tokio::fs::create_dir_all(&staging)
            .await
            .map_err(|e| e.to_string())?;
        let staged_out = staging.join(format!("{}.{ext}", input.stem));

        let produced = match category {
            Category::Audio => adapters::audio::convert(&input.path, &staged_out).await?,
            Category::Image => {
                let input_path = input.path.clone();
                let staged_out = staged_out.clone();
                let pool = self.rayon.clone();
                tokio::task::spawn_blocking(move || {
                    pool.install(|| adapters::image::convert_blocking(&input_path, &staged_out))
                })
                .await
                .map_err(|e| e.to_string())??
            }
            Category::Document => adapters::document::convert(&input.path, &staging, &ext).await?,
        };

        self.output
            .finalize(job_id, &produced, &input.stem, &ext)
            .await
            .map_err(|e| e.to_string())
    }
}
