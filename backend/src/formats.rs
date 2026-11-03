use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Audio,
    Image,
    Document,
}

pub const AUDIO_FORMATS: &[&str] = &["mp3", "wav", "flac", "ogg", "aac", "m4a", "opus"];
pub const IMAGE_FORMATS: &[&str] = &["jpg", "png", "webp", "gif", "tiff", "bmp"];
pub const DOCUMENT_FORMATS: &[&str] = &["pdf", "docx", "doc", "odt", "rtf", "txt", "pptx", "xlsx"];

#[derive(Serialize)]
pub struct FormatSet {
    pub sources: &'static [&'static str],
    pub targets: &'static [&'static str],
}

#[derive(Serialize)]
pub struct FormatRegistry {
    pub audio: FormatSet,
    pub image: FormatSet,
    pub document: FormatSet,
}

pub fn registry() -> FormatRegistry {
    FormatRegistry {
        audio: FormatSet {
            sources: AUDIO_FORMATS,
            targets: AUDIO_FORMATS,
        },
        image: FormatSet {
            sources: IMAGE_FORMATS,
            targets: IMAGE_FORMATS,
        },
        document: FormatSet {
            sources: DOCUMENT_FORMATS,
            targets: DOCUMENT_FORMATS,
        },
    }
}

pub fn normalize_ext(ext: &str) -> String {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    match e.as_str() {
        "jpeg" => "jpg".to_string(),
        "tif" => "tiff".to_string(),
        other => other.to_string(),
    }
}

pub fn category_for_ext(ext: &str) -> Option<Category> {
    let ext = normalize_ext(ext);
    if AUDIO_FORMATS.contains(&ext.as_str()) {
        Some(Category::Audio)
    } else if IMAGE_FORMATS.contains(&ext.as_str()) {
        Some(Category::Image)
    } else if DOCUMENT_FORMATS.contains(&ext.as_str()) {
        Some(Category::Document)
    } else {
        None
    }
}

pub fn is_valid_target(category: Category, ext: &str) -> bool {
    let ext = normalize_ext(ext);
    match category {
        Category::Audio => AUDIO_FORMATS.contains(&ext.as_str()),
        Category::Image => IMAGE_FORMATS.contains(&ext.as_str()),
        Category::Document => DOCUMENT_FORMATS.contains(&ext.as_str()),
    }
}

pub fn content_type_for_ext(ext: &str) -> &'static str {
    match normalize_ext(ext).as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "opus" => "audio/opus",
        "jpg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "odt" => "application/vnd.oasis.opendocument.text",
        "rtf" => "application/rtf",
        "txt" => "text/plain",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
}

pub fn extension_from_filename(name: &str) -> Option<String> {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(normalize_ext)
}

pub fn sanitize_stem(name: &str) -> String {
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned.chars().take(80).collect()
    }
}

pub fn unique_stem(name: &str, used: &mut std::collections::HashSet<String>) -> String {
    let base = sanitize_stem(name);
    if used.insert(base.clone()) {
        return base;
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}_{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}
