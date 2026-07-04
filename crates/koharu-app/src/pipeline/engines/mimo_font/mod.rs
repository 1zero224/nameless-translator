use std::sync::atomic::Ordering;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use koharu_core::{
    NodeDataPatch, NodePatch, Op, ProjectMetaPatch, TextData, TextDataPatch, TextWorkflow,
};
use koharu_ml::font_detector::FontDetector;

use crate::pipeline::artifacts::Artifact;
use crate::pipeline::engine::{Engine, EngineCtx, EngineInfo};
use crate::pipeline::engines::support::{lettering_text_nodes, load_source_image};

mod client;
mod parsing;
mod profile_policy;
mod selection;
mod taxonomy;
mod trace;
mod visuals;

use client::{MimoFontClient, MimoFontConfig};
use profile_policy::{
    FONT_EVIDENCE_TOP_K, FontEvidence, active_profile, apply_font_profile, build_auto_profile,
    build_mimo_profile, default_font_policy, mark_profile_generation_fallback, merge_review_queue,
};
use trace::{ml_prediction_to_core, normalize_font_prediction};
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
        let mut predictions = self.detector.inference(&crops, FONT_EVIDENCE_TOP_K)?;
        predictions.iter_mut().for_each(normalize_font_prediction);
        let predictions = predictions
            .into_iter()
            .map(ml_prediction_to_core)
            .collect::<Vec<_>>();

        let fonts = ctx.renderer.available_fonts().unwrap_or_default();
        let evidence = texts
            .iter()
            .zip(predictions.iter())
            .map(|((node_id, _, _), prediction)| FontEvidence {
                block_id: *node_id,
                prediction: prediction.clone(),
            })
            .collect::<Vec<_>>();
        let mut project_style = ctx.scene.project.style.clone();
        if project_style.font_policy.buckets.is_empty() {
            project_style.font_policy = default_font_policy(
                &fonts,
                ctx.scene.project.style.default_font.as_deref(),
                ctx.options.default_font.as_deref(),
            );
        }
        if active_profile(&project_style).is_none() {
            project_style.font_profile = Some(self.build_profile(&source, &evidence).await);
        }
        let profile = active_profile(&project_style)
            .cloned()
            .expect("profile exists after bootstrap");

        let mut text_ops = Vec::with_capacity(texts.len());
        let mut review_additions = Vec::new();
        for ((node_id, _, text), prediction) in texts.iter().zip(predictions) {
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

            let application = apply_font_profile(
                *node_id,
                text,
                &prediction,
                &profile,
                &project_style.font_policy,
                &fonts,
            );
            if let Some(item) = application.review_item {
                review_additions.push(item);
            }
            text_ops.push(Op::UpdateNode {
                page: ctx.page,
                id: *node_id,
                patch: NodePatch {
                    data: Some(NodeDataPatch::Text(TextDataPatch {
                        font_prediction: Some(Some(prediction)),
                        style: application.style.map(Some),
                        workflow: Some(workflow_with_trace(text, application.trace)),
                        ..Default::default()
                    })),
                    transform: None,
                    visible: None,
                },
                prev: NodePatch::default(),
            });
        }

        project_style.font_review_queue =
            merge_review_queue(&project_style.font_review_queue, review_additions);
        let mut ops = Vec::with_capacity(text_ops.len() + 1);
        ops.push(Op::UpdateProjectMeta {
            patch: ProjectMetaPatch {
                style: Some(project_style),
                ..Default::default()
            },
            prev: ProjectMetaPatch::default(),
        });
        ops.extend(text_ops);
        Ok(ops)
    }
}

impl Model {
    async fn build_profile(
        &self,
        source: &image::DynamicImage,
        evidence: &[FontEvidence],
    ) -> koharu_core::FontStyleProfile {
        let Some(client) = self.client.as_ref() else {
            return build_auto_profile(evidence, "yuzumarker_bootstrap");
        };
        match build_mimo_profile(client, source, evidence).await {
            Ok(profile) => profile,
            Err(error) => {
                let mut profile = build_auto_profile(evidence, "yuzumarker_bootstrap");
                mark_profile_generation_fallback(
                    &mut profile,
                    format!("MIMO profile generation failed: {error:#}"),
                );
                profile
            }
        }
    }
}

fn workflow_with_trace(text: &TextData, trace: koharu_core::FontWorkflowTrace) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    workflow.font_trace = Some(trace);
    workflow
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
