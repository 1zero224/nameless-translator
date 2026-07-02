//! `scene.bin` compatibility helpers.
//!
//! The current snapshot stores `Snapshot { epoch, scene }` as postcard bytes.
//! Postcard's binary format is position-based, so adding a field to a nested
//! struct (for example `TextData.workflow`) is not automatically backward
//! compatible even if the field has `#[serde(default)]`. Keep legacy wire
//! structs here and normalize them into the current scene model on open/list.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use koharu_core::{
    BlobRef, FontPrediction, ImageData, MaskData, Node, NodeId, NodeKind, Page, PageId,
    ProjectMeta, Scene, TextData, TextDirection, TextStyle, Transform,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub epoch: u64,
    pub scene: Scene,
}

pub fn encode(snapshot: &Snapshot) -> Result<Vec<u8>> {
    postcard::to_allocvec(snapshot).context("encode snapshot")
}

pub fn decode(bytes: &[u8]) -> Result<Snapshot> {
    postcard::from_bytes(bytes)
        .or_else(|_| postcard::from_bytes::<LegacySnapshotV1>(bytes).map(Into::into))
        .context("decode scene.bin")
}

#[derive(Serialize, Deserialize)]
pub struct LegacySnapshotV1 {
    pub epoch: u64,
    pub scene: LegacySceneV1,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacySceneV1 {
    pub project: ProjectMeta,
    pub pages: BTreeMap<PageId, LegacyPageV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyPageV1 {
    pub id: PageId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub nodes: BTreeMap<NodeId, LegacyNodeV1>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyNodeV1 {
    pub id: NodeId,
    #[serde(default)]
    pub transform: Transform,
    pub visible: bool,
    pub kind: LegacyNodeKindV1,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LegacyNodeKindV1 {
    Image(ImageData),
    Text(LegacyTextDataV1),
    Mask(MaskData),
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyTextDataV1 {
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
    #[serde(default)]
    pub sprite: Option<BlobRef>,
    #[serde(default)]
    pub sprite_transform: Option<Transform>,
    #[serde(default)]
    pub lock_layout_box: bool,
}

impl From<LegacySnapshotV1> for Snapshot {
    fn from(value: LegacySnapshotV1) -> Self {
        Self {
            epoch: value.epoch,
            scene: value.scene.into(),
        }
    }
}

impl From<LegacySceneV1> for Scene {
    fn from(value: LegacySceneV1) -> Self {
        let mut scene = Scene {
            project: value.project,
            pages: Default::default(),
        };
        for (id, page) in value.pages {
            scene.pages.insert(id, page.into());
        }
        scene
    }
}

impl From<LegacyPageV1> for Page {
    fn from(value: LegacyPageV1) -> Self {
        let mut page = Page {
            id: value.id,
            name: value.name,
            width: value.width,
            height: value.height,
            nodes: Default::default(),
        };
        for (id, node) in value.nodes {
            page.nodes.insert(id, node.into());
        }
        page
    }
}

impl From<LegacyNodeV1> for Node {
    fn from(value: LegacyNodeV1) -> Self {
        Self {
            id: value.id,
            transform: value.transform,
            visible: value.visible,
            kind: value.kind.into(),
        }
    }
}

impl From<LegacyNodeKindV1> for NodeKind {
    fn from(value: LegacyNodeKindV1) -> Self {
        match value {
            LegacyNodeKindV1::Image(data) => NodeKind::Image(data),
            LegacyNodeKindV1::Text(data) => NodeKind::Text(data.into()),
            LegacyNodeKindV1::Mask(data) => NodeKind::Mask(data),
        }
    }
}

impl From<LegacyTextDataV1> for TextData {
    fn from(value: LegacyTextDataV1) -> Self {
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
            workflow: Default::default(),
        }
    }
}
