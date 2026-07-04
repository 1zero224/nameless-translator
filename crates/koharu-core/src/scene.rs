//! Scene graph: Project → Pages → flat Nodes.
//!
//! Three primitives: `Node`, `Blob` (via `BlobRef`), `Op` (in `op.rs`).
//! Everything visual on a page is a `Node`; scene mutations flow through `Op`s.

// `NodeKind::Text` naturally carries more data than `Image`/`Mask`, and
// boxing would change the wire format. Same reasoning as in `op.rs`.
#![allow(clippy::large_enum_variant)]

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::blob::BlobRef;
use crate::font::{FontPrediction, NamedFontPrediction, TextDirection};
use crate::style::TextStyle;

// ---------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    ToSchema,
)]
#[serde(transparent)]
pub struct PageId(pub Uuid);

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    JsonSchema,
    ToSchema,
)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl PageId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for PageId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// Scene / Project
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub project: ProjectMeta,
    /// Pages in insertion order; `IndexMap` ordering *is* the page order.
    pub pages: IndexMap<PageId, Page>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            project: ProjectMeta::default(),
            pages: IndexMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub style: ProjectStyle,
}

impl Default for ProjectMeta {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            name: String::new(),
            created_at: now,
            updated_at: now,
            style: ProjectStyle::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStyle {
    #[serde(default)]
    pub default_font: Option<String>,
    #[serde(default)]
    pub font_profile: Option<FontStyleProfile>,
    #[serde(default)]
    pub font_policy: FontPolicy,
    #[serde(default)]
    pub font_review_queue: Vec<FontReviewQueueItem>,
}

impl<'de> Deserialize<'de> for ProjectStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "defaultFont",
            "fontProfile",
            "fontPolicy",
            "fontReviewQueue",
        ];

        if deserializer.is_human_readable() {
            return ProjectStyleRepr::deserialize(deserializer).map(Into::into);
        }
        deserializer.deserialize_struct("ProjectStyle", FIELDS, ProjectStyleVisitor)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStyleRepr {
    #[serde(default)]
    default_font: Option<String>,
    #[serde(default)]
    font_profile: Option<FontStyleProfile>,
    #[serde(default)]
    font_policy: FontPolicy,
    #[serde(default)]
    font_review_queue: Vec<FontReviewQueueItem>,
}

impl From<ProjectStyleRepr> for ProjectStyle {
    fn from(value: ProjectStyleRepr) -> Self {
        Self {
            default_font: value.default_font,
            font_profile: value.font_profile,
            font_policy: value.font_policy,
            font_review_queue: value.font_review_queue,
        }
    }
}

struct ProjectStyleVisitor;

impl<'de> serde::de::Visitor<'de> for ProjectStyleVisitor {
    type Value = ProjectStyle;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProjectStyle")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let default_font = seq.next_element()?.unwrap_or_default();
        let Some(font_profile) = optional_tail(&mut seq)? else {
            return Ok(ProjectStyle {
                default_font,
                ..Default::default()
            });
        };
        let Some(font_policy) = optional_tail(&mut seq)? else {
            return Ok(ProjectStyle {
                default_font,
                font_profile,
                ..Default::default()
            });
        };
        let Some(font_review_queue) = optional_tail(&mut seq)? else {
            return Ok(ProjectStyle {
                default_font,
                font_profile,
                font_policy,
                ..Default::default()
            });
        };
        Ok(ProjectStyle {
            default_font,
            font_profile,
            font_policy,
            font_review_queue,
        })
    }
}

fn optional_tail<'de, A, T>(seq: &mut A) -> Result<Option<T>, A::Error>
where
    A: serde::de::SeqAccess<'de>,
    T: Deserialize<'de> + Default,
{
    match seq.next_element::<T>() {
        Ok(value) => Ok(value.or_else(|| Some(T::default()))),
        Err(_) => Ok(None),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontStyleProfile {
    pub id: String,
    pub version: u32,
    pub status: FontProfileStatus,
    pub review_state: FontReviewState,
    pub source: String,
    pub profile_confidence: f32,
    #[serde(default)]
    pub profile_risks: Vec<FontProfileRisk>,
    #[serde(default)]
    pub style_groups: Vec<FontStyleGroup>,
    #[serde(default)]
    pub previous_versions: Vec<String>,
    #[serde(default)]
    pub change_log: Vec<FontProfileChange>,
}

impl Default for FontStyleProfile {
    fn default() -> Self {
        Self {
            id: "font_profile_main".to_string(),
            version: 1,
            status: FontProfileStatus::AutoActive,
            review_state: FontReviewState::Unreviewed,
            source: "mimo_calibrated".to_string(),
            profile_confidence: 0.0,
            profile_risks: Vec::new(),
            style_groups: Vec::new(),
            previous_versions: Vec::new(),
            change_log: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontProfileStatus {
    AutoDraft,
    AutoActive,
    ReviewedActive,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontReviewState {
    Unreviewed,
    Reviewed,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontProfileRisk {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontStyleGroup {
    pub id: String,
    pub label: String,
    pub role: FontStyleRole,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source_categories: Vec<String>,
    #[serde(default)]
    pub preserve_source_style: bool,
    pub target_bucket: String,
    #[serde(default)]
    pub representative_blocks: Vec<NodeId>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub risk_reasons: Vec<String>,
    #[serde(default)]
    pub distinguishing_features: Vec<String>,
    #[serde(default)]
    pub possible_confusions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FontStyleRole {
    BubbleBody,
    BubbleEmphasis,
    OutsideBubble,
    Caption,
    Sfx,
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontProfileChange {
    pub version: u32,
    pub change: String,
    #[serde(default)]
    pub affected_blocks: Vec<NodeId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontPolicy {
    #[serde(default)]
    pub buckets: IndexMap<String, FontBucket>,
    #[serde(default)]
    pub fallback_font: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontBucket {
    #[serde(default)]
    pub fonts: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontReviewQueueItem {
    pub id: String,
    pub block_id: NodeId,
    pub profile_id: String,
    pub profile_version: u32,
    pub style_group_id: String,
    pub review_priority: FontReviewPriority,
    #[serde(default)]
    pub risk_reasons: Vec<String>,
    #[serde(default)]
    pub suggested_action: String,
    #[serde(default)]
    pub status: FontReviewItemStatus,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FontReviewPriority {
    #[default]
    None,
    Medium,
    High,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FontReviewItemStatus {
    #[default]
    Open,
    Resolved,
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: PageId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Stacking = insertion order. Bottom-first: `source` is typically first,
    /// `rendered` typically last.
    pub nodes: IndexMap<NodeId, Node>,
}

impl Page {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id: PageId::new(),
            name: name.into(),
            width,
            height,
            nodes: IndexMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    #[serde(default)]
    pub transform: Transform,
    pub visible: bool,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    Image(ImageData),
    Text(TextData),
    Mask(MaskData),
}

impl NodeKind {
    pub fn discriminant(&self) -> NodeKindTag {
        match self {
            NodeKind::Image(_) => NodeKindTag::Image,
            NodeKind::Text(_) => NodeKindTag::Text,
            NodeKind::Mask(_) => NodeKindTag::Mask,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NodeKindTag {
    Image,
    Text,
    Mask,
}

// ---------------------------------------------------------------------------
// Image node
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageData {
    /// Role tags differentiate source / inpainted / rendered / user-imported images.
    /// Role is immutable on an existing node — switching roles = delete + add.
    pub role: ImageRole,
    pub blob: BlobRef,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    pub natural_width: u32,
    pub natural_height: u32,
    #[serde(default)]
    pub name: Option<String>,
}

const fn default_opacity() -> f32 {
    1.0
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ImageRole {
    /// Immutable page input; exactly one per page.
    Source,
    /// Pipeline output; text removed from `Source`.
    Inpainted,
    /// Pipeline output; final composite.
    Rendered,
    /// User-imported free layer, movable / selectable.
    Custom,
}

// ---------------------------------------------------------------------------
// Mask node
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MaskData {
    pub role: MaskRole,
    pub blob: BlobRef,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum MaskRole {
    /// Manual brush strokes driving local inpaint.
    BrushInpaint,
    /// Text-detector segmentation preview (text-pixel mask).
    Segment,
    /// Bubble-interior mask from `speech-bubble-segmentation`. The
    /// renderer grows text layout boxes inside this mask so English
    /// wraps into the available bubble space without leaking past the
    /// bubble border.
    Bubble,
}

// ---------------------------------------------------------------------------
// Text node
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Serialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextData {
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub source_lang: Option<String>,
    #[serde(default)]
    pub source_direction: Option<TextDirection>,
    #[serde(default)]
    pub rendered_direction: Option<TextDirection>,
    #[serde(default)]
    pub line_polygons: Option<Vec<[[f32; 2]; 4]>>,
    #[serde(default)]
    pub rotation_deg: Option<f32>,
    #[serde(default)]
    pub detected_font_size_px: Option<f32>,
    #[serde(default)]
    pub detector: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub translation: Option<String>,
    #[serde(default)]
    pub style: Option<TextStyle>,
    #[serde(default)]
    pub font_prediction: Option<FontPrediction>,
    /// Renderer-produced sprite for this block.
    #[serde(default)]
    pub sprite: Option<BlobRef>,
    /// Sprite placement when the renderer expands past the bubble geometry.
    #[serde(default)]
    pub sprite_transform: Option<Transform>,
    #[serde(default)]
    pub lock_layout_box: bool,
    #[serde(default)]
    pub workflow: TextWorkflow,
}

impl<'de> Deserialize<'de> for TextData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "confidence",
            "sourceLang",
            "sourceDirection",
            "renderedDirection",
            "linePolygons",
            "rotationDeg",
            "detectedFontSizePx",
            "detector",
            "text",
            "translation",
            "style",
            "fontPrediction",
            "sprite",
            "spriteTransform",
            "lockLayoutBox",
            "workflow",
        ];

        if deserializer.is_human_readable() {
            return TextDataRepr::deserialize(deserializer).map(Into::into);
        }
        deserializer.deserialize_struct("TextData", FIELDS, TextDataVisitor)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextDataRepr {
    #[serde(default)]
    confidence: f32,
    #[serde(default)]
    source_lang: Option<String>,
    #[serde(default)]
    source_direction: Option<TextDirection>,
    #[serde(default)]
    rendered_direction: Option<TextDirection>,
    #[serde(default)]
    line_polygons: Option<Vec<[[f32; 2]; 4]>>,
    #[serde(default)]
    rotation_deg: Option<f32>,
    #[serde(default)]
    detected_font_size_px: Option<f32>,
    #[serde(default)]
    detector: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    translation: Option<String>,
    #[serde(default)]
    style: Option<TextStyle>,
    #[serde(default)]
    font_prediction: Option<FontPrediction>,
    #[serde(default)]
    sprite: Option<BlobRef>,
    #[serde(default)]
    sprite_transform: Option<Transform>,
    #[serde(default)]
    lock_layout_box: bool,
    #[serde(default)]
    workflow: TextWorkflow,
}

impl From<TextDataRepr> for TextData {
    fn from(value: TextDataRepr) -> Self {
        Self {
            confidence: value.confidence,
            source_lang: value.source_lang,
            source_direction: value.source_direction,
            rendered_direction: value.rendered_direction,
            line_polygons: value.line_polygons,
            rotation_deg: value.rotation_deg,
            detected_font_size_px: value.detected_font_size_px,
            detector: value.detector,
            text: value.text,
            translation: value.translation,
            style: value.style,
            font_prediction: value.font_prediction,
            sprite: value.sprite,
            sprite_transform: value.sprite_transform,
            lock_layout_box: value.lock_layout_box,
            workflow: value.workflow,
        }
    }
}

struct TextDataVisitor;

impl<'de> serde::de::Visitor<'de> for TextDataVisitor {
    type Value = TextData;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TextData")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        Ok(TextData {
            confidence: seq.next_element()?.unwrap_or_default(),
            source_lang: seq.next_element()?.unwrap_or_default(),
            source_direction: seq.next_element()?.unwrap_or_default(),
            rendered_direction: seq.next_element()?.unwrap_or_default(),
            line_polygons: seq.next_element()?.unwrap_or_default(),
            rotation_deg: seq.next_element()?.unwrap_or_default(),
            detected_font_size_px: seq.next_element()?.unwrap_or_default(),
            detector: seq.next_element()?.unwrap_or_default(),
            text: seq.next_element()?.unwrap_or_default(),
            translation: seq.next_element()?.unwrap_or_default(),
            style: seq.next_element()?.unwrap_or_default(),
            font_prediction: seq.next_element()?.unwrap_or_default(),
            sprite: seq.next_element()?.unwrap_or_default(),
            sprite_transform: seq.next_element()?.unwrap_or_default(),
            lock_layout_box: seq.next_element()?.unwrap_or_default(),
            workflow: seq.next_element()?.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextWorkflow {
    #[serde(default = "default_text_workflow_modes")]
    pub modes: Vec<TextWorkflowMode>,
    #[serde(default)]
    pub result_mode: TextResultMode,
    #[serde(default)]
    pub lettering_status: WorkflowStatus,
    #[serde(default)]
    pub repair_status: WorkflowStatus,
    #[serde(default)]
    pub repair_layer: Option<NodeId>,
    #[serde(default)]
    pub font_trace: Option<FontWorkflowTrace>,
    #[serde(default)]
    pub repair_trace: Option<RepairWorkflowTrace>,
    #[serde(default)]
    pub selection: Option<TextSelection>,
}

impl Default for TextWorkflow {
    fn default() -> Self {
        Self {
            modes: default_text_workflow_modes(),
            result_mode: TextResultMode::default(),
            lettering_status: WorkflowStatus::default(),
            repair_status: WorkflowStatus::default(),
            repair_layer: None,
            font_trace: None,
            repair_trace: None,
            selection: None,
        }
    }
}

fn default_text_workflow_modes() -> Vec<TextWorkflowMode> {
    vec![TextWorkflowMode::Lettering]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextWorkflowMode {
    Lettering,
    Repair,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TextResultMode {
    #[default]
    Lettering,
    Repair,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontWorkflowTrace {
    #[serde(default)]
    pub primary_category: Option<String>,
    #[serde(default)]
    pub secondary_category: Option<String>,
    #[serde(default)]
    pub candidate_fonts: Vec<String>,
    #[serde(default)]
    pub selected_font: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profile_version: Option<u32>,
    #[serde(default)]
    pub profile_status: Option<FontProfileStatus>,
    #[serde(default)]
    pub yuzumarker_candidates: Vec<NamedFontPrediction>,
    #[serde(default)]
    pub text_role: Option<FontStyleRole>,
    #[serde(default)]
    pub style_group_id: Option<String>,
    #[serde(default)]
    pub source_primary_category: Option<String>,
    #[serde(default)]
    pub source_secondary_category: Option<String>,
    #[serde(default)]
    pub source_weight: Option<String>,
    #[serde(default)]
    pub source_emphasis: Option<String>,
    #[serde(default)]
    pub preserve_source_style: Option<bool>,
    #[serde(default)]
    pub recommended_font_bucket: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub auto_applied: Option<bool>,
    #[serde(default)]
    pub needs_review: Option<bool>,
    #[serde(default)]
    pub review_priority: FontReviewPriority,
    #[serde(default)]
    pub risk_reasons: Vec<String>,
    #[serde(default)]
    pub manual_override: bool,
    #[serde(default)]
    pub previous_font_families: Vec<String>,
    #[serde(default)]
    pub previous_font_size: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairWorkflowTrace {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub source_mask: Option<BlobRef>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextSelection {
    #[serde(default)]
    pub shapes: Vec<TextSelectionShape>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TextSelectionShape {
    Rectangle { transform: Transform },
    Brush { mask: BlobRef },
    Polygon { points: Vec<[f32; 2]> },
}

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation_deg: f32,
}

// ---------------------------------------------------------------------------
// Scene convenience helpers
// ---------------------------------------------------------------------------

impl Scene {
    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.get(&id)
    }

    pub fn page_mut(&mut self, id: PageId) -> Option<&mut Page> {
        self.pages.get_mut(&id)
    }

    pub fn node(&self, page: PageId, node: NodeId) -> Option<&Node> {
        self.page(page)?.nodes.get(&node)
    }

    pub fn node_mut(&mut self, page: PageId, node: NodeId) -> Option<&mut Node> {
        self.page_mut(page)?.nodes.get_mut(&node)
    }
}

impl Page {
    pub fn source_node(&self) -> Option<(&NodeId, &Node)> {
        self.nodes.iter().find(|(_, node)| {
            matches!(
                &node.kind,
                NodeKind::Image(img) if img.role == ImageRole::Source
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_datetime_postcard_round_trips() {
        let now: DateTime<Utc> = Utc::now();
        let bytes = postcard::to_allocvec(&now).expect("serialize");
        let decoded: DateTime<Utc> = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.timestamp(), now.timestamp());
    }

    #[test]
    fn project_style_postcard_round_trips() {
        let style = ProjectStyle::default();
        let bytes = postcard::to_allocvec(&style).expect("serialize");
        let _: ProjectStyle = postcard::from_bytes(&bytes).expect("deserialize");
    }

    #[test]
    fn project_style_deserializes_old_postcard_shape() {
        #[derive(Serialize)]
        struct OldProjectStyle {
            default_font: Option<String>,
        }

        let bytes = postcard::to_allocvec(&OldProjectStyle {
            default_font: Some("LegacyFont".to_string()),
        })
        .expect("serialize old style");

        let decoded: ProjectStyle = postcard::from_bytes(&bytes).expect("deserialize old style");

        assert_eq!(decoded.default_font.as_deref(), Some("LegacyFont"));
        assert!(decoded.font_profile.is_none());
        assert!(decoded.font_policy.buckets.is_empty());
        assert!(decoded.font_review_queue.is_empty());
    }

    #[test]
    fn font_profile_and_trace_json_round_trip() {
        let block_id = NodeId::new();
        let mut policy = FontPolicy::default();
        policy.buckets.insert(
            "body".to_string(),
            FontBucket {
                fonts: vec!["SourceHanSansSC-Bold".to_string()],
            },
        );
        let style = ProjectStyle {
            default_font: Some("SourceHanSansSC-Bold".to_string()),
            font_profile: Some(FontStyleProfile {
                id: "font_profile_main".to_string(),
                version: 2,
                status: FontProfileStatus::AutoActive,
                review_state: FontReviewState::Unreviewed,
                source: "mimo_calibrated".to_string(),
                profile_confidence: 0.82,
                profile_risks: vec![FontProfileRisk {
                    kind: "mixed_body_baseline".to_string(),
                    severity: "medium".to_string(),
                    message: "body mixes categories".to_string(),
                }],
                style_groups: vec![FontStyleGroup {
                    id: "body_bubble_primary".to_string(),
                    label: "气泡正文".to_string(),
                    role: FontStyleRole::BubbleBody,
                    description: "normal dialogue".to_string(),
                    source_categories: vec!["mincho".to_string(), "gothic".to_string()],
                    preserve_source_style: false,
                    target_bucket: "body".to_string(),
                    representative_blocks: vec![block_id],
                    confidence: 0.9,
                    needs_review: false,
                    risk_reasons: Vec::new(),
                    distinguishing_features: vec!["inside speech bubbles".to_string()],
                    possible_confusions: Vec::new(),
                }],
                previous_versions: vec!["v1".to_string()],
                change_log: vec![FontProfileChange {
                    version: 2,
                    change: "Changed target bucket".to_string(),
                    affected_blocks: vec![block_id],
                }],
            }),
            font_policy: policy,
            font_review_queue: vec![FontReviewQueueItem {
                id: "font-review-1".to_string(),
                block_id,
                profile_id: "font_profile_main".to_string(),
                profile_version: 2,
                style_group_id: "body_bubble_primary".to_string(),
                review_priority: FontReviewPriority::Medium,
                risk_reasons: vec!["style_group_unreviewed".to_string()],
                suggested_action: "review_style_group".to_string(),
                status: FontReviewItemStatus::Open,
            }],
        };
        let trace = FontWorkflowTrace {
            provider: Some("mimo_yuzumarker_guided".to_string()),
            profile_id: Some("font_profile_main".to_string()),
            profile_version: Some(2),
            profile_status: Some(FontProfileStatus::AutoActive),
            style_group_id: Some("body_bubble_primary".to_string()),
            text_role: Some(FontStyleRole::BubbleBody),
            recommended_font_bucket: Some("body".to_string()),
            selected_font: Some("SourceHanSansSC-Bold".to_string()),
            confidence: Some(0.88),
            auto_applied: Some(true),
            needs_review: Some(false),
            review_priority: FontReviewPriority::None,
            manual_override: false,
            ..Default::default()
        };

        let encoded = serde_json::to_string(&serde_json::json!({
            "style": style,
            "trace": trace,
        }))
        .expect("serialize profile payload");
        let decoded: serde_json::Value = serde_json::from_str(&encoded).expect("json");

        assert_eq!(
            decoded["style"]["fontProfile"]["styleGroups"][0]["targetBucket"],
            "body"
        );
        assert_eq!(
            decoded["style"]["fontReviewQueue"][0]["reviewPriority"],
            "medium"
        );
        assert_eq!(decoded["trace"]["profileVersion"], 2);
        assert_eq!(decoded["trace"]["provider"], "mimo_yuzumarker_guided");
    }

    #[test]
    fn project_meta_postcard_round_trips() {
        let meta = ProjectMeta::default();
        let bytes = postcard::to_allocvec(&meta).expect("serialize");
        let decoded: ProjectMeta = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.name, meta.name);
    }

    #[test]
    fn empty_scene_postcard_round_trips() {
        let scene = Scene::default();
        let bytes = postcard::to_allocvec(&scene).expect("serialize");
        let decoded: Scene = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.pages.len(), 0);
    }

    #[test]
    fn scene_with_one_page_postcard_round_trips() {
        let mut scene = Scene::default();
        scene.project.name = "hello".into();
        let page = Page::new("p1", 800, 600);
        let page_id = page.id;
        scene.pages.insert(page_id, page);
        let bytes = postcard::to_allocvec(&scene).expect("serialize");
        let decoded: Scene = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.pages.len(), 1);
        assert_eq!(decoded.project.name, "hello");
        assert!(decoded.pages.contains_key(&page_id));
    }

    #[test]
    fn text_workflow_defaults_to_lettering_mode() {
        let text = TextData::default();
        assert_eq!(text.workflow.modes, vec![TextWorkflowMode::Lettering]);
        assert_eq!(text.workflow.result_mode, TextResultMode::Lettering);
        assert_eq!(text.workflow.lettering_status, WorkflowStatus::Pending);
        assert_eq!(text.workflow.repair_status, WorkflowStatus::Pending);
    }

    #[test]
    fn text_workflow_postcard_round_trips() {
        let mut scene = Scene::default();
        let mut page = Page::new("p1", 800, 600);
        let node = Node {
            id: NodeId::new(),
            transform: Transform {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 50.0,
                rotation_deg: 12.0,
            },
            visible: true,
            kind: NodeKind::Text(TextData {
                workflow: TextWorkflow {
                    modes: vec![TextWorkflowMode::Lettering, TextWorkflowMode::Repair],
                    result_mode: TextResultMode::Repair,
                    repair_status: WorkflowStatus::Succeeded,
                    ..Default::default()
                },
                ..Default::default()
            }),
        };
        let node_id = node.id;
        page.nodes.insert(node_id, node);
        let page_id = page.id;
        scene.pages.insert(page_id, page);

        let bytes = postcard::to_allocvec(&scene).expect("serialize");
        let decoded: Scene = postcard::from_bytes(&bytes).expect("deserialize");
        let text = match &decoded.pages[&page_id].nodes[&node_id].kind {
            NodeKind::Text(text) => text,
            _ => panic!("expected text node"),
        };
        assert_eq!(
            text.workflow.modes,
            vec![TextWorkflowMode::Lettering, TextWorkflowMode::Repair]
        );
        assert_eq!(text.workflow.result_mode, TextResultMode::Repair);
        assert_eq!(text.workflow.repair_status, WorkflowStatus::Succeeded);
    }
}
