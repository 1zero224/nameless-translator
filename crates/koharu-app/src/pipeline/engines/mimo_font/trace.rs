use koharu_core::{
    FontPrediction, FontWorkflowTrace, NamedFontPrediction, TextData, TextStyle, TextWorkflow,
};

use super::parsing::MimoCategoryResult;
use super::selection::MimoFontOutcome;
use super::taxonomy::{FontCandidate, infer_prediction_secondary};

pub(super) fn workflow_with_mimo_trace(text: &TextData, outcome: &MimoFontOutcome) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    let mut notes = outcome.notes.clone();
    if let Some(value) = outcome.category.confidence {
        notes.push(format!("mimo category confidence {value:.3}"));
    }
    if let Some(reasoning) = outcome.category.reasoning_summary.as_ref() {
        notes.push(format!("mimo category: {reasoning}"));
    }
    if let Some(selection) = outcome.selection.as_ref() {
        if let Some(value) = selection.confidence {
            notes.push(format!("mimo selection confidence {value:.3}"));
        }
        if let Some(reasoning) = selection.reasoning_summary.as_ref() {
            notes.push(format!("mimo selection: {reasoning}"));
        }
    }
    workflow.font_trace = Some(FontWorkflowTrace {
        primary_category: Some(outcome.category.primary_category.clone()),
        secondary_category: Some(outcome.category.secondary_category.clone()),
        candidate_fonts: outcome
            .candidates
            .iter()
            .map(|candidate| candidate.post_script_name.clone())
            .collect(),
        selected_font: outcome
            .selected
            .as_ref()
            .map(|candidate| candidate.post_script_name.clone()),
        notes,
        ..Default::default()
    });
    workflow
}

pub(super) fn style_with_font(text: &TextData, candidate: &FontCandidate) -> TextStyle {
    let mut style = text.style.clone().unwrap_or_default();
    style.font_families = vec![candidate.post_script_name.clone()];
    style
}

pub(super) fn apply_outcome_to_prediction(
    prediction: &mut FontPrediction,
    outcome: &MimoFontOutcome,
) {
    let mut named_fonts = outcome
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| NamedFontPrediction {
            index,
            name: candidate.post_script_name.clone(),
            language: Some("ja".to_string()),
            probability: outcome
                .selection
                .as_ref()
                .filter(|selection| selection.selected_font_id == candidate.font_id)
                .and_then(|selection| selection.confidence)
                .unwrap_or(0.0),
            serif: candidate.primary_category == "serif",
        })
        .collect::<Vec<_>>();
    if let Some(selected) = outcome.selected.as_ref()
        && let Some(pos) = named_fonts
            .iter()
            .position(|font| font.name == selected.post_script_name)
    {
        named_fonts.swap(0, pos);
    }
    prediction.named_fonts = named_fonts;
}

pub(super) fn fallback_outcome(
    prediction: &FontPrediction,
    error: anyhow::Error,
) -> MimoFontOutcome {
    let selected = prediction.named_fonts.first().map(|font| {
        let primary = if font.serif { "serif" } else { "sans_serif" };
        let secondary = infer_prediction_secondary(font);
        FontCandidate {
            font_id: font.name.clone(),
            family_name: font.name.clone(),
            post_script_name: font.name.clone(),
            primary_category: primary.to_string(),
            secondary_category: secondary,
        }
    });
    let category = selected
        .as_ref()
        .map(|candidate| MimoCategoryResult {
            primary_category: candidate.primary_category.clone(),
            secondary_category: candidate.secondary_category.clone(),
            confidence: Some(0.0),
            reasoning_summary: Some("deterministic fallback from YuzuMarker".to_string()),
        })
        .unwrap_or_else(fallback_category);
    MimoFontOutcome {
        category,
        candidates: selected.iter().cloned().collect(),
        selected,
        selection: None,
        notes: vec![format!("mimo fallback: {error:#}")],
    }
}

pub(super) fn ml_prediction_to_core(p: koharu_ml::types::FontPrediction) -> FontPrediction {
    FontPrediction {
        top_fonts: p
            .top_fonts
            .into_iter()
            .map(|tf| koharu_core::TopFont {
                index: tf.index,
                score: tf.score,
            })
            .collect(),
        named_fonts: p
            .named_fonts
            .into_iter()
            .map(|nf| NamedFontPrediction {
                index: nf.index,
                name: nf.name,
                language: nf.language,
                probability: nf.probability,
                serif: nf.serif,
            })
            .collect(),
        direction: match p.direction {
            koharu_ml::types::TextDirection::Horizontal => koharu_core::TextDirection::Horizontal,
            koharu_ml::types::TextDirection::Vertical => koharu_core::TextDirection::Vertical,
        },
        text_color: p.text_color,
        stroke_color: p.stroke_color,
        font_size_px: p.font_size_px,
        stroke_width_px: p.stroke_width_px,
        line_height: p.line_height,
        angle_deg: p.angle_deg,
    }
}

pub(super) fn normalize_font_prediction(p: &mut koharu_ml::types::FontPrediction) {
    p.text_color = clamp_white(clamp_black(p.text_color));
    p.stroke_color = clamp_white(clamp_black(p.stroke_color));
    if p.stroke_width_px > 0.0 && colors_similar(p.text_color, p.stroke_color) {
        p.stroke_width_px = 0.0;
        p.stroke_color = p.text_color;
    }
}

fn fallback_category() -> MimoCategoryResult {
    MimoCategoryResult {
        primary_category: "sans_serif".to_string(),
        secondary_category: "gothic".to_string(),
        confidence: Some(0.0),
        reasoning_summary: Some("no font prediction was available".to_string()),
    }
}

fn clamp_black(c: [u8; 3]) -> [u8; 3] {
    let t = if gray(c) { 60 } else { 12 };
    if c[0] <= t && c[1] <= t && c[2] <= t {
        [0, 0, 0]
    } else {
        c
    }
}

fn clamp_white(c: [u8; 3]) -> [u8; 3] {
    let t = 255 - if gray(c) { 60 } else { 12 };
    if c[0] >= t && c[1] >= t && c[2] >= t {
        [255, 255, 255]
    } else {
        c
    }
}

fn gray(c: [u8; 3]) -> bool {
    c.iter().max().unwrap().abs_diff(*c.iter().min().unwrap()) <= 10
}

fn colors_similar(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= 16)
}
