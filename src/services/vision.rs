//! Image-to-text extraction via a Groq-hosted Qwen vision model.
//!
//! The model receives the image as a base64 `data:` URL and returns
//! the visible text. We send the request directly to the OpenAI-compatible
//! chat-completions endpoint because the existing `AiProvider` trait is
//! text-only.
//!
//! Text extraction is best-effort: the model can misread handwriting,
//! blurry images, or unusual fonts. Callers should present the result
//! as a draft that the user can edit before saving.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use reqwest::Client;
use serde_json::json;
use thiserror::Error;

/// Vision-capable model on Groq. Qwen 3.8 supports image input.
const VISION_MODEL: &str = "qwen/qwen3.8-27b";

/// Maximum accepted image size. Matches the route body limit.
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Image type detected from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Webp,
}

impl ImageFormat {
    fn mime(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::Webp => "image/webp",
        }
    }
}

/// Failure modes for image text extraction.
#[derive(Debug, Error)]
pub enum VisionError {
    /// Bytes are not a JPEG, PNG, or WebP image.
    #[error("unsupported image format")]
    UnsupportedFormat,

    /// Image exceeds [`MAX_IMAGE_BYTES`].
    #[error("image is too large")]
    TooLarge,

    /// The AI provider is not configured on this server.
    #[error("vision provider is not configured")]
    NotConfigured,

    /// Network or transport error reaching the provider.
    #[error("vision request failed")]
    RequestFailed,

    /// Provider returned a non-2xx response or an unexpected body.
    #[error("vision provider error")]
    ProviderError,

    /// The model found no readable text in the image.
    #[error("no readable text in image")]
    NoText,
}

/// Detects the image format from magic bytes.
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(ImageFormat::Png);
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(ImageFormat::Webp);
    }
    None
}

const EXTRACT_PROMPT: &str = "\
You are a document OCR engine. Read all visible text from the supplied \
image, preserving line breaks and order.

Rules:
- Output only the extracted text. No commentary, no markdown fences, no \
  explanations.
- Preserve paragraph and section structure.
- If the image contains no readable text, output the single word: EMPTY
- Do not translate. Output the text exactly as written.
- Do not summarise or paraphrase.";

/// Extracts text from an image via the Groq vision endpoint.
pub async fn extract_text(
    client: &Client,
    base_url: &str,
    api_key: &str,
    image_bytes: &[u8],
) -> Result<String, VisionError> {
    if image_bytes.is_empty() {
        return Err(VisionError::UnsupportedFormat);
    }
    if image_bytes.len() > MAX_IMAGE_BYTES {
        return Err(VisionError::TooLarge);
    }
    let format = detect_format(image_bytes).ok_or(VisionError::UnsupportedFormat)?;

    let data_url = format!("data:{};base64,{}", format.mime(), B64.encode(image_bytes));

    let url = build_chat_url(base_url);
    let body = json!({
        "model": VISION_MODEL,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": EXTRACT_PROMPT },
                { "type": "image_url", "image_url": { "url": data_url } }
            ]
        }],
        "max_completion_tokens": 4096,
        "temperature": 0.1
    });

    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|_| VisionError::RequestFailed)?;

    if !resp.status().is_success() {
        return Err(VisionError::ProviderError);
    }

    let parsed: serde_json::Value = resp.json().await.map_err(|_| VisionError::ProviderError)?;

    // Some reasoning models place their output in a `reasoning` field
    // and leave `content` empty. Prefer `content`; fall back to
    // `reasoning` only if content is empty.
    let msg = &parsed["choices"][0]["message"];
    let content = msg["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            msg["reasoning"]
                .as_str()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .ok_or(VisionError::ProviderError)?;

    if content.eq_ignore_ascii_case("EMPTY") {
        return Err(VisionError::NoText);
    }

    Ok(content)
}

/// Builds the chat-completions URL, tolerating a base that may or may
/// not already contain `/v1`. Kept in sync with `ai::openai::build_url`.
fn build_chat_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") || base.ends_with("/v1beta") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_jpeg() {
        let b = [0xFF, 0xD8, 0xFF, 0xE0, 0x00];
        assert_eq!(detect_format(&b), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn detects_png() {
        let b = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        assert_eq!(detect_format(&b), Some(ImageFormat::Png));
    }

    #[test]
    fn detects_webp() {
        let mut b = *b"RIFF\0\0\0\0WEBPVP8 ";
        assert_eq!(detect_format(&b), Some(ImageFormat::Webp));
        b[0] = b'X';
        assert_eq!(detect_format(&b), None);
    }

    #[test]
    fn rejects_unknown_format() {
        assert_eq!(detect_format(b"not an image"), None);
        assert_eq!(detect_format(&[]), None);
    }

    #[test]
    fn build_url_appends_v1_when_missing() {
        assert_eq!(
            build_chat_url("https://api.groq.com/openai"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        assert_eq!(
            build_chat_url("https://api.groq.com/openai/"),
            "https://api.groq.com/openai/v1/chat/completions"
        );
    }

    #[test]
    fn build_url_respects_existing_v1() {
        assert_eq!(
            build_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            build_chat_url("https://generativelanguage.googleapis.com/v1beta/openai"),
            "https://generativelanguage.googleapis.com/v1beta/openai/v1/chat/completions"
        );
    }
}
