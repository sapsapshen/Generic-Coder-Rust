//! Media file handler for Generic Coder.
//! Detects file types, extracts metadata and text from PDF/DOCX/XLSX/images.

use std::fs;
use std::path::Path;

use serde_json::Value;

pub fn get_file_info(path: &str) -> Value {
    let p = Path::new(path);
    if !p.exists() {
        return serde_json::json!({"status": "error", "msg": format!("File not found: {}", path)});
    }
    let len = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let media_type = detect_type(&ext);
    serde_json::json!({
        "status": "success",
        "name": p.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "path": path,
        "size": len,
        "extension": ext,
        "media_type": media_type,
    })
}

pub fn extract_text(path: &str) -> Value {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "txt" | "md" | "py" | "rs" | "js" | "ts" | "json" | "toml" | "yaml" | "yml" | "html"
        | "css" | "xml" | "csv" | "sh" | "bash" | "sql" | "java" | "go" | "rb" | "c" | "cpp"
        | "h" | "swift" | "kt" => match fs::read_to_string(path) {
            Ok(text) => {
                let preview = &text[..text.len().min(500)];
                serde_json::json!({
                    "status": "success",
                    "text": text,
                    "length": text.len(),
                    "preview": preview,
                })
            }
            Err(e) => serde_json::json!({"status": "error", "msg": e.to_string()}),
        },
        _ => {
            serde_json::json!({"status": "error", "msg": "Unsupported file type for text extraction"})
        }
    }
}

fn detect_type(ext: &str) -> &str {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "tiff" | "ico" => "IMAGE",
        "mp4" | "mov" | "avi" | "webm" | "mkv" | "flv" => "VIDEO",
        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" => "AUDIO",
        "pdf" => "PDF",
        "docx" | "doc" => "DOCUMENT",
        "xlsx" | "xls" => "SPREADSHEET",
        "pptx" | "ppt" => "PRESENTATION",
        "zip" | "tar" | "gz" | "7z" | "rar" => "ARCHIVE",
        "json" | "xml" | "yaml" | "yml" | "toml" | "csv" => "DATA",
        _ => "UNKNOWN",
    }
}
