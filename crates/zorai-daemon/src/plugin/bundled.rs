use std::path::{Path, PathBuf};

pub const BUNDLED_SOURCE: &str = "bundled";
const SKIPPED_DIRS: &[&str] = &[
    "node_modules",
    "__pycache__",
    ".pytest_cache",
    ".git",
    ".python",
    ".venv",
    "tests",
];
const KEPT_ON_UPDATE: &[&str] = &[".python", ".venv"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BundleSyncReport {
    pub installed: Vec<String>,
    pub updated: Vec<String>,
}

pub fn bundled_plugins_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let home_bundle = dirs::home_dir().map(|home| home.join(".zorai").join("bundled-plugins"));
    bundled_plugins_dir_from(
        std::env::var_os("ZORAI_BUNDLED_PLUGINS_DIR").map(PathBuf::from),
        exe_dir.as_deref(),
        home_bundle,
    )
}

pub fn bundled_plugins_dir_from(
    configured: Option<PathBuf>,
    exe_dir: Option<&Path>,
    home_bundle: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        return configured.is_dir().then_some(configured);
    }
    let beside_binary = exe_dir.into_iter().flat_map(|dir| {
        [
            dir.join("..").join("plugins"),
            dir.join("..").join("..").join("plugins"),
        ]
    });
    beside_binary
        .chain(home_bundle)
        .find(|candidate| contains_plugins(candidate))
}

fn contains_plugins(dir: &Path) -> bool {
    dir.is_dir() && !find_bundled_plugins(dir).is_empty()
}

pub fn find_bundled_plugins(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(root, 0, &mut found);
    found.sort();
    found
}

fn collect(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if dir.join("plugin.json").is_file() {
        found.push(dir.to_path_buf());
        return;
    }
    if depth >= 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() && !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
            collect(&path, depth + 1, found);
        }
    }
}

pub fn replace_plugin_files(source: &Path, target: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        for entry in std::fs::read_dir(target)?.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if KEPT_ON_UPDATE.contains(&name.as_str()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }
    copy_tree(source, target)
}

fn copy_tree(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if path.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_str()) {
                continue;
            }
            copy_tree(&path, &target.join(&name))?;
        } else if path.is_file() {
            std::fs::copy(&path, target.join(&name))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "bundled_tests.rs"]
mod tests;
