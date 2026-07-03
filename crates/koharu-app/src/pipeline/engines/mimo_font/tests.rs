use image::{DynamicImage, Rgba, RgbaImage};
use koharu_core::{FontFaceInfo, FontSource};
use std::path::{Path, PathBuf};

use crate::pipeline::{Artifact, Registry};

use super::client::{MimoFontClient, MimoFontConfig};
use super::parsing::{category_prompt, parse_category_response, parse_selection_response};
use super::taxonomy::{FontCandidate, infer_font_category, select_font_candidates};
use super::visuals::encode_png;

fn font(post_script_name: &str, family_name: &str, category: Option<&str>) -> FontFaceInfo {
    FontFaceInfo {
        family_name: family_name.to_string(),
        post_script_name: post_script_name.to_string(),
        source: FontSource::System,
        category: category.map(str::to_string),
        cached: true,
    }
}

#[test]
fn parse_category_response_accepts_markdown_json() {
    let parsed = parse_category_response(
        "```json\n{\"primary_category\":\"sans_serif\",\"secondary_category\":\"round\",\"confidence\":0.82,\"reasoning_summary\":\"rounded strokes\"}\n```",
    )
    .expect("parse category response");

    assert_eq!(parsed.primary_category, "sans_serif");
    assert_eq!(parsed.secondary_category, "round");
    assert_eq!(parsed.confidence, Some(0.82));
    assert_eq!(parsed.reasoning_summary.as_deref(), Some("rounded strokes"));
}

#[test]
fn parse_selection_response_rejects_unknown_candidate() {
    let candidates = vec![FontCandidate {
        font_id: "font-a".to_string(),
        family_name: "A".to_string(),
        post_script_name: "FontA".to_string(),
        primary_category: "sans_serif".to_string(),
        secondary_category: "gothic".to_string(),
    }];

    let err = parse_selection_response(
        "{\"selected_font_id\":\"font-b\",\"confidence\":0.7,\"reasoning_summary\":\"close\"}",
        &candidates,
    )
    .expect_err("unknown candidate should fail");
    assert!(
        err.to_string()
            .contains("selected font is not in candidates")
    );
}

#[test]
fn infer_font_category_maps_goal_taxonomy() {
    assert_eq!(
        infer_font_category(&font("YuMincho-Regular", "Yu Mincho", Some("serif"))),
        ("serif".to_string(), "mincho".to_string())
    );
    assert_eq!(
        infer_font_category(&font("KoharuKaiti", "Koharu Kaiti", None)),
        ("serif".to_string(), "kai".to_string())
    );
    assert_eq!(
        infer_font_category(&font("RoundedGothic", "Rounded Gothic", None)),
        ("sans_serif".to_string(), "round".to_string())
    );
    assert_eq!(
        infer_font_category(&font("YuGothic-Regular", "Yu Gothic", None)),
        ("sans_serif".to_string(), "gothic".to_string())
    );
}

#[test]
fn select_candidates_prefers_secondary_category_then_falls_back() {
    let fonts = vec![
        font("YuGothic-Regular", "Yu Gothic", None),
        font("YuMincho-Regular", "Yu Mincho", Some("serif")),
        font("KoharuKaiti", "Koharu Kaiti", None),
    ];

    let kai = select_font_candidates(&fonts, "serif", "kai", 4);
    assert_eq!(
        kai.iter()
            .map(|candidate| candidate.post_script_name.as_str())
            .collect::<Vec<_>>(),
        vec!["KoharuKaiti", "YuMincho-Regular"]
    );

    let fallback = select_font_candidates(&fonts, "sans_serif", "round", 2);
    assert_eq!(fallback[0].post_script_name, "YuGothic-Regular");
}

#[test]
fn registers_mimo_font_engine_as_font_prediction_provider() {
    let info = Registry::find("mimo-font-selection").expect("engine info");

    assert_eq!(info.name, "MIMO Vision Font Selection");
    assert!(info.needs.contains(&Artifact::SourceImage));
    assert!(info.needs.contains(&Artifact::TextBoxes));
    assert!(!info.needs.contains(&Artifact::Translations));
    assert!(info.produces.contains(&Artifact::FontPredictions));
}

#[tokio::test]
#[ignore = "requires MIMO_API_KEY and spends real vision API credits"]
async fn real_mimo_font_category_smoke_writes_artifacts() -> anyhow::Result<()> {
    let client = MimoFontClient::new(MimoFontConfig::from_env_or_config()?)?;
    let source = synthetic_gothic_source();
    let source_png = encode_png(&source)?;
    let out_dir = smoke_output_dir()?;

    std::fs::create_dir_all(&out_dir)?;
    write_request_artifacts(&out_dir, &client, &source, &source_png)?;
    let raw = client
        .analyze_image(
            &source_png,
            category_prompt(),
            "You classify manga lettering fonts. Return compact JSON only.",
        )
        .await?;
    write_response_artifacts(&out_dir, &raw)?;
    eprintln!("smoke artifacts: {}", out_dir.display());
    Ok(())
}

fn synthetic_gothic_source() -> DynamicImage {
    let mut source = RgbaImage::from_pixel(256, 192, Rgba([250, 250, 250, 255]));
    for y in 50..142 {
        for x in [70..88, 118..136, 166..184] {
            for px in x {
                source.put_pixel(px, y, Rgba([0, 0, 0, 255]));
            }
        }
    }
    for y in [50..68, 124..142] {
        for py in y {
            for x in 70..184 {
                source.put_pixel(x, py, Rgba([0, 0, 0, 255]));
            }
        }
    }
    DynamicImage::ImageRgba8(source)
}

fn write_request_artifacts(
    out_dir: &Path,
    client: &MimoFontClient,
    source: &DynamicImage,
    source_png: &[u8],
) -> anyhow::Result<()> {
    std::fs::write(out_dir.join("source.png"), source_png)?;
    std::fs::write(
        out_dir.join("request-summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "endpoint": client.endpoint(),
            "model": client.model(),
            "prompt": category_prompt(),
            "image": {"width": source.width(), "height": source.height(), "format": "png"},
            "secretPolicy": "API key loaded from environment/config and intentionally not written"
        }))?,
    )?;
    Ok(())
}

fn write_response_artifacts(out_dir: &Path, raw: &str) -> anyhow::Result<()> {
    let parsed = parse_category_response(raw)?;
    std::fs::write(out_dir.join("raw-response.txt"), raw)?;
    std::fs::write(
        out_dir.join("output-summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "primaryCategory": parsed.primary_category,
            "secondaryCategory": parsed.secondary_category,
            "confidence": parsed.confidence,
            "reasoningSummary": parsed.reasoning_summary
        }))?,
    )?;
    Ok(())
}

fn smoke_output_dir() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("KOHARU_MIMO_FONT_SMOKE_DIR") {
        return Ok(PathBuf::from(path));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    Ok(PathBuf::from(".tmp")
        .join("mimo-font-smoke")
        .join(stamp.to_string()))
}
