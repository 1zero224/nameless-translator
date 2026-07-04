use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use image::DynamicImage;
use koharu_core::{
    FontBucket, FontFaceInfo, FontPolicy, FontPrediction, FontProfileRisk, FontProfileStatus,
    FontReviewItemStatus, FontReviewPriority, FontReviewQueueItem, FontReviewState, FontStyleGroup,
    FontStyleProfile, FontStyleRole, FontWorkflowTrace, NamedFontPrediction, NodeId, ProjectStyle,
    TextData, TextStyle,
};
use serde::Serialize;

use super::client::MimoFontClient;
use super::visuals::encode_png;

pub(super) const FONT_EVIDENCE_TOP_K: usize = 5;

const PROFILE_ID: &str = "font_profile_main";
const BODY_GROUP_ID: &str = "body_bubble_primary";
const UNKNOWN_STYLE_ID: &str = "unknown/new_style";

#[derive(Debug, Clone)]
pub(super) struct FontEvidence {
    pub(super) block_id: NodeId,
    pub(super) prediction: FontPrediction,
}

#[derive(Debug, Clone)]
pub(super) struct FontApplication {
    pub(super) style: Option<TextStyle>,
    pub(super) trace: FontWorkflowTrace,
    pub(super) review_item: Option<FontReviewQueueItem>,
    pub(super) skipped_manual_override: bool,
}

#[derive(Debug, Clone)]
struct BlockClassification {
    style_group_id: String,
    role: FontStyleRole,
    target_bucket: String,
    preserve_source_style: bool,
    confidence: f32,
    needs_review: bool,
    review_priority: FontReviewPriority,
    risk_reasons: Vec<String>,
    source_primary_category: Option<String>,
    source_secondary_category: Option<String>,
}

pub(super) fn active_profile(style: &ProjectStyle) -> Option<&FontStyleProfile> {
    style
        .font_profile
        .as_ref()
        .filter(|profile| profile.status != FontProfileStatus::Archived)
}

pub(super) fn build_auto_profile(evidence: &[FontEvidence], source: &str) -> FontStyleProfile {
    let mut secondary_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut representatives = Vec::new();
    for item in evidence {
        if representatives.len() < 6 {
            representatives.push(item.block_id);
        }
        if let Some(font) = top_yuzumarker_candidates(&item.prediction).first() {
            *secondary_counts
                .entry(infer_secondary_category(font))
                .or_insert(0) += 1;
        }
    }

    let mut source_categories = secondary_counts.keys().cloned().collect::<Vec<_>>();
    if source_categories.is_empty() {
        source_categories.push("unknown".to_string());
    }
    let mut profile_risks = Vec::new();
    if source_categories.iter().any(|v| v == "mincho")
        && source_categories.iter().any(|v| v == "gothic")
    {
        profile_risks.push(FontProfileRisk {
            kind: "mixed_body_baseline".to_string(),
            severity: "medium".to_string(),
            message: "Main bubble body appears to mix Mincho and Gothic evidence.".to_string(),
        });
    }

    let total = evidence.len().max(1) as f32;
    let profile_confidence = if evidence.is_empty() { 0.45 } else { 0.82 };
    let body_group = FontStyleGroup {
        id: BODY_GROUP_ID.to_string(),
        label: "气泡正文".to_string(),
        role: FontStyleRole::BubbleBody,
        description: "Project-calibrated normal dialogue baseline. Source categories can mix, but policy maps the group to the body bucket.".to_string(),
        source_categories: source_categories.clone(),
        preserve_source_style: false,
        target_bucket: "body".to_string(),
        representative_blocks: representatives.iter().copied().take(3).collect(),
        confidence: profile_confidence,
        needs_review: profile_confidence < 0.85 || !profile_risks.is_empty(),
        risk_reasons: profile_risks
            .iter()
            .map(|risk| risk.kind.clone())
            .collect(),
        distinguishing_features: vec![
            "Generated from project-level recurring lettering evidence".to_string(),
            "Used automatically without pre-run user confirmation".to_string(),
        ],
        possible_confusions: Vec::new(),
    };

    let mut style_groups = vec![body_group];
    if let Some(round_count) = secondary_counts.get("round")
        && (*round_count as f32 / total) < 0.5
    {
        style_groups.push(FontStyleGroup {
            id: "bubble_round_emphasis".to_string(),
            label: "气泡圆体强调".to_string(),
            role: FontStyleRole::BubbleEmphasis,
            description:
                "Rounded style that may represent emphasis relative to the project baseline."
                    .to_string(),
            source_categories: vec!["round".to_string()],
            preserve_source_style: true,
            target_bucket: "round".to_string(),
            representative_blocks: representatives.iter().copied().take(3).collect(),
            confidence: 0.68,
            needs_review: true,
            risk_reasons: vec![
                "low_sample_count".to_string(),
                "visually_close_to_body".to_string(),
            ],
            distinguishing_features: vec!["Rounded terminals in YuzuMarker evidence".to_string()],
            possible_confusions: vec![BODY_GROUP_ID.to_string()],
        });
    }

    FontStyleProfile {
        id: PROFILE_ID.to_string(),
        version: 1,
        status: FontProfileStatus::AutoActive,
        review_state: FontReviewState::Unreviewed,
        source: source.to_string(),
        profile_confidence,
        profile_risks,
        style_groups,
        previous_versions: Vec::new(),
        change_log: Vec::new(),
    }
}

pub(super) async fn build_mimo_profile(
    client: &MimoFontClient,
    page_overview: &DynamicImage,
    evidence: &[FontEvidence],
) -> Result<FontStyleProfile> {
    let overview_png = encode_png(page_overview)?;
    let raw = client
        .analyze_image(
            &overview_png,
            &profile_prompt(evidence),
            "You build manga project font style profiles. Return strict JSON only.",
        )
        .await?;
    let mut profile = parse_profile_response(&raw)?;
    normalize_profile(&mut profile, evidence);
    Ok(profile)
}

pub(super) fn mark_profile_generation_fallback(
    profile: &mut FontStyleProfile,
    message: impl Into<String>,
) {
    profile.profile_risks.push(FontProfileRisk {
        kind: "mimo_profile_generation_failed".to_string(),
        severity: "medium".to_string(),
        message: message.into(),
    });
    profile
        .style_groups
        .iter_mut()
        .for_each(|group| group.needs_review = true);
}

pub(super) fn default_font_policy(
    fonts: &[FontFaceInfo],
    project_default_font: Option<&str>,
    run_default_font: Option<&str>,
) -> FontPolicy {
    let fallback = run_default_font
        .filter(|value| !value.trim().is_empty())
        .or(project_default_font.filter(|value| !value.trim().is_empty()))
        .map(str::to_string)
        .or_else(|| fonts.first().map(|font| font.post_script_name.clone()))
        .unwrap_or_else(|| "ArialMT".to_string());
    let mut buckets = Default::default();
    buckets.insert(
        "body".to_string(),
        FontBucket {
            fonts: unique_fonts([Some(fallback.clone()), first_by_secondary(fonts, "gothic")]),
        },
    );
    buckets.insert(
        "round".to_string(),
        FontBucket {
            fonts: unique_fonts([first_by_secondary(fonts, "round"), Some(fallback.clone())]),
        },
    );
    buckets.insert(
        "mincho".to_string(),
        FontBucket {
            fonts: unique_fonts([first_by_secondary(fonts, "mincho"), Some(fallback.clone())]),
        },
    );
    buckets.insert(
        "display".to_string(),
        FontBucket {
            fonts: unique_fonts([first_by_secondary(fonts, "gothic"), Some(fallback.clone())]),
        },
    );
    buckets.insert(
        "review".to_string(),
        FontBucket {
            fonts: vec![fallback.clone()],
        },
    );
    FontPolicy {
        buckets,
        fallback_font: Some(fallback),
    }
}

pub(super) fn apply_font_profile(
    node_id: NodeId,
    text: &TextData,
    prediction: &FontPrediction,
    profile: &FontStyleProfile,
    policy: &FontPolicy,
    fonts: &[FontFaceInfo],
) -> FontApplication {
    if text
        .workflow
        .font_trace
        .as_ref()
        .is_some_and(|trace| trace.manual_override)
    {
        let mut trace = text.workflow.font_trace.clone().unwrap_or_default();
        trace.auto_applied = Some(false);
        trace.needs_review = Some(false);
        trace
            .notes
            .push("manual override protected from automatic font policy".to_string());
        return FontApplication {
            style: None,
            trace,
            review_item: None,
            skipped_manual_override: true,
        };
    }

    let mut classification = classify_block(prediction, profile);
    let (selected_font, fallback_reason) = resolve_bucket_font(
        policy,
        &classification.target_bucket,
        fonts,
        policy.fallback_font.as_deref(),
    );
    if let Some(reason) = fallback_reason {
        classification.needs_review = true;
        classification.review_priority = FontReviewPriority::High;
        classification.risk_reasons.push(reason);
    }

    let previous = text.style.clone().unwrap_or_default();
    let mut style = previous.clone();
    style.font_families = vec![selected_font.clone()];
    let review_item = classification.needs_review.then(|| FontReviewQueueItem {
        id: format!("font-review-{}-{}-{node_id}", profile.id, profile.version),
        block_id: node_id,
        profile_id: profile.id.clone(),
        profile_version: profile.version,
        style_group_id: classification.style_group_id.clone(),
        review_priority: classification.review_priority,
        risk_reasons: classification.risk_reasons.clone(),
        suggested_action: if classification.style_group_id == UNKNOWN_STYLE_ID {
            "create_or_assign_style_group".to_string()
        } else {
            "review_style_group".to_string()
        },
        status: FontReviewItemStatus::Open,
    });

    FontApplication {
        style: Some(style),
        trace: build_trace(
            text,
            prediction,
            profile,
            &classification,
            selected_font,
            previous,
        ),
        review_item,
        skipped_manual_override: false,
    }
}

pub(super) fn merge_review_queue(
    existing: &[FontReviewQueueItem],
    additions: impl IntoIterator<Item = FontReviewQueueItem>,
) -> Vec<FontReviewQueueItem> {
    let mut merged = existing.to_vec();
    let mut seen = merged
        .iter()
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    for item in additions {
        if seen.insert(item.id.clone()) {
            merged.push(item);
        }
    }
    merged
}

fn classify_block(prediction: &FontPrediction, profile: &FontStyleProfile) -> BlockClassification {
    let top = top_yuzumarker_candidates(prediction).first().cloned();
    let source_primary_category = top.as_ref().map(|font| {
        if font.serif {
            "serif".to_string()
        } else {
            "sans_serif".to_string()
        }
    });
    let source_secondary_category = top.as_ref().map(infer_secondary_category);
    if top.is_none() {
        return unknown_classification(source_primary_category, source_secondary_category);
    }

    if let Some(group) = source_secondary_category.as_ref().and_then(|category| {
        profile
            .style_groups
            .iter()
            .find(|group| group.id != BODY_GROUP_ID && group.source_categories.contains(category))
    }) {
        let mut risk_reasons = group.risk_reasons.clone();
        let mut priority = FontReviewPriority::None;
        let mut needs_review = group.needs_review;
        let confidence = group.confidence.min(0.78);
        if confidence < 0.85 {
            needs_review = true;
            priority = if confidence < 0.65 {
                FontReviewPriority::High
            } else {
                FontReviewPriority::Medium
            };
            risk_reasons.push("classification_confidence_below_high_threshold".to_string());
        }
        return BlockClassification {
            style_group_id: group.id.clone(),
            role: group.role,
            target_bucket: group.target_bucket.clone(),
            preserve_source_style: group.preserve_source_style,
            confidence,
            needs_review,
            review_priority: priority,
            risk_reasons,
            source_primary_category,
            source_secondary_category,
        };
    }

    if let Some(group) = profile
        .style_groups
        .iter()
        .find(|group| group.id == BODY_GROUP_ID)
    {
        return BlockClassification {
            style_group_id: group.id.clone(),
            role: group.role,
            target_bucket: group.target_bucket.clone(),
            preserve_source_style: group.preserve_source_style,
            confidence: 0.88_f32.min(group.confidence.max(0.66)),
            needs_review: group.needs_review && group.confidence < 0.85,
            review_priority: if group.needs_review && group.confidence < 0.85 {
                FontReviewPriority::Medium
            } else {
                FontReviewPriority::None
            },
            risk_reasons: group.risk_reasons.clone(),
            source_primary_category,
            source_secondary_category,
        };
    }

    unknown_classification(source_primary_category, source_secondary_category)
}

fn profile_prompt(evidence: &[FontEvidence]) -> String {
    let evidence = evidence.iter().map(EvidenceDto::from).collect::<Vec<_>>();
    format!(
        "You are building an automatic font style profile for one manga project. \
Analyze the page overview and the YuzuMarker top-k evidence. Identify recurring text style groups relative to this manga project. \
Roles must be one of bubble_body, bubble_emphasis, outside_bubble, caption, sfx, unknown. \
YuzuMarker evidence is advisory only. Do not choose installed fonts. Do not ask the user for confirmation. \
Return strict JSON using camelCase keys matching this schema: \
{{\"id\":\"font_profile_main\",\"version\":1,\"status\":\"auto_active\",\"reviewState\":\"unreviewed\",\"source\":\"mimo_calibrated\",\"profileConfidence\":0.82,\"profileRisks\":[],\"styleGroups\":[{{\"id\":\"body_bubble_primary\",\"label\":\"气泡正文\",\"role\":\"bubble_body\",\"description\":\"...\",\"sourceCategories\":[\"gothic\"],\"preserveSourceStyle\":false,\"targetBucket\":\"body\",\"representativeBlocks\":[],\"confidence\":0.9,\"needsReview\":false,\"riskReasons\":[],\"distinguishingFeatures\":[],\"possibleConfusions\":[]}}],\"previousVersions\":[],\"changeLog\":[]}}. \
If normal speech-bubble body mixes Mincho and Gothic in this project, put both categories in one bubble_body style group and targetBucket body. \
If uncertain or visually close, set needsReview true and include riskReasons. Evidence JSON: {}",
        serde_json::to_string(&evidence).unwrap_or_else(|_| "[]".to_string())
    )
}

fn parse_profile_response(raw_text: &str) -> Result<FontStyleProfile> {
    serde_json::from_str(&strip_json_wrapper(raw_text)).context("parse MIMO font profile JSON")
}

fn normalize_profile(profile: &mut FontStyleProfile, evidence: &[FontEvidence]) {
    if profile.id.trim().is_empty() {
        profile.id = PROFILE_ID.to_string();
    }
    if profile.version == 0 {
        profile.version = 1;
    }
    profile.status = FontProfileStatus::AutoActive;
    profile.review_state = FontReviewState::Unreviewed;
    if profile.source.trim().is_empty() {
        profile.source = "mimo_calibrated".to_string();
    }
    if profile.style_groups.is_empty() {
        *profile = build_auto_profile(evidence, "yuzumarker_bootstrap");
        mark_profile_generation_fallback(
            profile,
            "MIMO returned no style groups; used YuzuMarker bootstrap profile.",
        );
        return;
    }
    for group in &mut profile.style_groups {
        if group.target_bucket.trim().is_empty() {
            group.target_bucket = "review".to_string();
            group.needs_review = true;
            group.risk_reasons.push("missing_target_bucket".to_string());
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvidenceDto {
    block_id: String,
    top_fonts: Vec<NamedFontPrediction>,
    font_size_px: f32,
    stroke_width_px: f32,
    direction: koharu_core::TextDirection,
    text_color: [u8; 3],
}

impl From<&FontEvidence> for EvidenceDto {
    fn from(value: &FontEvidence) -> Self {
        Self {
            block_id: value.block_id.to_string(),
            top_fonts: top_yuzumarker_candidates(&value.prediction),
            font_size_px: value.prediction.font_size_px,
            stroke_width_px: value.prediction.stroke_width_px,
            direction: value.prediction.direction,
            text_color: value.prediction.text_color,
        }
    }
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

fn unknown_classification(
    source_primary_category: Option<String>,
    source_secondary_category: Option<String>,
) -> BlockClassification {
    BlockClassification {
        style_group_id: UNKNOWN_STYLE_ID.to_string(),
        role: FontStyleRole::Unknown,
        target_bucket: "review".to_string(),
        preserve_source_style: true,
        confidence: 0.46,
        needs_review: true,
        review_priority: FontReviewPriority::High,
        risk_reasons: vec![
            "unknown_new_style".to_string(),
            "no_matching_reviewed_style_group".to_string(),
        ],
        source_primary_category,
        source_secondary_category,
    }
}

fn resolve_bucket_font(
    policy: &FontPolicy,
    bucket: &str,
    fonts: &[FontFaceInfo],
    fallback: Option<&str>,
) -> (String, Option<String>) {
    let available = fonts
        .iter()
        .map(|font| font.post_script_name.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(font) = policy
        .buckets
        .get(bucket)
        .into_iter()
        .flat_map(|bucket| bucket.fonts.iter())
        .find(|font| available.is_empty() || available.contains(font.as_str()))
    {
        return (font.clone(), None);
    }
    let fallback = fallback
        .filter(|font| !font.trim().is_empty())
        .or_else(|| fonts.first().map(|font| font.post_script_name.as_str()))
        .unwrap_or("ArialMT");
    (
        fallback.to_string(),
        Some(if policy.buckets.contains_key(bucket) {
            "selected_font_fallback".to_string()
        } else {
            "font_bucket_unavailable".to_string()
        }),
    )
}

fn build_trace(
    _text: &TextData,
    prediction: &FontPrediction,
    profile: &FontStyleProfile,
    classification: &BlockClassification,
    selected_font: String,
    previous: TextStyle,
) -> FontWorkflowTrace {
    let candidates = top_yuzumarker_candidates(prediction);
    let mut notes = vec![
        "YuzuMarker evidence used as advisory signal".to_string(),
        "MIMO-calibrated project profile controls style-group interpretation".to_string(),
        "FontPolicyResolver mapped target bucket to final font".to_string(),
    ];
    if profile.review_state == FontReviewState::Unreviewed {
        notes.push("profile is auto-generated and unreviewed".to_string());
    }
    FontWorkflowTrace {
        primary_category: classification.source_primary_category.clone(),
        secondary_category: classification.source_secondary_category.clone(),
        candidate_fonts: candidates.iter().map(|font| font.name.clone()).collect(),
        selected_font: Some(selected_font),
        notes,
        provider: Some("mimo_yuzumarker_guided".to_string()),
        profile_id: Some(profile.id.clone()),
        profile_version: Some(profile.version),
        profile_status: Some(profile.status),
        yuzumarker_candidates: candidates,
        text_role: Some(classification.role),
        style_group_id: Some(classification.style_group_id.clone()),
        source_primary_category: classification.source_primary_category.clone(),
        source_secondary_category: classification.source_secondary_category.clone(),
        source_weight: Some("regular".to_string()),
        source_emphasis: Some(if classification.role == FontStyleRole::BubbleBody {
            "normal".to_string()
        } else {
            "emphasis_or_display".to_string()
        }),
        preserve_source_style: Some(classification.preserve_source_style),
        recommended_font_bucket: Some(classification.target_bucket.clone()),
        policy: Some("vision_model_guided_font_policy".to_string()),
        confidence: Some(classification.confidence),
        auto_applied: Some(true),
        needs_review: Some(classification.needs_review),
        review_priority: classification.review_priority,
        risk_reasons: classification.risk_reasons.clone(),
        manual_override: false,
        previous_font_families: previous.font_families,
        previous_font_size: previous.font_size,
    }
}

fn top_yuzumarker_candidates(prediction: &FontPrediction) -> Vec<NamedFontPrediction> {
    prediction
        .named_fonts
        .iter()
        .take(FONT_EVIDENCE_TOP_K)
        .cloned()
        .collect()
}

fn first_by_secondary(fonts: &[FontFaceInfo], secondary: &str) -> Option<String> {
    fonts
        .iter()
        .find(|font| infer_font_face_secondary(font) == secondary)
        .map(|font| font.post_script_name.clone())
}

fn unique_fonts(values: impl IntoIterator<Item = Option<String>>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for value in values.into_iter().flatten() {
        if !value.trim().is_empty() && seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn infer_secondary_category(font: &NamedFontPrediction) -> String {
    infer_secondary_from_name(&font.name, font.serif)
}

fn infer_font_face_secondary(font: &FontFaceInfo) -> String {
    let category = font.category.as_deref().unwrap_or_default().to_lowercase();
    if category.contains("serif") {
        return infer_secondary_from_name(&font.post_script_name, true);
    }
    infer_secondary_from_name(
        &format!("{} {}", font.family_name, font.post_script_name),
        false,
    )
}

fn infer_secondary_from_name(name: &str, serif: bool) -> String {
    let lower = name.to_lowercase();
    if serif {
        if contains_any(&lower, &["kai", "kaiti", "klee", "楷", "行書"]) {
            "kai".to_string()
        } else {
            "mincho".to_string()
        }
    } else if contains_any(&lower, &["round", "rounded", "maru", "丸", "圆", "圓"]) {
        "round".to_string()
    } else {
        "gothic".to_string()
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use koharu_core::{FontSource, TextWorkflow};

    use super::*;

    fn node_id() -> NodeId {
        NodeId::new()
    }

    fn named(name: &str, probability: f32, serif: bool) -> NamedFontPrediction {
        NamedFontPrediction {
            index: 0,
            name: name.to_string(),
            language: Some("ja".to_string()),
            probability,
            serif,
        }
    }

    fn prediction(fonts: Vec<NamedFontPrediction>) -> FontPrediction {
        FontPrediction {
            named_fonts: fonts,
            ..Default::default()
        }
    }

    fn face(post_script_name: &str, family_name: &str, category: Option<&str>) -> FontFaceInfo {
        FontFaceInfo {
            family_name: family_name.to_string(),
            post_script_name: post_script_name.to_string(),
            source: FontSource::System,
            category: category.map(str::to_string),
            cached: true,
        }
    }

    #[test]
    fn builds_auto_profile_with_mixed_body_baseline_from_top_k_evidence() {
        let a = node_id();
        let b = node_id();
        let profile = build_auto_profile(
            &[
                FontEvidence {
                    block_id: a,
                    prediction: prediction(vec![
                        named("YuMincho-Regular", 0.42, true),
                        named("GothicMB101-Bold", 0.31, false),
                    ]),
                },
                FontEvidence {
                    block_id: b,
                    prediction: prediction(vec![named("YuGothic-Regular", 0.61, false)]),
                },
            ],
            "mimo_calibrated",
        );

        assert_eq!(profile.status, FontProfileStatus::AutoActive);
        assert_eq!(profile.review_state, FontReviewState::Unreviewed);
        let body = profile
            .style_groups
            .iter()
            .find(|group| group.id == BODY_GROUP_ID)
            .expect("body group");
        assert_eq!(body.target_bucket, "body");
        assert!(!body.preserve_source_style);
        assert!(body.source_categories.contains(&"mincho".to_string()));
        assert!(body.source_categories.contains(&"gothic".to_string()));
        assert_eq!(body.representative_blocks, vec![a, b]);
        assert_eq!(profile.profile_risks[0].kind, "mixed_body_baseline");
    }

    #[test]
    fn policy_maps_body_to_bucket_font_and_records_trace() {
        let block_id = node_id();
        let profile = build_auto_profile(
            &[FontEvidence {
                block_id,
                prediction: prediction(vec![named("YuMincho-Regular", 0.9, true)]),
            }],
            "mimo_calibrated",
        );
        let fonts = vec![face("BodyFontPS", "Body Font", None)];
        let policy = default_font_policy(&fonts, None, Some("BodyFontPS"));
        let text = TextData::default();

        let applied = apply_font_profile(
            block_id,
            &text,
            &prediction(vec![named("YuMincho-Regular", 0.9, true)]),
            &profile,
            &policy,
            &fonts,
        );

        assert_eq!(
            applied.style.expect("style").font_families,
            vec!["BodyFontPS"]
        );
        assert_eq!(applied.trace.profile_id.as_deref(), Some(PROFILE_ID));
        assert_eq!(applied.trace.style_group_id.as_deref(), Some(BODY_GROUP_ID));
        assert_eq!(
            applied.trace.recommended_font_bucket.as_deref(),
            Some("body")
        );
        assert_eq!(applied.trace.yuzumarker_candidates.len(), 1);
        assert!(applied.review_item.is_some());
    }

    #[test]
    fn unknown_style_uses_review_bucket_and_high_priority_queue_item() {
        let block_id = node_id();
        let profile = FontStyleProfile {
            style_groups: Vec::new(),
            ..Default::default()
        };
        let policy = default_font_policy(&[], None, Some("ReviewFont"));
        let text = TextData::default();

        let applied = apply_font_profile(
            block_id,
            &text,
            &prediction(Vec::new()),
            &profile,
            &policy,
            &[],
        );

        assert_eq!(
            applied.style.expect("style").font_families,
            vec!["ReviewFont"]
        );
        let item = applied.review_item.expect("review item");
        assert_eq!(item.review_priority, FontReviewPriority::High);
        assert_eq!(item.style_group_id, UNKNOWN_STYLE_ID);
        assert!(item.risk_reasons.contains(&"unknown_new_style".to_string()));
    }

    #[test]
    fn unavailable_bucket_falls_back_and_marks_review() {
        let block_id = node_id();
        let mut profile = build_auto_profile(
            &[FontEvidence {
                block_id,
                prediction: prediction(vec![named("RoundedGothic", 0.7, false)]),
            }],
            "mimo_calibrated",
        );
        profile.style_groups = vec![FontStyleGroup {
            id: "bubble_round_emphasis".to_string(),
            label: "round".to_string(),
            role: FontStyleRole::BubbleEmphasis,
            source_categories: vec!["round".to_string()],
            preserve_source_style: true,
            target_bucket: "round".to_string(),
            confidence: 0.72,
            ..profile.style_groups[0].clone()
        }];
        let mut policy = FontPolicy::default();
        policy.fallback_font = Some("FallbackFont".to_string());
        policy.buckets.insert(
            "round".to_string(),
            FontBucket {
                fonts: vec!["MissingRoundFont".to_string()],
            },
        );

        let applied = apply_font_profile(
            block_id,
            &TextData::default(),
            &prediction(vec![named("RoundedGothic", 0.7, false)]),
            &profile,
            &policy,
            &[face("AvailableFont", "Available", None)],
        );

        assert_eq!(
            applied.style.expect("style").font_families,
            vec!["FallbackFont"]
        );
        let item = applied.review_item.expect("review item");
        assert_eq!(item.review_priority, FontReviewPriority::High);
        assert!(
            item.risk_reasons
                .contains(&"selected_font_fallback".to_string())
        );
    }

    #[test]
    fn manual_override_skips_auto_application() {
        let block_id = node_id();
        let profile = build_auto_profile(&[], "mimo_calibrated");
        let policy = default_font_policy(&[], None, Some("BodyFont"));
        let text = TextData {
            workflow: TextWorkflow {
                font_trace: Some(FontWorkflowTrace {
                    manual_override: true,
                    selected_font: Some("ManualFont".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let applied = apply_font_profile(
            block_id,
            &text,
            &prediction(vec![named("YuGothic", 0.9, false)]),
            &profile,
            &policy,
            &[],
        );

        assert!(applied.skipped_manual_override);
        assert!(applied.style.is_none());
        assert!(applied.review_item.is_none());
        assert_eq!(applied.trace.selected_font.as_deref(), Some("ManualFont"));
        assert_eq!(applied.trace.auto_applied, Some(false));
    }

    #[test]
    fn merge_review_queue_deduplicates_by_item_id() {
        let block_id = node_id();
        let item = FontReviewQueueItem {
            id: "font-review-1".to_string(),
            block_id,
            profile_id: PROFILE_ID.to_string(),
            profile_version: 1,
            style_group_id: BODY_GROUP_ID.to_string(),
            review_priority: FontReviewPriority::Medium,
            risk_reasons: Vec::new(),
            suggested_action: "review_style_group".to_string(),
            status: FontReviewItemStatus::Open,
        };

        let merged = merge_review_queue(&[item.clone()], [item]);

        assert_eq!(merged.len(), 1);
    }
}
