use anyhow::{Context, Result};
use base64::Engine as _;
use serde_json::json;

const DEFAULT_MAX_COMPLETION_TOKENS: u32 = 768;

#[derive(Clone)]
pub(super) struct MimoFontConfig {
    base_url: String,
    api_key: String,
    model: String,
    max_completion_tokens: u32,
}

impl MimoFontConfig {
    pub(super) fn from_env_or_config() -> Result<Self> {
        let app_config = crate::config::load().ok();
        let mimo_provider = app_config.as_ref().and_then(|config| {
            config
                .providers
                .iter()
                .find(|provider| provider.id == "mimo")
        });
        let base_url = non_empty_env("MIMO_BASE_URL")
            .or_else(|| mimo_provider.and_then(|provider| provider.base_url.clone()))
            .context("MIMO font selection requires MIMO_BASE_URL or a mimo provider base_url")?;
        let api_key = non_empty_env("MIMO_API_KEY")
            .or_else(|| {
                mimo_provider
                    .and_then(|provider| provider.api_key.as_ref())
                    .map(|secret| secret.expose().to_string())
            })
            .context("MIMO font selection requires MIMO_API_KEY or a mimo provider secret")?;
        let model = non_empty_env("MIMO_VISION_MODEL")
            .context("MIMO font selection requires MIMO_VISION_MODEL")?;
        let max_completion_tokens = non_empty_env("MIMO_MAX_COMPLETION_TOKENS")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(DEFAULT_MAX_COMPLETION_TOKENS);

        Ok(Self {
            base_url,
            api_key,
            model,
            max_completion_tokens,
        })
    }
}

pub(super) struct MimoFontClient {
    http: reqwest::Client,
    config: MimoFontConfig,
}

impl MimoFontClient {
    pub(super) fn new(config: MimoFontConfig) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder().build()?,
            config,
        })
    }

    pub(super) async fn analyze_image(
        &self,
        image_png: &[u8],
        prompt: &str,
        system_prompt: &str,
    ) -> Result<String> {
        let response = self
            .http
            .post(self.endpoint())
            .header("api-key", &self.config.api_key)
            .header("content-type", "application/json")
            .json(&self.request_body(image_png, prompt, system_prompt))
            .send()
            .await
            .context("request MIMO chat completions")?;
        let status = response.status();
        let value: serde_json::Value = response
            .json()
            .await
            .context("decode MIMO chat completion response")?;
        if !status.is_success() {
            anyhow::bail!("MIMO chat completion failed with {status}: {value}");
        }
        value["choices"][0]["message"]["content"]
            .as_str()
            .map(ToOwned::to_owned)
            .context("MIMO response did not include choices[0].message.content")
    }

    pub(super) fn endpoint(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }

    #[cfg(test)]
    pub(super) fn model(&self) -> &str {
        &self.config.model
    }

    fn request_body(
        &self,
        image_png: &[u8],
        prompt: &str,
        system_prompt: &str,
    ) -> serde_json::Value {
        json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {
                    "role": "user",
                    "content": [
                        {"type": "image_url", "image_url": {"url": image_data_url(image_png)}},
                        {"type": "text", "text": prompt}
                    ]
                }
            ],
            "max_completion_tokens": self.config.max_completion_tokens
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn image_data_url(image_png: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(image_png);
    format!("data:image/png;base64,{encoded}")
}
