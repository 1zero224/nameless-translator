use std::io::Cursor;

use anyhow::Result;
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage, imageops};
use koharu_core::{FontPrediction, TextData, TextStyle, Transform};

use crate::renderer::{PageRenderOptions, RenderBlockInput, Renderer};

use super::taxonomy::FontCandidate;

pub(super) fn crop_text(source: &DynamicImage, transform: &Transform) -> DynamicImage {
    let (source_width, source_height) = source.dimensions();
    let x = transform.x.max(0.0).floor() as u32;
    let y = transform.y.max(0.0).floor() as u32;
    let max_width = source_width.saturating_sub(x).max(1);
    let max_height = source_height.saturating_sub(y).max(1);
    let width = (transform.width.max(1.0).ceil() as u32).min(max_width);
    let height = (transform.height.max(1.0).ceil() as u32).min(max_height);
    source.crop_imm(x, y, width, height)
}

pub(super) fn build_comparison_grid(
    renderer: &Renderer,
    crop: &DynamicImage,
    transform: &Transform,
    text: &TextData,
    prediction: &FontPrediction,
    candidates: &[FontCandidate],
) -> Result<DynamicImage> {
    let mut tiles = vec![crop.clone()];
    for candidate in candidates {
        tiles.push(render_candidate_preview(
            renderer, crop, transform, text, prediction, candidate,
        )?);
    }
    Ok(grid_from_tiles(&tiles))
}

pub(super) fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    image.write_to(&mut buf, ImageFormat::Png)?;
    Ok(buf.into_inner())
}

pub(super) fn sample_text(text: &TextData) -> String {
    text.text
        .as_deref()
        .or(text.translation.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Sample")
        .to_string()
}

fn render_candidate_preview(
    renderer: &Renderer,
    crop: &DynamicImage,
    transform: &Transform,
    text: &TextData,
    prediction: &FontPrediction,
    candidate: &FontCandidate,
) -> Result<DynamicImage> {
    let width = crop.width().clamp(96, 320);
    let height = crop.height().clamp(96, 320);
    let base = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([255; 4])));
    let block = RenderBlockInput {
        node_id: koharu_core::NodeId::new(),
        transform: Transform {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            rotation_deg: transform.rotation_deg,
        },
        translation: sample_text(text),
        style: Some(candidate_style(candidate, text, height)),
        font_prediction: Some(prediction.clone()),
        source_direction: text.source_direction,
        rendered_direction: text.rendered_direction,
        lock_layout_box: true,
        include_in_final_render: true,
    };
    renderer
        .render_page(
            &base,
            None,
            None,
            width,
            height,
            &[block],
            &PageRenderOptions::default(),
        )
        .map(|output| output.final_render)
}

fn candidate_style(candidate: &FontCandidate, text: &TextData, height: u32) -> TextStyle {
    let mut style = TextStyle {
        font_families: vec![candidate.post_script_name.clone()],
        ..Default::default()
    };
    style.font_size = text
        .detected_font_size_px
        .map(|size| size.clamp(10.0, height as f32 * 0.9));
    style
}

fn grid_from_tiles(tiles: &[DynamicImage]) -> DynamicImage {
    let tile = 220_u32;
    let padding = 12_u32;
    let columns = ((tiles.len() as f32).sqrt().ceil() as u32).max(1);
    let rows = (tiles.len() as u32).div_ceil(columns);
    let width = padding + columns * (tile + padding);
    let height = padding + rows * (tile + padding);
    let mut grid = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    for (index, image) in tiles.iter().enumerate() {
        overlay_tile(&mut grid, image, index, columns, tile, padding);
    }
    DynamicImage::ImageRgba8(grid)
}

fn overlay_tile(
    grid: &mut RgbaImage,
    image: &DynamicImage,
    index: usize,
    columns: u32,
    tile: u32,
    padding: u32,
) {
    let column = index as u32 % columns;
    let row = index as u32 / columns;
    let x = padding + column * (tile + padding);
    let y = padding + row * (tile + padding);
    let fitted = image.resize(tile, tile, imageops::FilterType::Lanczos3);
    let px = x + (tile - fitted.width()) / 2;
    let py = y + (tile - fitted.height()) / 2;
    imageops::overlay(grid, &fitted.to_rgba8(), px.into(), py.into());
}
