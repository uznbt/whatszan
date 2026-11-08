//! MIME type detection for files shared into WhatsApp Web.
//!
//! The Electron renderer guessed the MIME type in JavaScript from the file
//! extension. That decision now lives in Rust so the renderer only forwards the
//! bytes and the name supplied by the backend.

/// Return a MIME type for `name`, falling back to `application/octet-stream`.
///
/// Detection is extension-based because the caller may not have the file on
/// disk anymore (for example a browser-provided `File`).
pub fn mime_for_name(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "rar" => "application/vnd.rar",
        "7z" => "application/x-7z-compressed",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_types_the_renderer_used_to_guess() {
        assert_eq!(mime_for_name("photo.JPG"), "image/jpeg");
        assert_eq!(mime_for_name("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_for_name("sticker.png"), "image/png");
        assert_eq!(mime_for_name("clip.mp4"), "video/mp4");
        assert_eq!(mime_for_name("document.pdf"), "application/pdf");
        assert_eq!(mime_for_name("notes.txt"), "text/plain");
        assert_eq!(mime_for_name("table.csv"), "text/csv");
        assert_eq!(mime_for_name("archive.zip"), "application/zip");
    }

    #[test]
    fn unknown_and_extensionless_names_fall_back() {
        assert_eq!(mime_for_name("README"), "application/octet-stream");
        assert_eq!(mime_for_name("archive.unknown"), "application/octet-stream");
        assert_eq!(mime_for_name("trailing."), "application/octet-stream");
    }
}
