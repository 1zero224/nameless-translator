use anyhow::Result;
use image::DynamicImage;
use koharu_core::{FontFaceInfo, FontPrediction, TextData, Transform};

use crate::renderer::Renderer;

use super::MAX_CANDIDATES;
use super::client::MimoFontClient;
use super::parsing::{
    MimoCategoryResult, MimoSelectionResult, category_prompt, parse_category_response,
    parse_selection_response, selection_prompt,
};
use super::taxonomy::{FontCandidate, select_font_candidates};
use super::visuals::{build_comparison_grid, encode_png};

#[derive(Debug, Clone)]
pub(super) struct MimoFontOutcome {
    pub(super) category: MimoCategoryResult,
    pub(super) candidates: Vec<FontCandidate>,
    pub(super) selected: Option<FontCandidate>,
    pub(super) selection: Option<MimoSelectionResult>,
    pub(super) notes: Vec<String>,
}

pub(super) async fn choose_font_with_mimo(
    client: &MimoFontClient,
    renderer: &Renderer,
    crop: &DynamicImage,
    transform: &Transform,
    text: &TextData,
    prediction: &FontPrediction,
    fonts: &[FontFaceInfo],
) -> Result<MimoFontOutcome> {
    let crop_png = encode_png(crop)?;
    let category_raw = client
        .analyze_image(
            &crop_png,
            category_prompt(),
            "You classify manga lettering fonts. Return compact JSON only.",
        )
        .await?;
    let category = parse_category_response(&category_raw)?;
    let candidates = select_font_candidates(
        fonts,
        &category.primary_category,
        &category.secondary_category,
        MAX_CANDIDATES,
    );
    if candidates.is_empty() {
        anyhow::bail!("no usable font candidates for MIMO font selection");
    }

    let comparison =
        build_comparison_grid(renderer, crop, transform, text, prediction, &candidates)?;
    let comparison_png = encode_png(&comparison)?;
    let selection_raw = client
        .analyze_image(
            &comparison_png,
            &selection_prompt(text, &candidates),
            "You select manga lettering fonts from a comparison grid. Return compact JSON only.",
        )
        .await?;
    let selection = parse_selection_response(&selection_raw, &candidates)?;
    let selected = candidates
        .iter()
        .find(|candidate| candidate.font_id == selection.selected_font_id)
        .cloned();
    Ok(MimoFontOutcome {
        category,
        candidates,
        selected,
        selection: Some(selection),
        notes: vec![
            "mimo category classified from source crop".to_string(),
            "mimo selected from rendered comparison grid".to_string(),
        ],
    })
}
