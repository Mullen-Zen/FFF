use serde::{Deserialize, Serialize};

use crate::error::Result;

const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
const MODEL: &str = "llama3.2";

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model:  &'a str,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
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
         output ONLY a comma-separated list of 3-8 short, lowercase, descriptive tags. \
         No explanations, no numbering, no extra text — just the tags.\n\n\
         Filename: {}\nExtension: {}{}",
        name, ext, content_hint
    )
}

pub async fn get_tags(
    client: &reqwest::Client,
    name: &str,
    ext: &str,
    preview: Option<&str>,
) -> Result<Vec<String>> {
    let body = OllamaRequest {
        model: MODEL,
        prompt: build_prompt(name, ext, preview),
        stream: false,
    };

    let resp = client
        .post(OLLAMA_URL)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await?
        .json::<OllamaResponse>()
        .await?;

    let tags: Vec<String> = resp
        .response
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty() && t.len() < 50)
        .collect();

    Ok(tags)
}
