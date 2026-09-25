//! PDF text extraction.
//!
//! Extracts the embedded text layer from a PDF file. Works only on
//! "digital" PDFs (those produced by a word processor, LaTeX, or an
//! online tool). Scanned PDFs — where each page is an image of paper —
//! contain no text layer and are rejected with `PdfError::NoText`. Those
//! require OCR (planned).

use thiserror::Error;

/// Maximum accepted PDF size. Should match the route's body limit.
pub const MAX_PDF_BYTES: usize = 20 * 1024 * 1024;

/// PDF-specific failure modes. Deliberately coarse: the caller decides
/// what to tell the user; the underlying library message is never
/// surfaced.
#[derive(Debug, Error)]
pub enum PdfError {
    /// Input is not a PDF, or the PDF is structurally unreadable.
    #[error("file is not a readable PDF")]
    Unreadable,

    /// PDF is password-protected or encrypted.
    #[error("PDF is encrypted")]
    Encrypted,

    /// PDF has no extractable text (likely a scanned document).
    #[error("PDF contains no extractable text")]
    NoText,

    /// Any other extraction failure.
    #[error("PDF extraction failed")]
    Other,
}

/// Extracts text from a PDF byte stream.
///
/// On success the returned string is trimmed, has NUL bytes removed,
/// and uses `\n` line endings. Returns [`PdfError::NoText`] if the PDF
/// has no text layer.
pub fn extract_text(bytes: &[u8]) -> Result<String, PdfError> {
    // Verify magic bytes. `pdf-extract` does not check this itself, and
    // feeding it arbitrary data produces confusing errors.
    if !bytes.starts_with(b"%PDF-") {
        return Err(PdfError::Unreadable);
    }

    if bytes.len() > MAX_PDF_BYTES {
        // Defensive: the route body limit should already catch this.
        return Err(PdfError::Unreadable);
    }

    let raw = pdf_extract::extract_text_from_mem(bytes).map_err(|e| {
        // Classify without leaking the raw error string to clients.
        let s = e.to_string().to_lowercase();
        if s.contains("encrypted") || s.contains("password") {
            PdfError::Encrypted
        } else if s.contains("no text") || s.contains("empty") {
            PdfError::NoText
        } else {
            PdfError::Other
        }
    })?;

    // Strip NULs (PostgreSQL text columns reject them) and normalise
    // CRLF/CR to LF.
    let cleaned: String = raw
        .chars()
        .filter(|c| *c != '\0')
        .collect::<String>()
        .replace("\r\n", "\n")
        .replace('\r', "\n");

    let trimmed = cleaned.trim().to_string();
    if trimmed.is_empty() {
        return Err(PdfError::NoText);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pdf_data() {
        let err = extract_text(b"this is not a pdf").unwrap_err();
        assert!(matches!(err, PdfError::Unreadable));
    }

    #[test]
    fn rejects_empty_input() {
        let err = extract_text(b"").unwrap_err();
        assert!(matches!(err, PdfError::Unreadable));
    }

    #[test]
    fn rejects_pdf_without_magic_bytes() {
        let err = extract_text(b"PK\x03\x04 not a pdf").unwrap_err();
        assert!(matches!(err, PdfError::Unreadable));
    }
}
