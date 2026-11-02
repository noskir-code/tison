//! Files attached to private messages besides photos (`Attach a file` in
//! the messenger, see `handlers::messenger`): PDFs, office documents,
//! archives and plain text. Their type is read from their content, the name
//! only telling apart the formats made of a ZIP (Word, LibreOffice, EPUB...). The saved file gets the extension of
//! its type, never the one sent: the uploads folder is served by extension,
//! so nothing can become a page (HTML, SVG) on the site.

/// Like a photo.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;

/// Shown in the post form's `accept`.
pub const ACCEPTED: &str = ".pdf,.txt,.md,.csv,.odt,.ods,.odp,.docx,.xlsx,.pptx,.epub,.zip";

/// A file that can be attached: its extension and media type.
#[derive(Debug, PartialEq, Eq)]
pub struct Document {
    pub extension: &'static str,
    pub media_type: &'static str,
}

const fn document(extension: &'static str, media_type: &'static str) -> Option<Document> {
    Some(Document { extension, media_type })
}

/// The extension of the name sent by the browser, lowercase.
fn extension_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default()
}

/// What `data` is, or `None` when it can't be attached.
pub fn detect(data: &[u8], name: &str) -> Option<Document> {
    if data.is_empty() {
        return None;
    }
    if data.starts_with(b"%PDF-") {
        return document("pdf", "application/pdf");
    }
    if data.starts_with(b"PK\x03\x04") {
        return match extension_of(name).as_str() {
            "docx" => document("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
            "xlsx" => document("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
            "pptx" => document("pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
            "odt" => document("odt", "application/vnd.oasis.opendocument.text"),
            "ods" => document("ods", "application/vnd.oasis.opendocument.spreadsheet"),
            "odp" => document("odp", "application/vnd.oasis.opendocument.presentation"),
            "epub" => document("epub", "application/epub+zip"),
            _ => document("zip", "application/zip"),
        };
    }
    // Text: UTF-8 without control characters but tabs and line breaks.
    let text = std::str::from_utf8(data).ok()?;
    let plain = text.chars().all(|letter| !letter.is_control() || matches!(letter, '\n' | '\r' | '\t'));
    if !plain {
        return None;
    }
    match extension_of(name).as_str() {
        "csv" => document("csv", "text/csv"),
        "md" => document("md", "text/markdown"),
        _ => document("txt", "text/plain"),
    }
}

/// The name shown for the file: the one it had, without its folders nor
/// control characters, cut when very long.
pub fn shown_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let name: String = name.chars().filter(|letter| !letter.is_control()).take(100).collect();
    if name.trim().is_empty() { "file".to_string() } else { name.trim().to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_told_by_their_content() {
        assert_eq!(detect(b"%PDF-1.7 ...", "a.exe").unwrap().extension, "pdf");
        assert_eq!(detect(b"PK\x03\x04rest", "Report.DOCX").unwrap().extension, "docx");
        assert_eq!(detect(b"PK\x03\x04rest", "a.html").unwrap().extension, "zip");
        assert_eq!(detect(b"a,b\n1,2\n", "data.csv").unwrap().media_type, "text/csv");
        // A page stays text: never saved as .html.
        assert_eq!(detect(b"<script>alert(1)</script>", "x.html").unwrap().extension, "txt");
        assert_eq!(detect(b"\x7fELF\x02\x01", "x.txt"), None);
        assert_eq!(detect(b"", "x.txt"), None);
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(shown_name("C:\\Users\\me\\Report 2026.pdf"), "Report 2026.pdf");
        assert_eq!(shown_name("../../etc/passwd"), "passwd");
        assert_eq!(shown_name("\u{0}"), "file");
    }
}
