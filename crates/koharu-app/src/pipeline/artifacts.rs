//! Artifact enum: the pipeline's dependency currency.
//!
//! Engines declare `needs: &[Artifact]` and `produces: &[Artifact]`; the DAG
//! resolver derives execution order from these. Artifacts are satisfied when
//! the corresponding scene node / field is present on the target page.

use koharu_core::{ImageRole, MaskRole, NodeKind, Page, TextWorkflowMode};

/// Every named "thing" a pipeline step depends on or writes.
///
/// These correspond to scene node kinds + role tags (see `§7.1` of the
/// data-model design). Textual artifacts (OcrText, Translations) are
/// satisfied when every Text node on the page has the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Artifact {
    /// `Image { role: Source }` — always present on a valid page.
    SourceImage,
    /// `Image { role: Inpainted }` node present.
    Inpainted,
    /// `Mask { role: Segment }` node present.
    SegmentMask,
    /// `Mask { role: Bubble }` node present — bubble-interior mask from
    /// `speech-bubble-segmentation`, consumed by the renderer to grow
    /// text layout boxes inside the bubble.
    BubbleMask,
    /// `Mask { role: BrushInpaint }` node present.
    BrushMask,
    /// At least one `Text` node exists on the page.
    TextBoxes,
    /// Every `Text` node has `text` set.
    OcrText,
    /// Every `Text` node has `font_prediction` set.
    FontPredictions,
    /// Every `Text` node has `translation` set (or has no OCR text).
    Translations,
    /// Every `Text` node has a rendered sprite.
    RenderedSprites,
    /// Repair-mode text nodes have bound custom repair image layers.
    RepairLayers,
    /// `Image { role: Rendered }` node present.
    FinalRender,
}

impl Artifact {
    /// Whether this artifact is satisfied by the given page's current state.
    pub fn ready(self, page: &Page) -> bool {
        match self {
            Artifact::SourceImage => has_image_role(page, ImageRole::Source),
            Artifact::Inpainted => has_image_role(page, ImageRole::Inpainted),
            Artifact::SegmentMask => has_mask_role(page, MaskRole::Segment),
            Artifact::BubbleMask => has_mask_role(page, MaskRole::Bubble),
            Artifact::BrushMask => has_mask_role(page, MaskRole::BrushInpaint),
            Artifact::TextBoxes => page
                .nodes
                .values()
                .any(|n| matches!(n.kind, NodeKind::Text(_))),
            Artifact::OcrText => every_text(page, |t| {
                t.text.as_ref().is_some_and(|s| !s.trim().is_empty())
            }),
            Artifact::FontPredictions => every_text_matching(
                page,
                |t| has_workflow_mode(t, TextWorkflowMode::Lettering),
                |t| t.font_prediction.is_some(),
            ),
            Artifact::Translations => every_text_matching(page, has_workflow_mode_any, |t| {
                t.translation.as_ref().is_some_and(|s| !s.trim().is_empty())
            }),
            Artifact::RenderedSprites => every_text_matching(
                page,
                |t| has_workflow_mode(t, TextWorkflowMode::Lettering),
                |t| t.sprite.is_some(),
            ),
            Artifact::RepairLayers => every_text_matching(
                page,
                |t| has_workflow_mode(t, TextWorkflowMode::Repair),
                |t| t.workflow.repair_layer.is_some(),
            ),
            Artifact::FinalRender => has_image_role(page, ImageRole::Rendered),
        }
    }
}

fn has_image_role(page: &Page, role: ImageRole) -> bool {
    page.nodes.values().any(|n| match &n.kind {
        NodeKind::Image(img) => img.role == role,
        _ => false,
    })
}

fn has_mask_role(page: &Page, role: MaskRole) -> bool {
    page.nodes.values().any(|n| match &n.kind {
        NodeKind::Mask(mask) => mask.role == role,
        _ => false,
    })
}

fn every_text(page: &Page, predicate: impl Fn(&koharu_core::TextData) -> bool) -> bool {
    let texts: Vec<_> = page
        .nodes
        .values()
        .filter_map(|n| match &n.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        })
        .collect();
    // Empty page trivially satisfies text artifacts.
    if texts.is_empty() {
        return true;
    }
    texts.into_iter().all(predicate)
}

fn every_text_matching(
    page: &Page,
    relevant: impl Fn(&koharu_core::TextData) -> bool,
    predicate: impl Fn(&koharu_core::TextData) -> bool,
) -> bool {
    every_text(page, |text| !relevant(text) || predicate(text))
}

fn has_workflow_mode(text: &koharu_core::TextData, mode: TextWorkflowMode) -> bool {
    text.workflow.modes.contains(&mode)
}

fn has_workflow_mode_any(text: &koharu_core::TextData) -> bool {
    has_workflow_mode(text, TextWorkflowMode::Lettering)
        || has_workflow_mode(text, TextWorkflowMode::Repair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use koharu_core::{BlobRef, FontPrediction, Node, NodeId, TextData, TextWorkflow, Transform};

    #[test]
    fn font_predictions_ignore_repair_only_text_blocks() {
        let page = page_with_texts([
            text_node(
                TextWorkflow {
                    modes: vec![TextWorkflowMode::Lettering],
                    ..Default::default()
                },
                |text| text.font_prediction = Some(FontPrediction::default()),
            ),
            text_node(
                TextWorkflow {
                    modes: vec![TextWorkflowMode::Repair],
                    ..Default::default()
                },
                |_| {},
            ),
        ]);

        assert!(Artifact::FontPredictions.ready(&page));
    }

    #[test]
    fn rendered_sprites_ignore_repair_only_text_blocks() {
        let page = page_with_texts([
            text_node(
                TextWorkflow {
                    modes: vec![TextWorkflowMode::Lettering],
                    ..Default::default()
                },
                |text| text.sprite = Some(BlobRef::new("sprite")),
            ),
            text_node(
                TextWorkflow {
                    modes: vec![TextWorkflowMode::Repair],
                    ..Default::default()
                },
                |_| {},
            ),
        ]);

        assert!(Artifact::RenderedSprites.ready(&page));
    }

    #[test]
    fn translations_require_manual_repair_blocks_to_have_translation() {
        let page = page_with_texts([text_node(
            TextWorkflow {
                modes: vec![TextWorkflowMode::Repair],
                ..Default::default()
            },
            |text| {
                text.text = None;
                text.translation = None;
            },
        )]);

        assert!(!Artifact::Translations.ready(&page));
    }

    fn page_with_texts<const N: usize>(nodes: [Node; N]) -> Page {
        let mut page = Page::new("p1", 100, 100);
        for node in nodes {
            page.nodes.insert(node.id, node);
        }
        page
    }

    fn text_node(workflow: TextWorkflow, update: impl FnOnce(&mut TextData)) -> Node {
        let mut text = TextData {
            workflow,
            ..Default::default()
        };
        update(&mut text);
        Node {
            id: NodeId::new(),
            transform: Transform::default(),
            visible: true,
            kind: NodeKind::Text(text),
        }
    }
}
