//! Extract Tauri's embedded assets and expose them as an `AssetResolver` for
//! the axum server to fall back on (i.e. serve the bundled UI).

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use koharu_rpc::server::AssetResolver;

/// Build an `AssetResolver` from the Tauri context. Replaces the context's
/// embedded assets with an empty set so Tauri's own webview points at the
/// axum-served URL instead.
pub fn from_context<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> AssetResolver {
    struct Empty;
    impl<R: tauri::Runtime> tauri::Assets<R> for Empty {
        fn get(&self, _: &tauri::utils::assets::AssetKey) -> Option<std::borrow::Cow<'_, [u8]>> {
            None
        }
        fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(
            &self,
            _: &tauri::utils::assets::AssetKey,
        ) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }

    let assets: Arc<dyn tauri::Assets<R>> = context.set_assets(Box::new(Empty)).into();
    let filesystem_dist = filesystem_dist_dir();

    Arc::new(move |path: &str| {
        let key = tauri::utils::assets::AssetKey::from(path);
        if let Some(bytes) = assets.get(&key).map(|bytes| bytes.into_owned()) {
            let mime = tauri::utils::mime_type::MimeType::parse(&bytes, path);
            return Some((bytes, mime));
        }
        filesystem_dist
            .as_deref()
            .and_then(|dist| filesystem_asset(dist, path))
    })
}

fn filesystem_dist_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("KOHARU_UI_DIST_DIR")
        && !path.trim().is_empty()
    {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("ui")
        .join("out");
    path.is_dir().then_some(path)
}

fn filesystem_asset(root: &Path, request_path: &str) -> Option<(Vec<u8>, String)> {
    let request_path = request_path.trim_start_matches('/');
    let request_path = if request_path.is_empty() {
        "index.html"
    } else {
        request_path
    };

    let mut safe_path = PathBuf::new();
    for component in Path::new(request_path).components() {
        match component {
            Component::Normal(part) => safe_path.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if safe_path.as_os_str().is_empty() {
        safe_path.push("index.html");
    }

    let full_path = root.join(&safe_path);
    if !full_path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&full_path).ok()?;
    let path_for_mime = safe_path.to_string_lossy();
    let mime = tauri::utils::mime_type::MimeType::parse(&bytes, &path_for_mime);
    Some((bytes, mime))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::filesystem_asset;

    fn temp_dist() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("koharu-asset-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn filesystem_asset_resolves_empty_path_to_index() {
        let dir = temp_dist();
        fs::write(dir.join("index.html"), b"<html>Koharu</html>").unwrap();

        let (bytes, mime) = filesystem_asset(&dir, "").expect("index asset");

        assert_eq!(bytes, b"<html>Koharu</html>");
        assert_eq!(mime, "text/html");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn filesystem_asset_rejects_parent_traversal() {
        let dir = temp_dist();
        fs::write(dir.join("index.html"), b"ok").unwrap();

        assert!(filesystem_asset(&dir, "../config.toml").is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
