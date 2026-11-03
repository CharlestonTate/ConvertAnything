use crate::engine::InputFile;
use crate::formats::{self, Category};
use crate::jobs::{Job, JobStatus};
use crate::AppState;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 2000;

#[derive(Serialize)]
pub struct ConvertResponse {
    pub job_id: Uuid,
}

pub async fn list_formats() -> impl IntoResponse {
    Json(formats::registry())
}

pub async fn convert(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let job_id = Uuid::new_v4();
    if let Err(err) = state.output.prepare_job_dirs(job_id).await {
        tracing::error!(%err, "failed to create job directories");
        return error(StatusCode::INTERNAL_SERVER_ERROR, "failed to prepare job");
    }

    let mut target_format: Option<String> = None;
    let mut saved: Vec<(String, PathBuf)> = Vec::new();
    let mut used_stems = HashSet::new();
    let mut remaining = MAX_TOTAL_BYTES;

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "target_format" | "format" => match field.text().await {
                Ok(text) => target_format = Some(text),
                Err(_) => {
                    state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                    return error(StatusCode::BAD_REQUEST, "invalid target_format field");
                }
            },
            "file" | "files" => {
                if saved.len() >= MAX_FILES {
                    state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                    return error(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        &format!("too many files (max {MAX_FILES})"),
                    );
                }
                let original = field
                    .file_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "upload.bin".into());
                let source_ext = match formats::extension_from_filename(&original) {
                    Some(ext) => ext,
                    None => {
                        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                        return error(StatusCode::BAD_REQUEST, "file has no extension");
                    }
                };
                let stem = formats::unique_stem(&original, &mut used_stems);
                let path = state.output.input_path(job_id, &stem, &source_ext);
                match stream_field_to_disk(&mut field, &path, &mut remaining).await {
                    Ok(0) => {
                        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                        return error(StatusCode::BAD_REQUEST, "uploaded file is empty");
                    }
                    Ok(_) => saved.push((original, path)),
                    Err(status) if status == StatusCode::PAYLOAD_TOO_LARGE => {
                        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                        return error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "upload exceeds 8 GB total limit",
                        );
                    }
                    Err(_) => {
                        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                        return error(StatusCode::BAD_REQUEST, "failed to read upload");
                    }
                }
            }
            _ => {}
        }
    }

    let Some(target) = target_format else {
        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
        return error(
            StatusCode::BAD_REQUEST,
            "multipart fields `file` and `target_format` are required",
        );
    };
    if saved.is_empty() {
        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
        return error(StatusCode::BAD_REQUEST, "at least one file is required");
    }

    let mut source_ext: Option<String> = None;
    for (name, _) in &saved {
        let ext = match formats::extension_from_filename(name) {
            Some(ext) => ext,
            None => {
                state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                return error(StatusCode::BAD_REQUEST, "file has no extension");
            }
        };
        match &source_ext {
            None => source_ext = Some(ext),
            Some(expected) if expected == &ext => {}
            Some(expected) => {
                state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
                return error(
                    StatusCode::BAD_REQUEST,
                    &format!("all files must be the same type (`{expected}`), found `{ext}`"),
                );
            }
        }
    }
    let source_ext = source_ext.unwrap();
    let target_ext = formats::normalize_ext(&target);

    let Some(category) = formats::category_for_ext(&source_ext) else {
        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
        return error(
            StatusCode::BAD_REQUEST,
            &format!("unsupported source format `{source_ext}`"),
        );
    };

    if !formats::is_valid_target(category, &target_ext) {
        state.output.cleanup_dir(&state.output.work_dir(job_id)).await;
        return error(
            StatusCode::BAD_REQUEST,
            &format!(
                "`{target_ext}` is not a valid {category} target",
                category = category_label(category)
            ),
        );
    }

    let inputs: Vec<InputFile> = saved
        .iter()
        .map(|(_, path)| InputFile {
            stem: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string(),
            path: path.clone(),
        })
        .collect();

    let output_name = if inputs.len() == 1 {
        format!("{}.{target_ext}", inputs[0].stem)
    } else {
        format!("{source_ext}-to-{target_ext}.zip")
    };

    let job = Job {
        id: job_id,
        status: JobStatus::Queued,
        category,
        source_format: source_ext,
        target_format: target_ext.clone(),
        original_name: saved[0].0.clone(),
        output_name,
        file_count: inputs.len(),
        files_done: 0,
        error: None,
        output_path: None,
        work_dir: state.output.work_dir(job_id),
        created_at: chrono::Utc::now(),
    };

    state.jobs.insert(job).await;
    state.engine.spawn(job_id, inputs, target_ext);

    (StatusCode::ACCEPTED, Json(ConvertResponse { job_id })).into_response()
}

async fn stream_field_to_disk(
    field: &mut axum::extract::multipart::Field<'_>,
    path: &PathBuf,
    remaining: &mut u64,
) -> Result<u64, StatusCode> {
    let mut out = tokio::fs::File::create(path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut written = 0u64;
    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                let len = chunk.len() as u64;
                if len > *remaining {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                out.write_all(&chunk)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                written += len;
                *remaining -= len;
            }
            Ok(None) => break,
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        }
    }
    out.flush().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(written)
}

pub async fn get_job(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match state.jobs.get(id).await {
        Some(job) => Json(job.public_view()).into_response(),
        None => error(StatusCode::NOT_FOUND, "job not found"),
    }
}

pub async fn download(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let Some(job) = state.jobs.get(id).await else {
        return error(StatusCode::NOT_FOUND, "job not found");
    };

    match job.status {
        JobStatus::Done => {}
        JobStatus::Failed => {
            return error(
                StatusCode::CONFLICT,
                job.error.as_deref().unwrap_or("conversion failed"),
            );
        }
        _ => return error(StatusCode::CONFLICT, "conversion is not finished"),
    }

    let Some(path) = job.output_path else {
        return error(StatusCode::NOT_FOUND, "output file missing");
    };

    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(_) => return error(StatusCode::NOT_FOUND, "output file missing"),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let content_type = if job.output_name.ends_with(".zip") {
        "application/zip"
    } else {
        formats::content_type_for_ext(&job.target_format)
    };
    let disposition = format!("attachment; filename=\"{}\"", job.output_name);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(body)
        .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "failed to build response"))
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(ErrorBody { error: message.to_string() })).into_response()
}

fn category_label(category: Category) -> &'static str {
    match category {
        Category::Audio => "audio",
        Category::Image => "image",
        Category::Document => "document",
    }
}
