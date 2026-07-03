use std::io::Cursor;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::Engine as _;
use image::{DynamicImage, GenericImageView, ImageFormat};
use koharu_core::{
    BlobRef, NodeId, Op, RepairWorkflowTrace, Scene, TextData, Transform, WorkflowStatus,
};
use serde::Deserialize;

use crate::pipeline::artifacts::Artifact;
use crate::pipeline::engine::{Engine, EngineCtx, EngineInfo};
use crate::pipeline::engines::support::{
    build_bound_repair_layer_ops, load_source_image, openai_edit_mask_for_transform,
    remove_bound_repair_layer_op, repair_layer_image_from_edit_output, repair_text_nodes,
    update_text_workflow_op,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-image-2";

struct Model {
    client: GptImageRepairClient,
}

#[async_trait]
impl Engine for Model {
    async fn run(&self, ctx: EngineCtx<'_>) -> Result<Vec<Op>> {
        let source = load_source_image(ctx.scene, ctx.page, ctx.blobs)?;
        let (source_width, source_height) = source.dimensions();
        let source_png = encode_png(&source)?;
        let mut ops = Vec::new();
        for (node_id, transform, text) in repair_text_nodes(ctx.scene, ctx.page) {
            if ctx.cancel.load(Ordering::Relaxed) {
                return Err(anyhow!("pipeline cancelled"));
            }
            if ctx
                .options
                .text_node_ids
                .as_ref()
                .is_some_and(|ids| !ids.contains(&node_id))
            {
                continue;
            }

            let prompt = match repair_prompt(text, transform) {
                Ok(prompt) => prompt,
                Err(error) => {
                    ops.extend(repair_failure_ops(
                        ctx.scene,
                        ctx.page,
                        node_id,
                        text,
                        &self.client.config.model,
                        "",
                        None,
                        &error.to_string(),
                    ));
                    continue;
                }
            };
            let mask = openai_edit_mask_for_transform(source_width, source_height, transform);
            let mask_image = DynamicImage::ImageRgba8(mask.clone());
            let mask_png = encode_png(&mask_image)?;
            let mask_blob = ctx.blobs.put_bytes(&mask_png)?;
            let result = run_one_repair(
                &self.client,
                &source_png,
                &mask_png,
                &mask_image,
                &prompt,
                ctx.blobs,
            )
            .await;
            match result {
                Ok((layer_blob, natural_width, natural_height)) => {
                    ops.extend(build_bound_repair_layer_ops(
                        ctx.scene,
                        ctx.page,
                        node_id,
                        layer_blob,
                        mask_blob,
                        natural_width,
                        natural_height,
                        &self.client.config.model,
                        &prompt,
                    )?);
                }
                Err(error) => ops.extend(repair_failure_ops(
                    ctx.scene,
                    ctx.page,
                    node_id,
                    text,
                    &self.client.config.model,
                    &prompt,
                    Some(mask_blob),
                    &error.to_string(),
                )),
            }
        }
        Ok(ops)
    }
}

async fn run_one_repair(
    client: &GptImageRepairClient,
    source_png: &[u8],
    mask_png: &[u8],
    mask_image: &DynamicImage,
    prompt: &str,
    blobs: &crate::blobs::BlobStore,
) -> Result<(BlobRef, u32, u32)> {
    let output_size = image_size_param(mask_image.width(), mask_image.height());
    let output_bytes = client
        .edit(source_png, mask_png, prompt, &output_size)
        .await?;
    let edited =
        image::load_from_memory(&output_bytes).context("decode OpenAI image edit output")?;
    let layer = repair_layer_image_from_edit_output(&edited, mask_image)?;
    let (natural_width, natural_height) = layer.dimensions();
    let blob = blobs.put_raw(&DynamicImage::ImageRgba8(layer))?;
    Ok((blob, natural_width, natural_height))
}

fn repair_failure_op(
    page: koharu_core::PageId,
    node_id: NodeId,
    text: &TextData,
    model: &str,
    prompt: &str,
    mask_blob: Option<BlobRef>,
    error: &str,
) -> Op {
    let mut workflow = text.workflow.clone();
    workflow.repair_status = WorkflowStatus::Failed;
    workflow.repair_layer = None;
    workflow.repair_trace = Some(RepairWorkflowTrace {
        model: Some(model.to_string()),
        prompt: Some(prompt.to_string()),
        source_mask: mask_blob,
        error: Some(error.to_string()),
    });
    update_text_workflow_op(page, node_id, workflow)
}

fn repair_failure_ops(
    scene: &Scene,
    page: koharu_core::PageId,
    node_id: NodeId,
    text: &TextData,
    model: &str,
    prompt: &str,
    mask_blob: Option<BlobRef>,
    error: &str,
) -> Vec<Op> {
    let mut ops = Vec::with_capacity(2);
    if let Some((remove_op, _)) = remove_bound_repair_layer_op(scene, page, text) {
        ops.push(remove_op);
    }
    ops.push(repair_failure_op(
        page, node_id, text, model, prompt, mask_blob, error,
    ));
    ops
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    image.write_to(&mut buf, ImageFormat::Png)?;
    Ok(buf.into_inner())
}

#[derive(Clone)]
struct GptImageRepairConfig {
    base_url: String,
    api_key: String,
    model: String,
}

impl GptImageRepairConfig {
    fn from_env_or_config() -> Result<Self> {
        let app_config = crate::config::load().ok();
        let openai_provider = app_config.as_ref().and_then(|config| {
            config
                .providers
                .iter()
                .find(|provider| provider.id == "openai")
        });
        let api_key = non_empty_env("GPT_IMAGE_API_KEY")
            .or_else(|| non_empty_env("OPENAI_API_KEY"))
            .or_else(|| {
                openai_provider
                    .and_then(|provider| provider.api_key.as_ref())
                    .map(|secret| secret.expose().to_string())
            })
            .filter(|value| !value.trim().is_empty())
            .context("GPT Image repair requires GPT_IMAGE_API_KEY, OPENAI_API_KEY, or an openai provider secret")?;
        let base_url = non_empty_env("GPT_IMAGE_BASE_URL")
            .or_else(|| openai_provider.and_then(|provider| provider.base_url.clone()))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = non_empty_env("GPT_IMAGE_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string());
        Ok(Self {
            base_url,
            api_key,
            model,
        })
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

struct GptImageRepairClient {
    http: reqwest::Client,
    config: GptImageRepairConfig,
}

impl GptImageRepairClient {
    async fn edit(
        &self,
        source_png: &[u8],
        mask_png: &[u8],
        prompt: &str,
        size: &str,
    ) -> Result<Vec<u8>> {
        let image_part = reqwest::multipart::Part::bytes(source_png.to_vec())
            .file_name("source.png")
            .mime_str("image/png")?;
        let mask_part = reqwest::multipart::Part::bytes(mask_png.to_vec())
            .file_name("mask.png")
            .mime_str("image/png")?;
        let form = reqwest::multipart::Form::new()
            .text("model", self.config.model.clone())
            .text("prompt", prompt.to_string())
            .text("n", "1")
            .text("size", size.to_string())
            .text("quality", "auto")
            .part("image", image_part)
            .part("mask", mask_part);
        let response = self
            .http
            .post(image_edits_endpoint(&self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .multipart(form)
            .send()
            .await
            .context("request OpenAI image edit")?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("read OpenAI image edit response")?;
        if !status.is_success() {
            let text = String::from_utf8_lossy(&body);
            anyhow::bail!("OpenAI image edit failed with {status}: {text}");
        }
        decode_image_edit_response(&body)
    }
}

fn image_edits_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/images") {
        format!("{base}/edits")
    } else {
        format!("{base}/images/edits")
    }
}

fn image_size_param(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

fn repair_prompt(text: &TextData, transform: &Transform) -> Result<String> {
    let original = text.text.as_deref().unwrap_or("").trim();
    let translation = text.translation.as_deref().unwrap_or("").trim();
    if translation.is_empty() {
        anyhow::bail!("translation is required for GPT image repair");
    }
    let font_size = text
        .detected_font_size_px
        .map(|v| format!("{v:.1}px"))
        .unwrap_or_else(|| "unknown".to_string());
    Ok(format!(
        "Replace only the masked original manga text with the translation. \
         Preserve the local artwork, screentones, speech bubble edges, original lettering style, \
         approximate font weight, font size, and rotation. Original Japanese text: {original}. \
         Translation to render: {translation}. Text box: x={:.1}, y={:.1}, width={:.1}, height={:.1}, \
         rotation={:.1} degrees, detected font size={font_size}. \
         Do not modify pixels outside the transparent mask.",
        transform.x, transform.y, transform.width, transform.height, transform.rotation_deg
    ))
}

#[derive(Deserialize)]
struct ImageEditResponse {
    data: Vec<ImageEditItem>,
}

#[derive(Deserialize)]
struct ImageEditItem {
    b64_json: Option<String>,
}

fn decode_image_edit_response(body: &[u8]) -> Result<Vec<u8>> {
    let response: ImageEditResponse =
        serde_json::from_slice(body).context("decode OpenAI image edit response")?;
    let b64 = response
        .data
        .first()
        .and_then(|item| item.b64_json.as_deref())
        .filter(|value| !value.trim().is_empty())
        .context("OpenAI image edit response did not include data[0].b64_json")?;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .context("decode OpenAI image edit b64_json")
}

inventory::submit! {
    EngineInfo {
        id: "gpt-image-2-repair",
        name: "GPT Image 2 Repair",
        needs: &[Artifact::SourceImage, Artifact::TextBoxes, Artifact::Translations],
        produces: &[Artifact::RepairLayers],
        load: |runtime, _cpu| Box::pin(async move {
            let _ = runtime;
            let config = GptImageRepairConfig::from_env_or_config()?;
            Ok(Box::new(Model {
                client: GptImageRepairClient {
                    http: reqwest::Client::builder().build()?,
                    config,
                },
            }) as Box<dyn Engine>)
        }),
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgba, RgbaImage};
    use koharu_core::{
        BlobRef, ImageData, ImageRole, Node, NodeDataPatch, NodeId, NodeKind, Op, PageId, Scene,
        TextData, TextWorkflow, Transform, WorkflowStatus,
    };

    #[test]
    fn repair_prompt_includes_original_translation_and_geometry() {
        let text = TextData {
            text: Some("こんにちは".to_string()),
            translation: Some("hello".to_string()),
            detected_font_size_px: Some(28.0),
            ..Default::default()
        };
        let prompt = super::repair_prompt(
            &text,
            &Transform {
                x: 10.0,
                y: 20.0,
                width: 80.0,
                height: 44.0,
                rotation_deg: 12.0,
            },
        )
        .expect("prompt");

        assert!(prompt.contains("こんにちは"));
        assert!(prompt.contains("hello"));
        assert!(prompt.contains("12"));
        assert!(prompt.contains("28"));
    }

    #[test]
    fn repair_prompt_rejects_empty_translation() {
        let text = TextData {
            text: Some("こんにちは".to_string()),
            translation: Some("  ".to_string()),
            ..Default::default()
        };

        let error =
            super::repair_prompt(&text, &Transform::default()).expect_err("missing translation");

        assert!(error.to_string().contains("translation"));
    }

    #[test]
    fn image_edits_endpoint_appends_path_without_double_slash() {
        assert_eq!(
            super::image_edits_endpoint("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/images/edits"
        );
    }

    #[test]
    fn image_edits_endpoint_accepts_base_url_that_already_points_to_images() {
        assert_eq!(
            super::image_edits_endpoint("https://api.openai.com/v1/images"),
            "https://api.openai.com/v1/images/edits"
        );
    }

    #[test]
    fn decode_image_edit_response_reads_first_b64_json() {
        let json = r#"{"created":1,"data":[{"b64_json":"aGVsbG8="}]}"#;
        let bytes = super::decode_image_edit_response(json.as_bytes()).expect("decode response");

        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn image_size_param_uses_source_dimensions() {
        assert_eq!(super::image_size_param(512, 768), "512x768");
    }

    #[test]
    fn registers_engine_requires_translations() {
        let info = crate::pipeline::Registry::find("gpt-image-2-repair").expect("engine info");

        assert_eq!(info.name, "GPT Image 2 Repair");
        assert!(info.needs.contains(&crate::pipeline::Artifact::SourceImage));
        assert!(info.needs.contains(&crate::pipeline::Artifact::TextBoxes));
        assert!(
            info.needs
                .contains(&crate::pipeline::Artifact::Translations)
        );
        assert!(
            info.produces
                .contains(&crate::pipeline::Artifact::RepairLayers)
        );
        assert!(
            !info
                .produces
                .contains(&crate::pipeline::Artifact::Inpainted)
        );
    }

    #[test]
    fn repair_failure_op_clears_stale_repair_layer_binding() {
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let old_layer_id = NodeId::new();
        let text = TextData {
            workflow: TextWorkflow {
                repair_layer: Some(old_layer_id),
                ..Default::default()
            },
            ..Default::default()
        };

        let op = super::repair_failure_op(
            page_id,
            text_id,
            &text,
            "gpt-image-2",
            "prompt",
            None,
            "missing translation",
        );

        match op {
            Op::UpdateNode { id, patch, .. } => {
                assert_eq!(id, text_id);
                let Some(NodeDataPatch::Text(text_patch)) = patch.data else {
                    panic!("expected text workflow patch");
                };
                let workflow = text_patch.workflow.expect("workflow patch");
                assert_eq!(workflow.repair_status, WorkflowStatus::Failed);
                assert_eq!(workflow.repair_layer, None);
            }
            other => panic!("expected UpdateNode, got {other:?}"),
        }
    }

    #[test]
    fn repair_failure_ops_removes_stale_bound_layer() {
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let old_layer_id = NodeId::new();
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 100, 100);
        page.id = page_id;
        page.nodes.insert(
            text_id,
            Node {
                id: text_id,
                transform: Transform::default(),
                visible: true,
                kind: NodeKind::Text(TextData {
                    workflow: TextWorkflow {
                        repair_layer: Some(old_layer_id),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            },
        );
        page.nodes.insert(
            old_layer_id,
            Node {
                id: old_layer_id,
                transform: Transform::default(),
                visible: true,
                kind: NodeKind::Image(ImageData {
                    role: ImageRole::Custom,
                    blob: BlobRef::new("old-layer"),
                    opacity: 1.0,
                    natural_width: 100,
                    natural_height: 100,
                    name: Some("old repair".into()),
                }),
            },
        );
        let text = match &page.nodes.get(&text_id).expect("text").kind {
            NodeKind::Text(text) => text.clone(),
            other => panic!("expected text node, got {other:?}"),
        };
        scene.pages.insert(page_id, page);

        let ops = super::repair_failure_ops(
            &scene,
            page_id,
            text_id,
            &text,
            "gpt-image-2",
            "",
            None,
            "missing translation",
        );

        assert_eq!(ops.len(), 2);
        match &ops[0] {
            Op::RemoveNode { id, .. } => assert_eq!(id, &old_layer_id),
            other => panic!("expected RemoveNode, got {other:?}"),
        }
        match &ops[1] {
            Op::UpdateNode { id, patch, .. } => {
                assert_eq!(id, &text_id);
                let Some(NodeDataPatch::Text(text_patch)) = &patch.data else {
                    panic!("expected text workflow patch");
                };
                assert_eq!(
                    text_patch.workflow.as_ref().expect("workflow").repair_layer,
                    None
                );
            }
            other => panic!("expected UpdateNode, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "requires GPT_IMAGE_API_KEY and spends real image API credits"]
    async fn real_gpt_image_repair_smoke_writes_artifacts() -> anyhow::Result<()> {
        let config = super::GptImageRepairConfig::from_env_or_config()?;
        let client = super::GptImageRepairClient {
            http: reqwest::Client::builder().build()?,
            config,
        };
        let mut source = RgbaImage::from_pixel(512, 512, Rgba([245, 245, 240, 255]));
        for y in 210..302 {
            for x in 170..342 {
                source.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let mut mask = RgbaImage::from_pixel(512, 512, Rgba([0, 0, 0, 255]));
        for y in 210..302 {
            for x in 170..342 {
                mask.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        let source_png = super::encode_png(&DynamicImage::ImageRgba8(source))?;
        let mask_image = DynamicImage::ImageRgba8(mask);
        let mask_png = super::encode_png(&mask_image)?;
        let prompt = "Replace only the transparent mask area with the text HELLO in clean black manga lettering. Preserve everything outside the transparent mask.";
        let out_dir = smoke_output_dir()?;
        std::fs::create_dir_all(&out_dir)?;
        std::fs::write(out_dir.join("source.png"), &source_png)?;
        std::fs::write(out_dir.join("mask.png"), &mask_png)?;
        std::fs::write(
            out_dir.join("request-summary.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "endpoint": super::image_edits_endpoint(&client.config.base_url),
                "model": client.config.model,
                "prompt": prompt,
                "size": super::image_size_param(512, 512),
                "image": {"width": 512, "height": 512, "format": "png"},
                "mask": {"width": 512, "height": 512, "format": "png", "transparentRegion": [170, 210, 342, 302]},
                "secretPolicy": "API key loaded from environment/config and intentionally not written"
            }))?,
        )?;

        let size = super::image_size_param(512, 512);
        let output = client.edit(&source_png, &mask_png, prompt, &size).await?;
        let decoded = image::load_from_memory(&output)?;
        let repair_layer = crate::pipeline::engines::support::repair_layer_image_from_edit_output(
            &decoded,
            &mask_image,
        )?;
        assert_eq!(repair_layer.dimensions(), (512, 512));
        std::fs::write(out_dir.join("output.png"), &output)?;
        std::fs::write(
            out_dir.join("repair-layer.png"),
            super::encode_png(&DynamicImage::ImageRgba8(repair_layer))?,
        )?;
        std::fs::write(
            out_dir.join("output-summary.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "width": decoded.width(),
                "height": decoded.height(),
                "bytes": output.len(),
                "repairLayerWidth": 512,
                "repairLayerHeight": 512
            }))?,
        )?;
        eprintln!("smoke artifacts: {}", out_dir.display());
        Ok(())
    }

    fn smoke_output_dir() -> anyhow::Result<std::path::PathBuf> {
        if let Ok(path) = std::env::var("KOHARU_GPT_IMAGE_REPAIR_SMOKE_DIR") {
            return Ok(std::path::PathBuf::from(path));
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        Ok(std::path::PathBuf::from(".tmp")
            .join("gpt-image-repair-smoke")
            .join(stamp.to_string()))
    }
}
