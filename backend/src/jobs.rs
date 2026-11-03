use crate::formats::Category;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub status: JobStatus,
    pub category: Category,
    pub source_format: String,
    pub target_format: String,
    pub original_name: String,
    pub output_name: String,
    pub file_count: usize,
    pub files_done: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub output_path: Option<PathBuf>,
    #[serde(skip)]
    pub work_dir: PathBuf,
    pub created_at: DateTime<Utc>,
}

impl Job {
    pub fn public_view(&self) -> JobView {
        JobView {
            job_id: self.id,
            status: self.status,
            category: self.category,
            source_format: self.source_format.clone(),
            target_format: self.target_format.clone(),
            output_name: self.output_name.clone(),
            file_count: self.file_count,
            files_done: self.files_done,
            error: self.error.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobView {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub category: Category,
    pub source_format: String,
    pub target_format: String,
    pub output_name: String,
    pub file_count: usize,
    pub files_done: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct JobStore {
    inner: Arc<RwLock<HashMap<Uuid, Job>>>,
}

impl JobStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, job: Job) {
        self.inner.write().await.insert(job.id, job);
    }

    pub async fn get(&self, id: Uuid) -> Option<Job> {
        self.inner.read().await.get(&id).cloned()
    }

    pub async fn update<F>(&self, id: Uuid, f: F) -> Option<Job>
    where
        F: FnOnce(&mut Job),
    {
        let mut map = self.inner.write().await;
        if let Some(job) = map.get_mut(&id) {
            f(job);
            return Some(job.clone());
        }
        None
    }

    pub async fn remove_expired(&self, max_age: chrono::Duration) -> Vec<PathBuf> {
        let now = Utc::now();
        let mut map = self.inner.write().await;
        let stale: Vec<Uuid> = map
            .iter()
            .filter(|(_, job)| now - job.created_at > max_age)
            .map(|(id, _)| *id)
            .collect();
        let mut dirs = Vec::new();
        for id in stale {
            if let Some(job) = map.remove(&id) {
                dirs.push(job.work_dir);
            }
        }
        dirs
    }
}
