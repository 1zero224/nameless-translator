use std::sync::atomic::Ordering;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use image::DynamicImage;
use koharu_core::{
    FontFaceInfo, NodeDataPatch, NodeId, NodePatch, Op, PageId, TextData, TextDataPatch, Transform,
};
use koharu_ml::font_detector::FontDetector;

use crate::pipeline::artifacts::Artifact;
use crate::pipeline::engine::{Engine, EngineCtx, EngineInfo};
use crate::pipeline::engines::support::{lettering_text_nodes, load_source_image};
use crate::renderer::Renderer;

mod client;
mod parsing;
mod selection;
mod taxonomy;
mod trace;
mod visuals;

use client::{MimoFontClient, MimoFontConfig};
use selection::choose_font_with_mimo;
use trace::{
    apply_outcome_to_prediction, fallback_outcome, ml_prediction_to_core,
    normalize_font_prediction, style_with_font, workflow_with_mimo_trace,
};
use visuals::crop_text;

const MAX_CANDIDATES: usize = 8;

struct Model {
    detector: FontDetector,
    client: Option<MimoFontClient>,
}

#[async_trait]
impl Engine for Model {
    async fn run(&self, ctx: EngineCtx<'_>) -> Result<Vec<Op>> {
        let texts = lettering_text_nodes(ctx.scene, ctx.page);
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let source = load_source_image(ctx.scene, ctx.page, ctx.blobs)?;
        let crops = texts
            .iter()
            .map(|(_, transform, _)| crop_text(&source, transform))
            .collect::<Vec<_>>();
        let mut predictions = self.detector.inference(&crops, 1)?;
        predictions.iter_mut().for_each(normalize_font_prediction);

        let fonts = ctx.renderer.available_fonts().unwrap_or_default();
        let mut ops = Vec::with_capacity(texts.len());
        for (((node_id, transform, text), crop), prediction) in
            texts.iter().zip(crops.iter()).zip(predictions)
        {
            if ctx.cancel.load(Ordering::Relaxed) {
                return Err(anyhow!("pipeline cancelled"));
            }
            if ctx
                .options
                .text_node_ids
                .as_ref()
                .is_some_and(|ids| !ids.contains(node_id))
            {
                continue;
            }
            ops.push(
                self.op_for_text(
                    ctx.page,
                    *node_id,
                    ctx.renderer,
                    crop,
                    transform,
                    text,
                    prediction,
                    &fonts,
                )
                .await?,
            );
        }
        Ok(ops)
    }
}

impl Model {
    async fn op_for_text(
        &self,
        page: PageId,
        node_id: NodeId,
        renderer: &Renderer,
        crop: &DynamicImage,
        transform: &Transform,
        text: &TextData,
        prediction: koharu_ml::types::FontPrediction,
        fonts: &[FontFaceInfo],
    ) -> Result<Op> {
        let mut prediction = ml_prediction_to_core(prediction);
        let outcome = if let Some(client) = &self.client {
            choose_font_with_mimo(client, renderer, crop, transform, text, &prediction, fonts)
                .await
                .unwrap_or_else(|error| fallback_outcome(&prediction, error))
        } else {
            fallback_outcome(
                &prediction,
                anyhow!("MIMO configuration is missing; using YuzuMarker fallback"),
            )
        };

        apply_outcome_to_prediction(&mut prediction, &outcome);
        let style = outcome
            .selected
            .as_ref()
            .map(|candidate| style_with_font(text, candidate));
        Ok(Op::UpdateNode {
            page,
            id: node_id,
            patch: NodePatch {
                data: Some(NodeDataPatch::Text(TextDataPatch {
                    font_prediction: Some(Some(prediction)),
                    style: style.map(Some),
                    workflow: Some(workflow_with_mimo_trace(text, &outcome)),
                    ..Default::default()
                })),
                transform: None,
                visible: None,
            },
            prev: NodePatch::default(),
        })
    }
}

inventory::submit! {
    EngineInfo {
        id: "mimo-font-selection",
        name: "MIMO Vision Font Selection",
        needs: &[Artifact::SourceImage, Artifact::TextBoxes],
        produces: &[Artifact::FontPredictions],
        load: |runtime, cpu| Box::pin(async move {
            let detector = FontDetector::load(runtime, cpu).await?;
            let config = MimoFontConfig::from_env_or_config().ok();
            Ok(Box::new(Model {
                detector,
                client: config.map(MimoFontClient::new).transpose()?,
            }) as Box<dyn Engine>)
        }),
    }
}

#[cfg(test)]
mod tests;
