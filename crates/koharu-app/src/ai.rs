use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use dashmap::DashMap;
use image::{DynamicImage, ImageFormat};
use koharu_ai::codex::{CodexClient, CodexConfig};
use koharu_ai::{AiImageProvider, AiImageRequest};
use koharu_core::{
    BlobRef, ImageData, ImageDataPatch, ImageRole, Node, NodeDataPatch, NodeId, NodeKind,
    NodePatch, Op, PageId, Scene, TextData, Transform,
};
use koharu_runtime::{RuntimeHttpClient, RuntimeManager};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::Instrument as _;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::blobs::BlobStore;
use crate::pipeline::engines::support::{
    build_bound_repair_layer_ops, openai_edit_mask_for_text, repair_layer_image_from_edit_output,
};
use crate::session::ProjectSession;

const DEFAULT_CODEX_IMAGE_MODEL: &str = "gpt-5.5";
const DEFAULT_CODEX_IMAGE_INSTRUCTIONS: &str = "Generate or edit the requested image.";
const DEFAULT_CODEX_IMAGE_QUALITY: &str = "high";

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CodexAuthAttemptStatus {
    Pending,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexDeviceLogin {
    pub login_id: String,
    pub verification_url: String,
    pub user_code: String,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexDeviceLoginStatus {
    pub login_id: String,
    pub status: CodexAuthAttemptStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthStatus {
    pub signed_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<CodexDeviceLoginStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodexImageGenerationOptions {
    pub page_id: PageId,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_node_id: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

#[derive(Debug, Clone)]
struct LoginAttempt {
    status: CodexAuthAttemptStatus,
    account_id: Option<String>,
    error: Option<String>,
}

pub struct AiManager {
    codex: CodexClient,
    http_client: RuntimeHttpClient,
    codex_device_timeout: Duration,
    codex_logins: Arc<DashMap<String, LoginAttempt>>,
    latest_codex_login: RwLock<Option<String>>,
}

impl AiManager {
    pub fn new(runtime: &RuntimeManager) -> Self {
        let config = CodexConfig::default();
        let codex_device_timeout = config.device_auth_timeout;
        Self {
            codex: CodexClient::with_http_client(config, runtime.http_client()),
            http_client: runtime.http_client(),
            codex_device_timeout,
            codex_logins: Arc::new(DashMap::new()),
            latest_codex_login: RwLock::new(None),
        }
    }

    pub fn codex_auth_status(&self) -> Result<CodexAuthStatus> {
        let tokens = self.codex.token_store().load()?;
        let account_id = tokens
            .as_ref()
            .and_then(|tokens| tokens.chatgpt_account_id());
        let login = self
            .latest_codex_login
            .read()
            .as_ref()
            .and_then(|id| {
                self.codex_logins
                    .get(id)
                    .map(|entry| (id.clone(), entry.clone()))
            })
            .map(|(login_id, attempt)| CodexDeviceLoginStatus {
                login_id,
                status: attempt.status,
                account_id: attempt.account_id,
                error: attempt.error,
            });

        Ok(CodexAuthStatus {
            signed_in: tokens.is_some(),
            account_id,
            login,
        })
    }

    pub async fn start_codex_device_login(self: &Arc<Self>) -> Result<CodexDeviceLogin> {
        let device_code = self.codex.request_device_code().await?;
        let login_id = Uuid::new_v4().to_string();
        self.codex_logins.insert(
            login_id.clone(),
            LoginAttempt {
                status: CodexAuthAttemptStatus::Pending,
                account_id: None,
                error: None,
            },
        );
        *self.latest_codex_login.write() = Some(login_id.clone());

        let manager = Arc::clone(self);
        let device_code_for_task = device_code.clone();
        let login_id_for_task = login_id.clone();
        tokio::spawn(async move {
            let result = manager
                .codex
                .complete_device_code_login(&device_code_for_task)
                .await;
            let attempt = match result {
                Ok(tokens) => LoginAttempt {
                    status: CodexAuthAttemptStatus::Succeeded,
                    account_id: tokens.chatgpt_account_id(),
                    error: None,
                },
                Err(err) => LoginAttempt {
                    status: CodexAuthAttemptStatus::Failed,
                    account_id: None,
                    error: Some(format!("{err:#}")),
                },
            };
            manager.codex_logins.insert(login_id_for_task, attempt);
        });

        let interval_seconds = device_code.interval().as_secs().max(1);
        Ok(CodexDeviceLogin {
            login_id,
            verification_url: device_code.verification_url,
            user_code: device_code.user_code,
            interval_seconds,
            timeout_seconds: self.codex_device_timeout.as_secs(),
        })
    }

    pub fn logout_codex(&self) -> Result<()> {
        self.codex.token_store().delete()?;
        Ok(())
    }

    pub async fn generate_codex_page_image(
        &self,
        session: Arc<ProjectSession>,
        options: CodexImageGenerationOptions,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        let workflow_span = tracing::info_span!(
            "codex_image_generation_workflow",
            page_id = %options.page_id
        );
        async move {
            let prompt = options.prompt.trim().to_string();
            if prompt.is_empty() {
                bail!("prompt is required");
            }

            let model = options
                .model
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_CODEX_IMAGE_MODEL.to_string());

            let source = tracing::info_span!("codex_source_image_load").in_scope(|| {
                let scene = session.scene_snapshot();
                let (_, image_data) = source_image(&scene, options.page_id)?;
                session.blobs.load_image(&image_data.blob)
            })?;
            let (source_width, source_height) = image_dimensions(&source);

            let source_data_url = tracing::info_span!("codex_source_image_encode")
                .in_scope(|| image_data_url(&source))?;
            tracing::info!(bytes = source_data_url.len(), "encoded Codex source image");

            check_cancelled(&cancel)?;
            let mut request = AiImageRequest::new(model.clone(), prompt.clone())
                .with_input_image(source_data_url);
            request.instructions = options
                .instructions
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_CODEX_IMAGE_INSTRUCTIONS.to_string());
            request.quality = options
                .quality
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_CODEX_IMAGE_QUALITY.to_string());
            request.size = options
                .size
                .filter(|value| !value.trim().is_empty())
                .or_else(|| Some("auto".to_string()));
            request.action = Some("edit".to_string());

            let result = self
                .codex
                .generate_image(request)
                .instrument(tracing::info_span!("codex_image_request"))
                .await?;
            tracing::info!("Codex image request completed");

            check_cancelled(&cancel)?;
            let generated_bytes = self
                .load_generated_image_bytes(&result.image_url)
                .instrument(tracing::info_span!("codex_generated_image_load"))
                .await?;
            tracing::info!(
                bytes = generated_bytes.len(),
                "loaded Codex generated image bytes"
            );

            let generated = tracing::info_span!("codex_generated_image_decode").in_scope(|| {
                image::load_from_memory(&generated_bytes)
                    .with_context(|| "failed to decode Codex image result")
            })?;
            let (width, height) = image_dimensions(&generated);
            tracing::info!(width, height, "decoded and stored Codex generated image");

            check_cancelled(&cancel)?;
            let scene = session.scene_snapshot();
            let ops = if let Some(text_node_id) = options.text_node_id {
                tracing::info!(%text_node_id, "storing Codex image as bound repair layer");
                codex_repair_layer_ops(
                    &scene,
                    options.page_id,
                    text_node_id,
                    source_width,
                    source_height,
                    &generated,
                    &session.blobs,
                    &model,
                    &prompt,
                )?
            } else {
                let blob = session.blobs.put_webp(&generated)?;
                vec![upsert_image_blob(
                    &scene,
                    options.page_id,
                    ImageRole::Rendered,
                    blob,
                    width,
                    height,
                )?]
            };
            session.apply(Op::Batch {
                ops,
                label: format!("codex-image: page {}", options.page_id),
            })?;
            tracing::info!("finished Codex image generation workflow");
            Ok(())
        }
        .instrument(workflow_span)
        .await
    }

    async fn load_generated_image_bytes(&self, url: &str) -> Result<Vec<u8>> {
        if let Some(bytes) = decode_data_image_url(url)? {
            return Ok(bytes);
        }

        if !(url.starts_with("http://") || url.starts_with("https://")) {
            bail!("unsupported Codex image result URL: {url}");
        }

        let response = self.http_client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("failed to fetch Codex image result ({status}): {body}");
        }
        Ok(response.bytes().await?.to_vec())
    }
}

fn check_cancelled(cancel: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        bail!("cancelled");
    }
    Ok(())
}

fn source_image(scene: &Scene, page_id: PageId) -> Result<(NodeId, &ImageData)> {
    let page = scene
        .page(page_id)
        .with_context(|| format!("page {} not found", page_id))?;
    page.nodes
        .iter()
        .find_map(|(id, node)| match &node.kind {
            NodeKind::Image(image) if image.role == ImageRole::Source => Some((*id, image)),
            _ => None,
        })
        .ok_or_else(|| anyhow!("page has no Source image node"))
}

fn image_data_url(image: &DynamicImage) -> Result<String> {
    let bytes = image_png_bytes(image)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/png;base64,{encoded}"))
}

fn image_png_bytes(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    image.write_to(&mut buf, ImageFormat::Png)?;
    Ok(buf.into_inner())
}

fn decode_data_image_url(url: &str) -> Result<Option<Vec<u8>>> {
    let Some(rest) = url.strip_prefix("data:image/") else {
        return Ok(None);
    };
    let Some((_, data)) = rest.split_once(',') else {
        bail!("invalid data image URL");
    };
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .context("failed to decode data image URL")?;
    Ok(Some(decoded))
}

fn image_dimensions(image: &DynamicImage) -> (u32, u32) {
    use image::GenericImageView as _;
    image.dimensions()
}

#[allow(clippy::too_many_arguments)]
fn codex_repair_layer_ops(
    scene: &Scene,
    page: PageId,
    text_id: NodeId,
    source_width: u32,
    source_height: u32,
    generated: &DynamicImage,
    blobs: &BlobStore,
    model: &str,
    prompt: &str,
) -> Result<Vec<Op>> {
    let (transform, text) = selected_text_node(scene, page, text_id)?;
    let mask = openai_edit_mask_for_text(source_width, source_height, transform, text, Some(blobs));
    let mask_image = DynamicImage::ImageRgba8(mask);
    let mask_png = image_png_bytes(&mask_image)?;
    let mask_blob = blobs.put_bytes(&mask_png)?;
    let layer = repair_layer_image_from_edit_output(generated, &mask_image)?;
    let natural_width = layer.width();
    let natural_height = layer.height();
    let layer_blob = blobs.put_raw(&DynamicImage::ImageRgba8(layer))?;
    build_bound_repair_layer_ops(
        scene,
        page,
        text_id,
        layer_blob,
        mask_blob,
        natural_width,
        natural_height,
        model,
        prompt,
    )
}

fn selected_text_node(
    scene: &Scene,
    page: PageId,
    text_id: NodeId,
) -> Result<(&Transform, &TextData)> {
    let page_ref = scene
        .page(page)
        .with_context(|| format!("page {} not found", page))?;
    let node = page_ref
        .nodes
        .get(&text_id)
        .with_context(|| format!("node {} not found", text_id))?;
    let NodeKind::Text(text) = &node.kind else {
        bail!("node {} is not text", text_id);
    };
    Ok((&node.transform, text))
}

fn upsert_image_blob(
    scene: &Scene,
    page: PageId,
    role: ImageRole,
    blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
) -> Result<Op> {
    let page_ref = scene
        .page(page)
        .with_context(|| format!("page {} not found", page))?;

    if let Some((node_id, _)) = page_ref
        .nodes
        .iter()
        .find_map(|(id, node)| match &node.kind {
            NodeKind::Image(image) if image.role == role => Some((*id, image)),
            _ => None,
        })
    {
        return Ok(Op::UpdateNode {
            page,
            id: node_id,
            patch: NodePatch {
                data: Some(NodeDataPatch::Image(ImageDataPatch {
                    blob: Some(blob),
                    opacity: None,
                    name: None,
                    natural_width: Some(natural_width),
                    natural_height: Some(natural_height),
                })),
                transform: None,
                visible: None,
            },
            prev: NodePatch::default(),
        });
    }

    let at = if role == ImageRole::Inpainted {
        1.min(page_ref.nodes.len())
    } else {
        page_ref.nodes.len()
    };
    Ok(Op::AddNode {
        page,
        node: Node {
            id: NodeId::new(),
            transform: Transform::default(),
            visible: role != ImageRole::Rendered,
            kind: NodeKind::Image(ImageData {
                role,
                blob,
                opacity: 1.0,
                natural_width,
                natural_height,
                name: None,
            }),
        },
        at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use koharu_core::{TextWorkflow, TextWorkflowMode, WorkflowStatus};

    #[test]
    fn decodes_data_image_url() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"png");
        let decoded = decode_data_image_url(&format!("data:image/png;base64,{encoded}")).unwrap();
        assert_eq!(decoded, Some(b"png".to_vec()));
    }

    #[test]
    fn ignores_non_data_url() {
        assert!(
            decode_data_image_url("https://example.test/image.png")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn codex_repair_layer_ops_adds_bound_custom_layer_for_selected_text() {
        let dir = tempfile::tempdir().expect("tempdir");
        let blobs = BlobStore::open(dir.path()).expect("blob store");
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 4, 4);
        page.id = page_id;
        page.nodes.insert(
            text_id,
            Node {
                id: text_id,
                transform: Transform {
                    x: 1.0,
                    y: 1.0,
                    width: 2.0,
                    height: 2.0,
                    rotation_deg: 0.0,
                },
                visible: true,
                kind: NodeKind::Text(TextData {
                    text: Some("source".to_string()),
                    translation: Some("translation".to_string()),
                    workflow: TextWorkflow {
                        modes: vec![TextWorkflowMode::Repair],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            },
        );
        scene.pages.insert(page_id, page);

        let generated = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
        let ops = codex_repair_layer_ops(
            &scene,
            page_id,
            text_id,
            4,
            4,
            &DynamicImage::ImageRgba8(generated),
            &blobs,
            "gpt-image-2",
            "replace text",
        )
        .expect("repair ops");

        assert_eq!(ops.len(), 2);
        for mut op in ops {
            op.apply(&mut scene).expect("apply repair op");
        }

        let page = scene.page(page_id).expect("page");
        let text = match &page.nodes.get(&text_id).expect("text").kind {
            NodeKind::Text(text) => text,
            other => panic!("expected text node, got {other:?}"),
        };
        let layer_id = text.workflow.repair_layer.expect("repair layer");
        assert_eq!(text.workflow.repair_status, WorkflowStatus::Succeeded);
        let trace = text.workflow.repair_trace.as_ref().expect("repair trace");
        assert_eq!(trace.model.as_deref(), Some("gpt-image-2"));
        assert_eq!(trace.prompt.as_deref(), Some("replace text"));
        assert!(trace.source_mask.is_some());

        let layer_node = page.nodes.get(&layer_id).expect("layer node");
        let NodeKind::Image(image) = &layer_node.kind else {
            panic!("expected image repair layer");
        };
        assert_eq!(image.role, ImageRole::Custom);
        assert_eq!(image.natural_width, 4);
        assert_eq!(image.natural_height, 4);

        let layer = blobs
            .load_image(&image.blob)
            .expect("layer image")
            .to_rgba8();
        assert_eq!(layer.get_pixel(1, 1).0, [10, 20, 30, 255]);
        assert_eq!(layer.get_pixel(0, 0).0, [10, 20, 30, 0]);
    }
}
