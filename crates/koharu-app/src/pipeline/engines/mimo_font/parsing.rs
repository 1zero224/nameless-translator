use anyhow::{Context, Result};
use koharu_core::TextData;
use serde::Deserialize;
use serde_json::json;

use super::taxonomy::FontCandidate;
use super::visuals::sample_text;

#[derive(Debug, Clone)]
pub(super) struct MimoCategoryResult {
    pub(super) primary_category: String,
    pub(super) secondary_category: String,
    pub(super) confidence: Option<f32>,
    pub(super) reasoning_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct MimoSelectionResult {
    pub(super) selected_font_id: String,
    pub(super) confidence: Option<f32>,
    pub(super) reasoning_summary: Option<String>,
}

pub(super) fn parse_category_response(raw_text: &str) -> Result<MimoCategoryResult> {
    #[derive(Deserialize)]
    struct Payload {
        primary_category: String,
        secondary_category: String,
        confidence: Option<f32>,
        reasoning_summary: Option<String>,
    }
    let payload: Payload = serde_json::from_str(&strip_json_wrapper(raw_text))
        .context("parse MIMO font category JSON")?;
    validate_primary_category(&payload.primary_category)?;
    validate_secondary_category(&payload.primary_category, &payload.secondary_category)?;
    Ok(MimoCategoryResult {
        primary_category: payload.primary_category,
        secondary_category: payload.secondary_category,
        confidence: payload.confidence,
        reasoning_summary: clean_optional(payload.reasoning_summary),
    })
}

pub(super) fn parse_selection_response(
    raw_text: &str,
    candidates: &[FontCandidate],
) -> Result<MimoSelectionResult> {
    #[derive(Deserialize)]
    struct Payload {
        selected_font_id: String,
        confidence: Option<f32>,
        reasoning_summary: Option<String>,
    }
    let payload: Payload = serde_json::from_str(&strip_json_wrapper(raw_text))
        .context("parse MIMO font selection JSON")?;
    if !candidates
        .iter()
        .any(|candidate| candidate.font_id == payload.selected_font_id)
    {
        anyhow::bail!(
            "selected font is not in candidates: {}",
            payload.selected_font_id
        );
    }
    Ok(MimoSelectionResult {
        selected_font_id: payload.selected_font_id,
        confidence: payload.confidence,
        reasoning_summary: clean_optional(payload.reasoning_summary),
    })
}

pub(super) fn category_prompt() -> &'static str {
    "Inspect the source manga lettering. Return only JSON with keys primary_category, secondary_category, confidence, reasoning_summary. primary_category must be sans_serif or serif. If sans_serif, secondary_category must be gothic or round. If serif, secondary_category must be mincho or kai."
}

pub(super) fn selection_prompt(text: &TextData, candidates: &[FontCandidate]) -> String {
    let candidates_json = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| candidate_json(index, candidate))
        .collect::<Vec<_>>();
    format!(
        "The grid tile 0 is the source crop. Candidate tiles follow left-to-right, top-to-bottom. Choose the candidate preview that best matches the source lettering style. Source text: {}. Candidates JSON: {}. Return only JSON with keys selected_font_id, confidence, reasoning_summary.",
        sample_text(text),
        serde_json::to_string(&candidates_json).unwrap_or_default()
    )
}

fn candidate_json(index: usize, candidate: &FontCandidate) -> serde_json::Value {
    json!({
        "tile": index + 1,
        "font_id": candidate.font_id,
        "family_name": candidate.family_name,
        "post_script_name": candidate.post_script_name,
        "category": [candidate.primary_category, candidate.secondary_category],
    })
}

fn strip_json_wrapper(raw_text: &str) -> String {
    let mut text = raw_text.trim();
    if text.starts_with("```") {
        if let Some(stripped) = text.strip_prefix("```json") {
            text = stripped;
        } else if let Some(stripped) = text.strip_prefix("```") {
            text = stripped;
        }
        if let Some(stripped) = text.trim().strip_suffix("```") {
            text = stripped;
        }
    }
    text.trim().to_string()
}

fn validate_primary_category(value: &str) -> Result<()> {
    match value {
        "sans_serif" | "serif" => Ok(()),
        _ => anyhow::bail!("invalid primary category: {value}"),
    }
}

fn validate_secondary_category(primary: &str, secondary: &str) -> Result<()> {
    match (primary, secondary) {
        ("sans_serif", "gothic" | "round") => Ok(()),
        ("serif", "mincho" | "kai") => Ok(()),
        _ => anyhow::bail!("invalid secondary category: {secondary} for {primary}"),
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
