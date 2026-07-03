use koharu_core::{FontFaceInfo, NamedFontPrediction};

#[derive(Debug, Clone)]
pub(super) struct FontCandidate {
    pub(super) font_id: String,
    pub(super) family_name: String,
    pub(super) post_script_name: String,
    pub(super) primary_category: String,
    pub(super) secondary_category: String,
}

pub(super) fn select_font_candidates(
    fonts: &[FontFaceInfo],
    primary_category: &str,
    secondary_category: &str,
    limit: usize,
) -> Vec<FontCandidate> {
    if limit == 0 {
        return Vec::new();
    }
    let all = fonts
        .iter()
        .filter(|font| font.cached && !font.post_script_name.trim().is_empty())
        .map(font_candidate)
        .collect::<Vec<_>>();
    let mut selected = Vec::new();
    push_matching(&mut selected, &all, limit, |candidate| {
        candidate.primary_category == primary_category
            && candidate.secondary_category == secondary_category
    });
    push_matching(&mut selected, &all, limit, |candidate| {
        candidate.primary_category == primary_category
    });
    if selected.is_empty() {
        push_matching(&mut selected, &all, limit, |_| true);
    }
    selected
}

pub(super) fn infer_font_category(font: &FontFaceInfo) -> (String, String) {
    let value = format!(
        "{} {} {}",
        font.family_name,
        font.post_script_name,
        font.category.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    if contains_any(&value, &["kai", "kaiti", "klee", "kaisho", "楷"]) {
        ("serif".to_string(), "kai".to_string())
    } else if contains_any(&value, &["mincho", "ming", "song", "serif", "宋", "明朝"]) {
        ("serif".to_string(), "mincho".to_string())
    } else if contains_any(&value, &["round", "rounded", "maru", "丸"]) {
        ("sans_serif".to_string(), "round".to_string())
    } else {
        ("sans_serif".to_string(), "gothic".to_string())
    }
}

pub(super) fn infer_prediction_secondary(font: &NamedFontPrediction) -> String {
    let name = font.name.to_lowercase();
    if font.serif {
        if contains_any(&name, &["kai", "kaiti", "klee", "楷"]) {
            "kai".to_string()
        } else {
            "mincho".to_string()
        }
    } else if contains_any(&name, &["round", "rounded", "maru", "丸"]) {
        "round".to_string()
    } else {
        "gothic".to_string()
    }
}

fn font_candidate(font: &FontFaceInfo) -> FontCandidate {
    let (primary, secondary) = infer_font_category(font);
    FontCandidate {
        font_id: font.post_script_name.clone(),
        family_name: font.family_name.clone(),
        post_script_name: font.post_script_name.clone(),
        primary_category: primary,
        secondary_category: secondary,
    }
}

fn push_matching(
    selected: &mut Vec<FontCandidate>,
    all: &[FontCandidate],
    limit: usize,
    predicate: impl Fn(&FontCandidate) -> bool,
) {
    for candidate in all.iter().filter(|candidate| predicate(candidate)) {
        if selected.len() >= limit {
            return;
        }
        if selected
            .iter()
            .any(|item| item.post_script_name == candidate.post_script_name)
        {
            continue;
        }
        selected.push(candidate.clone());
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}
