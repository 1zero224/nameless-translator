//! Shared helpers used by multiple engine implementations.
//!
//! The patterns here map `koharu-ml` / `koharu-llm` outputs (plain
//! `TextRegion`s, `DynamicImage`s) into `Op` sequences that mutate the scene.

use std::collections::HashMap;

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops::FilterType};
use koharu_core::{
    BlobRef, FontPrediction, FontWorkflowTrace, ImageData, ImageRole, MaskData, MaskRole,
    NamedFontPrediction, Node, NodeDataPatch, NodeId, NodeKind, Op, PageId, ReadingOrder, Region,
    RepairWorkflowTrace, Scene, TextData, TextDataPatch, TextSelection, TextSelectionShape,
    TextWorkflow, TextWorkflowMode, Transform, WorkflowStatus,
};

use crate::blobs::BlobStore;

// ---------------------------------------------------------------------------
// Read helpers
// ---------------------------------------------------------------------------

/// Find the Source image node on `page`. Returns `(node_id, image_data)`.
/// Every valid page has exactly one; absence means the page is malformed.
pub fn source_node(scene: &Scene, page: PageId) -> Result<(NodeId, &ImageData)> {
    let page = scene
        .page(page)
        .with_context(|| format!("page {} not found", page))?;
    for (id, node) in page.nodes.iter() {
        if let NodeKind::Image(img) = &node.kind
            && img.role == ImageRole::Source
        {
            return Ok((*id, img));
        }
    }
    anyhow::bail!("page has no Source image node")
}

/// Load the source image bytes + decoded image for `page`.
pub fn load_source_image(scene: &Scene, page: PageId, blobs: &BlobStore) -> Result<DynamicImage> {
    let (_, img_data) = source_node(scene, page)?;
    blobs.load_image(&img_data.blob)
}

/// Find a node of `Image { role }` on `page`, if any.
pub fn find_image_node(scene: &Scene, page: PageId, role: ImageRole) -> Option<(NodeId, BlobRef)> {
    let page = scene.page(page)?;
    page.nodes.iter().find_map(|(id, node)| match &node.kind {
        NodeKind::Image(img) if img.role == role => Some((*id, img.blob.clone())),
        _ => None,
    })
}

/// Find a node of `Mask { role }` on `page`, if any.
pub fn find_mask_node(scene: &Scene, page: PageId, role: MaskRole) -> Option<(NodeId, BlobRef)> {
    let page = scene.page(page)?;
    page.nodes.iter().find_map(|(id, node)| match &node.kind {
        NodeKind::Mask(mask) if mask.role == role => Some((*id, mask.blob.clone())),
        _ => None,
    })
}

/// Collect `(NodeId, &Transform, &TextData)` for every text node on `page`,
/// in stacking order.
pub fn text_nodes(scene: &Scene, page: PageId) -> Vec<(NodeId, &Transform, &TextData)> {
    let Some(page) = scene.page(page) else {
        return Vec::new();
    };
    page.nodes
        .iter()
        .filter_map(|(id, node)| match &node.kind {
            NodeKind::Text(t) => Some((*id, &node.transform, t)),
            _ => None,
        })
        .collect()
}

/// Text nodes that should participate in the lettering pipeline.
pub fn lettering_text_nodes(scene: &Scene, page: PageId) -> Vec<(NodeId, &Transform, &TextData)> {
    text_nodes(scene, page)
        .into_iter()
        .filter(|(_, _, text)| text.workflow.modes.contains(&TextWorkflowMode::Lettering))
        .collect()
}

/// Text regions that should guide lettering-mode cleanup/inpainting.
pub fn lettering_text_regions(scene: &Scene, page: PageId) -> Vec<koharu_ml::types::TextRegion> {
    lettering_text_nodes(scene, page)
        .into_iter()
        .map(|(_, transform, text)| text_node_to_region(transform, text))
        .collect()
}

/// Text nodes that should participate in repair workflows.
pub fn repair_text_nodes(scene: &Scene, page: PageId) -> Vec<(NodeId, &Transform, &TextData)> {
    text_nodes(scene, page)
        .into_iter()
        .filter(|(_, _, text)| text.workflow.modes.contains(&TextWorkflowMode::Repair))
        .collect()
}

/// Build a workflow-only text patch. Engine outputs stay in normal Op/history
/// flow instead of maintaining a side-channel status store.
pub fn update_text_workflow_op(page: PageId, node_id: NodeId, workflow: TextWorkflow) -> Op {
    Op::UpdateNode {
        page,
        id: node_id,
        patch: koharu_core::NodePatch {
            data: Some(NodeDataPatch::Text(TextDataPatch {
                workflow: Some(workflow),
                ..Default::default()
            })),
            transform: None,
            visible: None,
        },
        prev: koharu_core::NodePatch::default(),
    }
}

/// Record the font detector's observable output on the workflow trace.
pub fn workflow_with_font_trace(text: &TextData, prediction: &FontPrediction) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    let selected = prediction.named_fonts.first();
    workflow.font_trace = Some(FontWorkflowTrace {
        primary_category: selected.map(|font| {
            if font.serif {
                "serif".to_string()
            } else {
                "sans_serif".to_string()
            }
        }),
        secondary_category: selected.map(infer_font_secondary_category),
        candidate_fonts: prediction
            .named_fonts
            .iter()
            .take(8)
            .map(|font| font.name.clone())
            .collect(),
        selected_font: selected.map(|font| font.name.clone()),
        notes: selected
            .map(|font| vec![format!("yuzumarker probability {:.3}", font.probability)])
            .unwrap_or_default(),
        ..Default::default()
    });
    workflow
}

fn infer_font_secondary_category(font: &NamedFontPrediction) -> String {
    let name = font.name.to_lowercase();
    if font.serif {
        if contains_any(&name, &["kai", "kaiti", "klee", "楷", "行書"]) {
            "kai".to_string()
        } else {
            "mincho".to_string()
        }
    } else if contains_any(&name, &["round", "rounded", "maru", "丸", "圆", "圓"]) {
        "round".to_string()
    } else {
        "gothic".to_string()
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

pub fn workflow_with_lettering_status(text: &TextData, status: WorkflowStatus) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    workflow.lettering_status = status;
    workflow
}

pub fn workflow_with_repair_success(text: &TextData, model: &str) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    workflow.repair_status = WorkflowStatus::Succeeded;
    workflow.repair_trace = Some(RepairWorkflowTrace {
        model: Some(model.to_string()),
        ..Default::default()
    });
    workflow
}

pub fn mark_repair_succeeded_ops(
    scene: &Scene,
    page: PageId,
    model: &str,
    region: Option<Region>,
) -> Vec<Op> {
    repair_text_nodes(scene, page)
        .into_iter()
        .filter(|(_, transform, _)| {
            region
                .as_ref()
                .is_none_or(|r| transform_intersects_region(transform, r))
        })
        .map(|(node_id, _, text)| {
            update_text_workflow_op(page, node_id, workflow_with_repair_success(text, model))
        })
        .collect()
}

/// Build an OpenAI image-edit mask for a text transform. Per the Images Edit
/// API, transparent pixels are editable and opaque pixels are preserved.
pub fn openai_edit_mask_for_transform(
    source_width: u32,
    source_height: u32,
    transform: &Transform,
) -> RgbaImage {
    let mut mask = RgbaImage::from_pixel(source_width, source_height, Rgba([0, 0, 0, 255]));
    for y in 0..source_height {
        for x in 0..source_width {
            if transform_contains_pixel(transform, x as f32 + 0.5, y as f32 + 0.5) {
                mask.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    mask
}

/// Build an OpenAI image-edit mask for a text block. If the text workflow has
/// a supported selection shape, use that selection as the editable region;
/// otherwise fall back to the block transform.
pub fn openai_edit_mask_for_text(
    source_width: u32,
    source_height: u32,
    transform: &Transform,
    text: &TextData,
    blobs: Option<&BlobStore>,
) -> RgbaImage {
    let Some(selection) = text.workflow.selection.as_ref() else {
        return openai_edit_mask_for_transform(source_width, source_height, transform);
    };
    let brush_masks = load_selection_brush_masks(selection, blobs, source_width, source_height);
    if !selection
        .shapes
        .iter()
        .any(|shape| is_supported_selection_shape(shape, &brush_masks))
    {
        return openai_edit_mask_for_transform(source_width, source_height, transform);
    }

    let mut mask = RgbaImage::from_pixel(source_width, source_height, Rgba([0, 0, 0, 255]));
    for y in 0..source_height {
        for x in 0..source_width {
            if selection.shapes.iter().any(|shape| {
                selection_shape_contains_pixel(shape, &brush_masks, x as f32 + 0.5, y as f32 + 0.5)
            }) {
                mask.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }
    mask
}

fn load_selection_brush_masks(
    selection: &TextSelection,
    blobs: Option<&BlobStore>,
    source_width: u32,
    source_height: u32,
) -> HashMap<BlobRef, RgbaImage> {
    let Some(blobs) = blobs else {
        return HashMap::new();
    };
    let mut masks = HashMap::new();
    for shape in &selection.shapes {
        let TextSelectionShape::Brush { mask } = shape else {
            continue;
        };
        if masks.contains_key(mask) {
            continue;
        }
        if let Ok(image) = blobs.load_image(mask) {
            let rgba = if image.dimensions() == (source_width, source_height) {
                image.to_rgba8()
            } else {
                image
                    .resize_exact(source_width, source_height, FilterType::Nearest)
                    .to_rgba8()
            };
            masks.insert(mask.clone(), rgba);
        }
    }
    masks
}

fn is_supported_selection_shape(
    shape: &TextSelectionShape,
    brush_masks: &HashMap<BlobRef, RgbaImage>,
) -> bool {
    match shape {
        TextSelectionShape::Rectangle { .. } => true,
        TextSelectionShape::Polygon { points } => points.len() >= 3,
        TextSelectionShape::Brush { mask } => brush_masks.contains_key(mask),
    }
}

fn selection_shape_contains_pixel(
    shape: &TextSelectionShape,
    brush_masks: &HashMap<BlobRef, RgbaImage>,
    px: f32,
    py: f32,
) -> bool {
    match shape {
        TextSelectionShape::Rectangle { transform } => transform_contains_pixel(transform, px, py),
        TextSelectionShape::Polygon { points } => polygon_contains_pixel(points, px, py),
        TextSelectionShape::Brush { mask } => brush_masks
            .get(mask)
            .is_some_and(|brush_mask| brush_mask_contains_pixel(brush_mask, px, py)),
    }
}

fn brush_mask_contains_pixel(mask: &RgbaImage, px: f32, py: f32) -> bool {
    if !px.is_finite() || !py.is_finite() || px < 0.0 || py < 0.0 {
        return false;
    }
    let x = px.floor() as u32;
    let y = py.floor() as u32;
    if x >= mask.width() || y >= mask.height() {
        return false;
    }
    mask.get_pixel(x, y).0[3] > 0
}

fn transform_contains_pixel(transform: &Transform, px: f32, py: f32) -> bool {
    let cx = transform.x + transform.width / 2.0;
    let cy = transform.y + transform.height / 2.0;
    let theta = -transform.rotation_deg.to_radians();
    let dx = px - cx;
    let dy = py - cy;
    let local_x = dx * theta.cos() - dy * theta.sin() + transform.width / 2.0;
    let local_y = dx * theta.sin() + dy * theta.cos() + transform.height / 2.0;
    local_x >= 0.0 && local_x < transform.width && local_y >= 0.0 && local_y < transform.height
}

fn polygon_contains_pixel(points: &[[f32; 2]], px: f32, py: f32) -> bool {
    if points.len() < 3 {
        return false;
    }

    let mut inside = false;
    let mut prev = points.len() - 1;
    for current in 0..points.len() {
        let [x1, y1] = points[current];
        let [x2, y2] = points[prev];
        if x1.is_finite()
            && y1.is_finite()
            && x2.is_finite()
            && y2.is_finite()
            && (y1 > py) != (y2 > py)
        {
            let x_at_y = (x2 - x1) * (py - y1) / (y2 - y1) + x1;
            if px < x_at_y {
                inside = !inside;
            }
        }
        prev = current;
    }
    inside
}

/// Convert a full-page edit result into a transparent repair layer by keeping
/// only pixels that were editable in the OpenAI mask.
pub fn repair_layer_image_from_edit_output(
    edited: &DynamicImage,
    mask: &DynamicImage,
) -> Result<RgbaImage> {
    let mask = mask.to_rgba8();
    let (width, height) = mask.dimensions();
    let edited = if edited.dimensions() == (width, height) {
        edited.to_rgba8()
    } else {
        edited
            .resize_exact(width, height, FilterType::Lanczos3)
            .to_rgba8()
    };
    let mut layer = RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let mut pixel = *edited.get_pixel(x, y);
            pixel.0[3] = 255_u8.saturating_sub(mask.get_pixel(x, y).0[3]);
            layer.put_pixel(x, y, pixel);
        }
    }
    Ok(layer)
}

#[allow(clippy::too_many_arguments)]
pub fn build_bound_repair_layer_ops(
    scene: &Scene,
    page: PageId,
    text_id: NodeId,
    layer_blob: BlobRef,
    mask_blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
    model: &str,
    prompt: &str,
) -> Result<Vec<Op>> {
    let page_ref = scene
        .page(page)
        .with_context(|| format!("page {} not found", page))?;
    let node = page_ref
        .nodes
        .get(&text_id)
        .with_context(|| format!("node {} not found", text_id))?;
    let NodeKind::Text(text) = &node.kind else {
        anyhow::bail!("node {} is not text", text_id);
    };
    let layer_id = NodeId::new();
    let previous_layer = remove_bound_repair_layer_op(scene, page, text);
    let add_at = previous_layer
        .as_ref()
        .map(|(_, index)| (*index).min(page_ref.nodes.len().saturating_sub(1)))
        .unwrap_or(page_ref.nodes.len());
    let workflow = workflow_with_bound_repair_layer(text, layer_id, mask_blob, model, prompt);
    let node = bound_repair_layer_node(
        layer_id,
        text_id,
        layer_blob,
        natural_width,
        natural_height,
        model,
    );
    let mut ops = Vec::with_capacity(3);
    if let Some((remove_op, _)) = previous_layer {
        ops.push(remove_op);
    }
    ops.push(Op::AddNode {
        page,
        node,
        at: add_at,
    });
    ops.push(update_text_workflow_op(page, text_id, workflow));
    Ok(ops)
}

pub fn remove_bound_repair_layer_op(
    scene: &Scene,
    page: PageId,
    text: &TextData,
) -> Option<(Op, usize)> {
    let layer_id = text.workflow.repair_layer?;
    let page_ref = scene.page(page)?;
    let index = page_ref.nodes.get_index_of(&layer_id)?;
    let node = page_ref.nodes.get(&layer_id)?.clone();
    Some((
        Op::RemoveNode {
            page,
            id: layer_id,
            prev_node: node,
            prev_index: index,
        },
        index,
    ))
}

fn workflow_with_bound_repair_layer(
    text: &TextData,
    layer_id: NodeId,
    mask_blob: BlobRef,
    model: &str,
    prompt: &str,
) -> TextWorkflow {
    let mut workflow = text.workflow.clone();
    workflow.repair_layer = Some(layer_id);
    workflow.repair_status = WorkflowStatus::Succeeded;
    workflow.repair_trace = Some(RepairWorkflowTrace {
        model: Some(model.to_string()),
        prompt: Some(prompt.to_string()),
        source_mask: Some(mask_blob),
        error: None,
    });
    workflow
}

fn bound_repair_layer_node(
    id: NodeId,
    text_id: NodeId,
    blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
    model: &str,
) -> Node {
    Node {
        id,
        transform: Transform {
            x: 0.0,
            y: 0.0,
            width: natural_width as f32,
            height: natural_height as f32,
            rotation_deg: 0.0,
        },
        visible: true,
        kind: NodeKind::Image(ImageData {
            role: ImageRole::Custom,
            blob,
            opacity: 1.0,
            natural_width,
            natural_height,
            name: Some(format!("Repair {text_id} ({model})")),
        }),
    }
}

fn transform_intersects_region(transform: &Transform, region: &Region) -> bool {
    let ax0 = transform.x;
    let ay0 = transform.y;
    let ax1 = transform.x + transform.width;
    let ay1 = transform.y + transform.height;
    let bx0 = region.x as f32;
    let by0 = region.y as f32;
    let bx1 = region.x.saturating_add(region.width) as f32;
    let by1 = region.y.saturating_add(region.height) as f32;
    ax0 < bx1 && ax1 > bx0 && ay0 < by1 && ay1 > by0
}

/// Convert a scene `(Transform, TextData)` pair into a `koharu-ml` `TextRegion`
/// for passing back through detector helpers that need geometry + language
/// hints (e.g. CTD's `refine_segmentation_mask`, OCR's `extract_text_block_regions`).
pub fn text_node_to_region(transform: &Transform, text: &TextData) -> koharu_ml::types::TextRegion {
    koharu_ml::types::TextRegion {
        x: transform.x,
        y: transform.y,
        width: transform.width,
        height: transform.height,
        confidence: text.confidence,
        line_polygons: text.line_polygons.clone(),
        source_direction: text.source_direction.map(core_text_direction_to_ml),
        rotation_deg: text.rotation_deg,
        detected_font_size_px: text.detected_font_size_px,
        detector: text.detector.clone(),
    }
}

/// Inverse of `ml_text_direction_to_core`.
pub fn core_text_direction_to_ml(d: koharu_core::TextDirection) -> koharu_ml::types::TextDirection {
    match d {
        koharu_core::TextDirection::Horizontal => koharu_ml::types::TextDirection::Horizontal,
        koharu_core::TextDirection::Vertical => koharu_ml::types::TextDirection::Vertical,
    }
}

// ---------------------------------------------------------------------------
// Op constructors
// ---------------------------------------------------------------------------

/// Build an `AddNode` for a new `Image { role }` layer.
#[allow(clippy::too_many_arguments)]
pub fn add_image_node_op(
    page: PageId,
    role: ImageRole,
    blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
    transform: Transform,
    visible: bool,
    at: usize,
) -> Op {
    let node = Node {
        id: NodeId::new(),
        transform,
        visible,
        kind: NodeKind::Image(ImageData {
            role,
            blob,
            opacity: 1.0,
            natural_width,
            natural_height,
            name: None,
        }),
    };
    Op::AddNode { page, node, at }
}

/// Build an `AddNode` for a new `Mask { role }` layer.
pub fn add_mask_node_op(
    page: PageId,
    role: MaskRole,
    blob: BlobRef,
    transform: Transform,
    visible: bool,
    at: usize,
) -> Op {
    let node = Node {
        id: NodeId::new(),
        transform,
        visible,
        kind: NodeKind::Mask(MaskData { role, blob }),
    };
    Op::AddNode { page, node, at }
}

/// Replace or add an `Image { role }` blob for `page`. If a node already
/// exists with that role, emits an `UpdateNode` with `ImageDataPatch`.
/// Otherwise emits `AddNode` at the top of the stack (renderer role) or
/// after Source (inpainted/custom role).
pub fn upsert_image_blob(
    scene: &Scene,
    page: PageId,
    role: ImageRole,
    blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
) -> Op {
    if let Some((node_id, _)) = find_image_node(scene, page, role) {
        Op::UpdateNode {
            page,
            id: node_id,
            patch: koharu_core::NodePatch {
                data: Some(NodeDataPatch::Image(koharu_core::ImageDataPatch {
                    blob: Some(blob),
                    opacity: None,
                    name: None,
                    natural_width: Some(natural_width),
                    natural_height: Some(natural_height),
                })),
                transform: None,
                visible: None,
            },
            prev: koharu_core::NodePatch::default(),
        }
    } else {
        let at = {
            let page_ref = scene.page(page);
            let base = page_ref.map(|p| p.nodes.len()).unwrap_or(0);
            match role {
                // Rendered on top.
                ImageRole::Rendered => base,
                // Inpainted directly after source (index 1 if source is present).
                ImageRole::Inpainted => 1.min(base),
                // Custom / Source → append.
                _ => base,
            }
        };
        add_image_node_op(
            page,
            role,
            blob,
            natural_width,
            natural_height,
            Transform::default(),
            role != ImageRole::Rendered, // hide Rendered by default; make a toggle explicit
            at,
        )
    }
}

/// Write the full-page lettering cleanup result without changing per-block
/// repair workflow state. Repair status is owned by repair-layer engines.
pub fn inpainted_image_ops(
    scene: &Scene,
    page: PageId,
    blob: BlobRef,
    natural_width: u32,
    natural_height: u32,
) -> Vec<Op> {
    vec![upsert_image_blob(
        scene,
        page,
        ImageRole::Inpainted,
        blob,
        natural_width,
        natural_height,
    )]
}

/// Replace or add a `Mask { role }` blob for `page`.
pub fn upsert_mask_blob(scene: &Scene, page: PageId, role: MaskRole, blob: BlobRef) -> Op {
    if let Some((node_id, _)) = find_mask_node(scene, page, role) {
        Op::UpdateNode {
            page,
            id: node_id,
            patch: koharu_core::NodePatch {
                data: Some(NodeDataPatch::Mask(koharu_core::MaskDataPatch {
                    blob: Some(blob),
                })),
                transform: None,
                visible: None,
            },
            prev: koharu_core::NodePatch::default(),
        }
    } else {
        let at = scene.page(page).map(|p| p.nodes.len()).unwrap_or(0);
        let visible = matches!(role, MaskRole::BrushInpaint);
        add_mask_node_op(page, role, blob, Transform::default(), visible, at)
    }
}

/// Build a `Node` ready to be added for a new Text region.
pub fn new_text_node(bbox: [f32; 4], text_data: TextData) -> Node {
    Node {
        id: NodeId::new(),
        transform: Transform {
            x: bbox[0],
            y: bbox[1],
            width: bbox[2] - bbox[0],
            height: bbox[3] - bbox[1],
            rotation_deg: text_data.rotation_deg.unwrap_or(0.0),
        },
        visible: true,
        kind: NodeKind::Text(text_data),
    }
}

/// Small helper: decoded image dimensions.
pub fn image_dimensions(image: &DynamicImage) -> (u32, u32) {
    image.dimensions()
}

/// Translate the `koharu-ml` `TextDirection` primitive into the scene-layer one.
pub fn ml_text_direction_to_core(d: koharu_ml::types::TextDirection) -> koharu_core::TextDirection {
    match d {
        koharu_ml::types::TextDirection::Horizontal => koharu_core::TextDirection::Horizontal,
        koharu_ml::types::TextDirection::Vertical => koharu_core::TextDirection::Vertical,
    }
}

/// Translate a `koharu-ml::TextRegion` (detector output) into a scene-layer
/// `(bbox, TextData)` pair ready for `new_text_node`.
pub fn text_region_to_pair(
    r: koharu_ml::types::TextRegion,
    default_detector: &'static str,
) -> ([f32; 4], TextData) {
    let bbox = text_region_outer_bbox(&r);
    let data = TextData {
        confidence: r.confidence,
        source_direction: r.source_direction.map(ml_text_direction_to_core),
        line_polygons: r.line_polygons,
        rotation_deg: r.rotation_deg,
        detected_font_size_px: r.detected_font_size_px,
        detector: r.detector.or_else(|| Some(default_detector.to_string())),
        ..Default::default()
    };
    (bbox, data)
}

fn text_region_outer_bbox(r: &koharu_ml::types::TextRegion) -> [f32; 4] {
    let detector_bbox = [r.x, r.y, r.x + r.width, r.y + r.height];
    let Some(polygons) = r
        .line_polygons
        .as_ref()
        .filter(|polygons| !polygons.is_empty())
    else {
        return detector_bbox;
    };

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for point in polygons.iter().flat_map(|polygon| polygon.iter()) {
        let [x, y] = *point;
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
        [min_x, min_y, max_x, max_y]
    } else {
        detector_bbox
    }
}

/// Current node count on `page`, or 0 if the page doesn't exist.
pub fn page_node_count(scene: &Scene, page: PageId) -> usize {
    scene.page(page).map(|p| p.nodes.len()).unwrap_or(0)
}

/// Emit `RemoveNode` ops for every text node currently on `page`. Detectors
/// prepend these so a re-detect replaces the previous blocks instead of
/// layering on top. `prev_node` / `prev_index` are the best snapshot we have
/// at emission time — `ops::apply` overwrites them with the live state for
/// undo anyway.
pub fn clear_text_nodes_ops(scene: &Scene, page: PageId) -> Vec<Op> {
    let Some(page_ref) = scene.page(page) else {
        return Vec::new();
    };
    page_ref
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, (_, node))| matches!(&node.kind, NodeKind::Text(_)))
        .map(|(idx, (id, node))| Op::RemoveNode {
            page,
            id: *id,
            prev_node: node.clone(),
            prev_index: idx,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Manga reading-order sort (Recursive XY-Cut)
//
// Right-to-left columns, top-to-bottom within each column. Shared by every
// detector that emits text blocks (CTD, comic-text-bubble, PP-DocLayout).
// ---------------------------------------------------------------------------

/// Sort `(bbox, data)` pairs in a reading order (RTL, LTR, or Custom).
pub fn sort_manga_reading_order<T>(blocks: &mut [([f32; 4], T)], order: ReadingOrder) {
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum Axis {
        X,
        Y,
    }

    if order == ReadingOrder::Custom {
        return;
    }

    if blocks.len() <= 1 {
        return;
    }

    let mut widths: Vec<f32> = blocks.iter().map(|(b, _)| b[2] - b[0]).collect();
    let mut heights: Vec<f32> = blocks.iter().map(|(b, _)| b[3] - b[1]).collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let median_w = widths[widths.len() / 2].max(1.0);
    let median_h = heights[heights.len() / 2].max(1.0);
    let min_gap_x = (median_w * 0.15).max(10.0);
    let min_gap_y = (median_h * 0.10).max(8.0);

    fn xy_cut_recursive<T>(
        blocks: &mut [([f32; 4], T)],
        min_gap_x: f32,
        min_gap_y: f32,
        order: ReadingOrder,
    ) {
        use std::cmp::Ordering;
        if blocks.len() <= 1 {
            return;
        }
        let cut = find_best_cut(blocks, min_gap_x, min_gap_y);
        let Some((axis, gap)) = cut else {
            let row_height = min_gap_y * 4.0;
            blocks.sort_by(|a, b| {
                let row_a = (a.0[1] / row_height).floor();
                let row_b = (b.0[1] / row_height).floor();
                row_a
                    .partial_cmp(&row_b)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| match order {
                        ReadingOrder::Rtl => b.0[0].partial_cmp(&a.0[0]).unwrap_or(Ordering::Equal),
                        ReadingOrder::Ltr => a.0[0].partial_cmp(&b.0[0]).unwrap_or(Ordering::Equal),
                        _ => Ordering::Equal,
                    })
            });
            return;
        };

        let cut_coord = (gap.0 + gap.1) / 2.0;
        blocks.sort_by_key(|(b, _)| {
            if axis == Axis::X {
                let center_x = b[0] + (b[2] - b[0]) * 0.5;
                match order {
                    ReadingOrder::Rtl => center_x < cut_coord, // Right first
                    ReadingOrder::Ltr => center_x > cut_coord, // Left first
                    _ => false,
                }
            } else {
                // Top partition first: items whose center is BELOW cut go second.
                (b[1] + (b[3] - b[1]) * 0.5) > cut_coord
            }
        });

        let group1_len = blocks
            .iter()
            .filter(|(b, _)| {
                if axis == Axis::X {
                    let center_x = b[0] + (b[2] - b[0]) * 0.5;
                    match order {
                        ReadingOrder::Rtl => center_x >= cut_coord,
                        ReadingOrder::Ltr => center_x <= cut_coord,
                        _ => true,
                    }
                } else {
                    (b[1] + (b[3] - b[1]) * 0.5) <= cut_coord
                }
            })
            .count();

        if group1_len == 0 || group1_len == blocks.len() {
            blocks.sort_by(|a, b| match order {
                ReadingOrder::Rtl => b.0[0].partial_cmp(&a.0[0]).unwrap_or(Ordering::Equal),
                ReadingOrder::Ltr => a.0[0].partial_cmp(&b.0[0]).unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            });
            return;
        }

        let (left, right) = blocks.split_at_mut(group1_len);
        xy_cut_recursive(left, min_gap_x, min_gap_y, order);
        xy_cut_recursive(right, min_gap_x, min_gap_y, order);
    }

    fn find_best_cut<T>(
        blocks: &[([f32; 4], T)],
        min_gap_x: f32,
        min_gap_y: f32,
    ) -> Option<(Axis, (f32, f32))> {
        let mut x_intervals: Vec<(f32, f32)> = blocks.iter().map(|(b, _)| (b[0], b[2])).collect();
        let mut y_intervals: Vec<(f32, f32)> = blocks.iter().map(|(b, _)| (b[1], b[3])).collect();
        x_intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        y_intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let gap_x = find_largest_gap(&x_intervals, min_gap_x);
        let gap_y = find_largest_gap(&y_intervals, min_gap_y);
        match (gap_x, gap_y) {
            (Some(gx), Some(gy)) => {
                let width_y = gy.1 - gy.0;
                let width_x = gx.1 - gx.0;
                if width_y > 12.0 || width_y > (width_x * 0.4) {
                    Some((Axis::Y, gy))
                } else {
                    Some((Axis::X, gx))
                }
            }
            (None, Some(gy)) => Some((Axis::Y, gy)),
            (Some(gx), None) => Some((Axis::X, gx)),
            (None, None) => None,
        }
    }

    fn find_largest_gap(intervals: &[(f32, f32)], min_gap: f32) -> Option<(f32, f32)> {
        if intervals.is_empty() {
            return None;
        }
        let mut largest: Option<(f32, f32)> = None;
        let mut current_max_end = intervals[0].1;
        for interval in intervals.iter().skip(1) {
            if interval.0 > current_max_end {
                let gap = interval.0 - current_max_end;
                if gap >= min_gap
                    && match largest {
                        Some(best) => gap > best.1 - best.0,
                        None => true,
                    }
                {
                    largest = Some((current_max_end, interval.0));
                }
            }
            current_max_end = current_max_end.max(interval.1);
        }
        largest
    }

    xy_cut_recursive(blocks, min_gap_x, min_gap_y, order);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use koharu_core::{
        BlobRef, FontPrediction, ImageRole, NamedFontPrediction, NodeDataPatch, NodeId, NodeKind,
        ReadingOrder, TextDirection, TextSelection, TextSelectionShape, TextWorkflow,
        TextWorkflowMode,
    };
    use koharu_ml::types::TextRegion;

    #[test]
    fn test_reading_order_sort() {
        // Two blocks side-by-side
        // B1: [100, 100, 200, 200] (Left)
        // B2: [300, 100, 400, 200] (Right)
        let b1 = [100.0, 100.0, 200.0, 200.0];
        let b2 = [300.0, 100.0, 400.0, 200.0];

        let mut blocks = vec![(b1, "left"), (b2, "right")];

        // RTL: Right should come first
        sort_manga_reading_order(&mut blocks, ReadingOrder::Rtl);
        assert_eq!(blocks[0].1, "right");
        assert_eq!(blocks[1].1, "left");

        // LTR: Left should come first
        sort_manga_reading_order(&mut blocks, ReadingOrder::Ltr);
        assert_eq!(blocks[0].1, "left");
        assert_eq!(blocks[1].1, "right");
    }

    #[test]
    fn text_region_to_pair_uses_line_polygon_outer_bbox_when_detector_bbox_is_smaller() {
        let region = TextRegion {
            x: 20.0,
            y: 20.0,
            width: 30.0,
            height: 30.0,
            confidence: 0.9,
            line_polygons: Some(vec![
                [[5.0, 8.0], [45.0, 8.0], [45.0, 24.0], [5.0, 24.0]],
                [[70.0, 40.0], [110.0, 40.0], [110.0, 72.0], [70.0, 72.0]],
            ]),
            ..Default::default()
        };

        let (bbox, text) = text_region_to_pair(region, "ctd");

        assert_eq!(bbox, [5.0, 8.0, 110.0, 72.0]);
        assert_eq!(text.line_polygons.as_ref().expect("line polygons").len(), 2);
    }

    #[test]
    fn workflow_with_font_trace_records_target_font_categories() {
        let text = TextData::default();
        let prediction = FontPrediction {
            named_fonts: vec![NamedFontPrediction {
                index: 7,
                name: "Koharu Rounded Gothic".to_string(),
                language: Some("ja".to_string()),
                probability: 0.91,
                serif: false,
            }],
            direction: TextDirection::Horizontal,
            ..Default::default()
        };

        let workflow = workflow_with_font_trace(&text, &prediction);
        let trace = workflow.font_trace.expect("font trace");

        assert_eq!(trace.primary_category.as_deref(), Some("sans_serif"));
        assert_eq!(trace.secondary_category.as_deref(), Some("round"));
        assert_eq!(
            trace.selected_font.as_deref(),
            Some("Koharu Rounded Gothic")
        );
        assert_eq!(trace.candidate_fonts, vec!["Koharu Rounded Gothic"]);
    }

    #[test]
    fn workflow_with_font_trace_classifies_serif_kai_candidates() {
        let text = TextData::default();
        let prediction = FontPrediction {
            named_fonts: vec![NamedFontPrediction {
                index: 2,
                name: "Koharu Kaiti".to_string(),
                language: Some("ja".to_string()),
                probability: 0.84,
                serif: true,
            }],
            direction: TextDirection::Horizontal,
            ..Default::default()
        };

        let workflow = workflow_with_font_trace(&text, &prediction);
        let trace = workflow.font_trace.expect("font trace");

        assert_eq!(trace.primary_category.as_deref(), Some("serif"));
        assert_eq!(trace.secondary_category.as_deref(), Some("kai"));
    }

    #[test]
    fn mark_repair_succeeded_ops_filters_to_intersecting_region() {
        let page_id = PageId::new();
        let near_id = NodeId::new();
        let far_id = NodeId::new();
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 500, 500);
        page.id = page_id;
        for (node_id, x) in [(near_id, 10.0), (far_id, 300.0)] {
            page.nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    transform: Transform {
                        x,
                        y: 10.0,
                        width: 40.0,
                        height: 40.0,
                        rotation_deg: 0.0,
                    },
                    visible: true,
                    kind: NodeKind::Text(TextData {
                        workflow: TextWorkflow {
                            modes: vec![TextWorkflowMode::Repair],
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                },
            );
        }
        scene.pages.insert(page_id, page);

        let ops = mark_repair_succeeded_ops(
            &scene,
            page_id,
            "lama-manga",
            Some(Region {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }),
        );

        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Op::UpdateNode { id, .. } => assert_eq!(*id, near_id),
            other => panic!("expected UpdateNode, got {other:?}"),
        }
    }

    #[test]
    fn lettering_text_regions_excludes_repair_only_nodes() {
        let page_id = PageId::new();
        let lettering_id = NodeId::new();
        let repair_id = NodeId::new();
        let dual_id = NodeId::new();
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 500, 500);
        page.id = page_id;

        for (node_id, x, modes) in [
            (lettering_id, 10.0, vec![TextWorkflowMode::Lettering]),
            (repair_id, 80.0, vec![TextWorkflowMode::Repair]),
            (
                dual_id,
                150.0,
                vec![TextWorkflowMode::Lettering, TextWorkflowMode::Repair],
            ),
        ] {
            page.nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    transform: Transform {
                        x,
                        y: 20.0,
                        width: 30.0,
                        height: 40.0,
                        rotation_deg: 0.0,
                    },
                    visible: true,
                    kind: NodeKind::Text(TextData {
                        workflow: TextWorkflow {
                            modes,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                },
            );
        }
        scene.pages.insert(page_id, page);

        let regions = lettering_text_regions(&scene, page_id);

        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].x, 10.0);
        assert_eq!(regions[1].x, 150.0);
    }

    #[test]
    fn inpainted_image_ops_do_not_mark_repair_workflows() {
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 500, 500);
        page.id = page_id;
        page.nodes.insert(
            text_id,
            Node {
                id: text_id,
                transform: Transform::default(),
                visible: true,
                kind: NodeKind::Text(TextData {
                    workflow: TextWorkflow {
                        modes: vec![TextWorkflowMode::Repair],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            },
        );
        scene.pages.insert(page_id, page);

        let ops = inpainted_image_ops(&scene, page_id, BlobRef::new("inpainted"), 500, 500);

        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], Op::AddNode { .. }));
        assert!(!ops.iter().any(|op| matches!(op, Op::UpdateNode { .. })));
    }

    #[test]
    fn openai_edit_mask_for_transform_marks_text_region_transparent_and_rest_opaque() {
        let mask = openai_edit_mask_for_transform(
            20,
            20,
            &Transform {
                x: 5.0,
                y: 6.0,
                width: 4.0,
                height: 3.0,
                rotation_deg: 0.0,
            },
        );

        assert_eq!(mask.dimensions(), (20, 20));
        assert_eq!(mask.get_pixel(6, 7).0[3], 0);
        assert_eq!(mask.get_pixel(0, 0).0[3], 255);
    }

    #[test]
    fn openai_edit_mask_for_text_uses_polygon_selection_when_present() {
        let mut text = TextData::default();
        text.workflow.selection = Some(TextSelection {
            shapes: vec![TextSelectionShape::Polygon {
                points: vec![[2.0, 2.0], [9.0, 2.0], [2.0, 9.0]],
            }],
        });
        let fallback_transform = Transform {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 12.0,
            rotation_deg: 0.0,
        };

        let mask = openai_edit_mask_for_text(12, 12, &fallback_transform, &text, None);

        assert_eq!(mask.get_pixel(3, 3).0[3], 0);
        assert_eq!(mask.get_pixel(8, 8).0[3], 255);
        assert_eq!(mask.get_pixel(11, 11).0[3], 255);
    }

    #[test]
    fn openai_edit_mask_for_text_uses_brush_selection_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = BlobStore::open(dir.path()).expect("blob store");
        let mut brush = RgbaImage::new(6, 6);
        brush.put_pixel(1, 2, Rgba([255, 255, 255, 255]));
        brush.put_pixel(4, 3, Rgba([255, 255, 255, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(brush)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode brush mask");
        let brush_blob = store.put_bytes(bytes.get_ref()).expect("store brush mask");

        let mut text = TextData::default();
        text.workflow.selection = Some(TextSelection {
            shapes: vec![TextSelectionShape::Brush { mask: brush_blob }],
        });
        let fallback_transform = Transform {
            x: 0.0,
            y: 0.0,
            width: 6.0,
            height: 6.0,
            rotation_deg: 0.0,
        };

        let mask = openai_edit_mask_for_text(6, 6, &fallback_transform, &text, Some(&store));

        assert_eq!(mask.get_pixel(1, 2).0[3], 0);
        assert_eq!(mask.get_pixel(4, 3).0[3], 0);
        assert_eq!(mask.get_pixel(0, 0).0[3], 255);
    }

    #[test]
    fn openai_edit_mask_for_text_falls_back_to_transform_without_selection() {
        let text = TextData::default();
        let transform = Transform {
            x: 5.0,
            y: 6.0,
            width: 4.0,
            height: 3.0,
            rotation_deg: 0.0,
        };

        let mask = openai_edit_mask_for_text(20, 20, &transform, &text, None);

        assert_eq!(mask.get_pixel(6, 7).0[3], 0);
        assert_eq!(mask.get_pixel(0, 0).0[3], 255);
    }

    #[test]
    fn repair_layer_image_keeps_only_editable_mask_pixels() {
        let mut edited = RgbaImage::new(2, 1);
        edited.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        edited.put_pixel(1, 0, Rgba([200, 210, 220, 255]));
        let mut mask = RgbaImage::new(2, 1);
        mask.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        mask.put_pixel(1, 0, Rgba([0, 0, 0, 255]));

        let layer = repair_layer_image_from_edit_output(
            &DynamicImage::ImageRgba8(edited),
            &DynamicImage::ImageRgba8(mask),
        )
        .expect("extract repair layer");

        assert_eq!(layer.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(layer.get_pixel(1, 0).0, [200, 210, 220, 0]);
    }

    #[test]
    fn repair_layer_image_resizes_edit_output_to_mask_dimensions() {
        let edited = RgbaImage::from_pixel(4, 2, Rgba([100, 110, 120, 255]));
        let mut mask = RgbaImage::new(2, 1);
        mask.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        mask.put_pixel(1, 0, Rgba([0, 0, 0, 255]));

        let layer = repair_layer_image_from_edit_output(
            &DynamicImage::ImageRgba8(edited),
            &DynamicImage::ImageRgba8(mask),
        )
        .expect("extract repair layer");

        assert_eq!(layer.dimensions(), (2, 1));
        assert_eq!(layer.get_pixel(0, 0).0[3], 255);
        assert_eq!(layer.get_pixel(1, 0).0[3], 0);
    }

    #[test]
    fn build_bound_repair_layer_ops_adds_custom_layer_and_binds_workflow() {
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let layer_blob = BlobRef::new("layer");
        let mask_blob = BlobRef::new("mask");
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 100, 120);
        page.id = page_id;
        page.nodes.insert(
            text_id,
            Node {
                id: text_id,
                transform: Transform {
                    x: 20.0,
                    y: 30.0,
                    width: 40.0,
                    height: 50.0,
                    rotation_deg: 12.0,
                },
                visible: true,
                kind: NodeKind::Text(TextData {
                    text: Some("原文".to_string()),
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

        let ops = build_bound_repair_layer_ops(
            &scene,
            page_id,
            text_id,
            layer_blob.clone(),
            mask_blob.clone(),
            100,
            120,
            "gpt-image-2",
            "replace with translation",
        )
        .expect("build repair ops");

        assert_eq!(ops.len(), 2);
        let layer_id = match &ops[0] {
            Op::AddNode { node, at, .. } => {
                assert_eq!(*at, 1);
                assert_eq!(node.transform.x, 0.0);
                assert_eq!(node.transform.y, 0.0);
                assert_eq!(node.transform.width, 100.0);
                assert_eq!(node.transform.height, 120.0);
                let NodeKind::Image(image) = &node.kind else {
                    panic!("expected custom image layer");
                };
                assert_eq!(image.role, ImageRole::Custom);
                assert_eq!(image.blob, layer_blob);
                assert_eq!(image.natural_width, 100);
                assert_eq!(image.natural_height, 120);
                assert_eq!(
                    image.name.as_deref(),
                    Some(format!("Repair {text_id} (gpt-image-2)").as_str())
                );
                node.id
            }
            other => panic!("expected AddNode, got {other:?}"),
        };

        match &ops[1] {
            Op::UpdateNode { id, patch, .. } => {
                assert_eq!(*id, text_id);
                let Some(NodeDataPatch::Text(text_patch)) = &patch.data else {
                    panic!("expected text workflow patch");
                };
                let workflow = text_patch.workflow.as_ref().expect("workflow patch");
                assert_eq!(workflow.repair_layer, Some(layer_id));
                assert_eq!(workflow.repair_status, WorkflowStatus::Succeeded);
                let trace = workflow.repair_trace.as_ref().expect("repair trace");
                assert_eq!(trace.model.as_deref(), Some("gpt-image-2"));
                assert_eq!(trace.prompt.as_deref(), Some("replace with translation"));
                assert_eq!(trace.source_mask.as_ref(), Some(&mask_blob));
            }
            other => panic!("expected UpdateNode, got {other:?}"),
        }
    }

    #[test]
    fn build_bound_repair_layer_ops_removes_previous_bound_layer_before_replacing_it() {
        let page_id = PageId::new();
        let text_id = NodeId::new();
        let old_layer_id = NodeId::new();
        let layer_blob = BlobRef::new("new-layer");
        let mask_blob = BlobRef::new("mask");
        let mut scene = Scene::default();
        let mut page = koharu_core::Page::new("p1", 100, 120);
        page.id = page_id;
        page.nodes.insert(
            text_id,
            Node {
                id: text_id,
                transform: Transform::default(),
                visible: true,
                kind: NodeKind::Text(TextData {
                    workflow: TextWorkflow {
                        modes: vec![TextWorkflowMode::Repair],
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
                    natural_height: 120,
                    name: Some("old repair".into()),
                }),
            },
        );
        scene.pages.insert(page_id, page);

        let ops = build_bound_repair_layer_ops(
            &scene,
            page_id,
            text_id,
            layer_blob,
            mask_blob,
            100,
            120,
            "gpt-image-2",
            "replace with translation",
        )
        .expect("build repair ops");

        assert_eq!(ops.len(), 3);
        match &ops[0] {
            Op::RemoveNode {
                id,
                prev_node,
                prev_index,
                ..
            } => {
                assert_eq!(*id, old_layer_id);
                assert_eq!(prev_node.id, old_layer_id);
                assert_eq!(*prev_index, 1);
            }
            other => panic!("expected RemoveNode, got {other:?}"),
        }
        match &ops[1] {
            Op::AddNode { at, .. } => assert_eq!(*at, 1),
            other => panic!("expected AddNode, got {other:?}"),
        }
    }
}
