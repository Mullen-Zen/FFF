use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::error::Result;

const OLLAMA_URL:  &str = "http://localhost:11434/api/generate";
const TEXT_MODEL:  &str = "llama3.2";
const VISION_MODEL: &str = "moondream";

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model:  &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaVisionRequest<'a> {
    model:  &'a str,
    prompt: String,
    images: Vec<String>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

fn parse_tags(raw: &str) -> Vec<String> {
    // Models may emit commas, newlines, quotes, or Python-list brackets
    raw.split([',', '\n'])
        .map(|t| {
            // Strip list punctuation then re-join whitespace
            t.chars()
                .filter(|c| !matches!(c, '\'' | '"' | '[' | ']'))
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .filter(|t| {
            !t.is_empty()
                && t.len() < 50
                && t.split_whitespace().count() <= 3 // drop run-on phrases
        })
        .collect()
}

fn build_prompt(name: &str, ext: &str, preview: Option<&str>) -> String {
    let content_hint = match preview {
        Some(p) if !p.trim().is_empty() => {
            let truncated = &p[..p.len().min(2000)];
            format!("\n\nFirst 2000 characters of file content:\n```\n{}\n```", truncated)
        }
        _ => String::new(),
    };
    format!(
        "You are a file tagging assistant. Given the filename and optional content preview, \
         output ONLY a comma-separated list of 3-8 short, lowercase, one-or-two-word tags. \
         No explanations, no numbering, no quotes, no brackets — just the tags.\n\n\
         Filename: {}\nExtension: {}{}",
        name, ext, content_hint
    )
}

pub async fn get_tags(
    client:  &reqwest::Client,
    name:    &str,
    ext:     &str,
    preview: Option<&str>,
) -> Result<Vec<String>> {
    let resp = client
        .post(OLLAMA_URL)
        .json(&OllamaRequest { model: TEXT_MODEL, prompt: build_prompt(name, ext, preview), stream: false })
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .json::<OllamaResponse>()
        .await?;

    Ok(parse_tags(&resp.response))
}

/// Tag an image using moondream (vision model)
pub async fn get_image_tags(
    client: &reqwest::Client,
    name:   &str,
    bytes:  &[u8],
) -> Result<Vec<String>> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let prompt = format!(
        "You are a file tagging assistant. The image filename is '{}'. \
         Output ONLY a comma-separated list of 3-6 short, lowercase, one-or-two-word tags. \
         Describe what the main subject IS and its context (e.g. cat, pet, indoor, portrait). \
         Focus on subjects and categories — not body parts or low-level details. \
         No quotes, no brackets, no explanations — just the tags.",
        name
    );
    let resp = client
        .post(OLLAMA_URL)
        .json(&OllamaVisionRequest { model: VISION_MODEL, prompt, images: vec![b64], stream: false })
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .json::<OllamaResponse>()
        .await?;

    Ok(parse_tags(&resp.response))
}
