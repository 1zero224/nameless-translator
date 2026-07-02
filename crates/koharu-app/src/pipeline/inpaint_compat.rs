//! Compatibility aliases for inpainter engine names used by other manga
//! lettering tools.
//!
//! Koharu keeps a native inventory-based engine registry. This module only
//! canonicalizes external names at API/config boundaries; unsupported external
//! engines still fail normal registry validation instead of pretending to work.

use std::borrow::Cow;

/// Resolve known BallonsTranslator-style inpainter ids to Koharu engine ids.
pub fn resolve_inpainter_alias(id: &str) -> Cow<'_, str> {
    let trimmed = id.trim();
    let normalized = trimmed
        .chars()
        .map(|c| match c {
            '_' | ' ' => '-',
            _ => c.to_ascii_lowercase(),
        })
        .collect::<String>();

    match normalized.as_str() {
        "lama" | "lama-manga" | "lama-large" | "lama-large-512px" | "bt-lama-large"
        | "lama-mpe" | "bt-lama-mpe" => Cow::Borrowed("lama-manga"),
        "aot" | "bt-aot" | "aot-inpainting" => Cow::Borrowed("aot-inpainting"),
        "flux2-klein" | "flux-2-klein" | "bt-flux2-klein" => Cow::Borrowed("flux2-klein"),
        _ => Cow::Borrowed(trimmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_balloonstranslator_lama_aliases() {
        assert_eq!(resolve_inpainter_alias("lama_large_512px"), "lama-manga");
        assert_eq!(resolve_inpainter_alias("bt_lama_mpe"), "lama-manga");
        assert_eq!(resolve_inpainter_alias(" LAMA_MPE "), "lama-manga");
    }

    #[test]
    fn maps_aot_aliases() {
        assert_eq!(resolve_inpainter_alias("aot"), "aot-inpainting");
        assert_eq!(resolve_inpainter_alias("bt_aot"), "aot-inpainting");
    }

    #[test]
    fn leaves_unsupported_external_engines_unresolved() {
        assert_eq!(resolve_inpainter_alias("opencv_tela"), "opencv_tela");
        assert_eq!(resolve_inpainter_alias("patchmatch"), "patchmatch");
        assert_eq!(
            resolve_inpainter_alias("gpt_image2_masked_edit"),
            "gpt_image2_masked_edit"
        );
    }
}
