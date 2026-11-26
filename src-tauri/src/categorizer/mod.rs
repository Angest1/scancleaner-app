use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use windows::core::PWSTR;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_Profile, FOLDERID_RoamingAppData,
    SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use crate::scanner::model::{NodeFlags, NodeId, NodeKind, OriginCategory, ScanTree, TypeCategory};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRulesFile {
    #[serde(default)]
    pub extension_overrides: HashMap<String, TypeCategory>,
    #[serde(default)]
    pub path_origin_overrides: Vec<PathOriginRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathOriginRule {
    pub pattern: String,
    pub origin: OriginCategory,
}

#[derive(Debug, Clone)]
pub struct RulesConfig {
    pub custom: CustomRulesFile,
    pub downloads_dir: Option<String>,
    pub local_appdata_dir: Option<String>,
    pub roaming_appdata_dir: Option<String>,
    pub user_profile_dir: Option<String>,
    pub temp_dir: Option<String>,
}

fn known_folder(id: &windows::core::GUID) -> Option<String> {
    unsafe {
        let ptr: PWSTR = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = ptr.to_string().ok();
        CoTaskMemFree(Some(ptr.as_ptr().cast()));
        s.map(|v| v.to_ascii_lowercase())
    }
}

pub fn user_rules_path() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata)
            .join("ScanCleaner")
            .join("rules.json")
    } else {
        PathBuf::from("rules.json")
    }
}

impl RulesConfig {
    pub fn load() -> Self {
        let path = user_rules_path();
        let custom = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    serde_json::from_str::<CustomRulesFile>(&content).unwrap_or_default()
                }
                Err(_) => CustomRulesFile::default(),
            }
        } else {
            CustomRulesFile::default()
        };

        let temp_dir = std::env::var("TEMP")
            .or_else(|_| std::env::var("TMP"))
            .ok()
            .map(|s| s.to_ascii_lowercase());

        Self {
            custom,
            downloads_dir: known_folder(&FOLDERID_Downloads),
            local_appdata_dir: known_folder(&FOLDERID_LocalAppData),
            roaming_appdata_dir: known_folder(&FOLDERID_RoamingAppData),
            user_profile_dir: known_folder(&FOLDERID_Profile),
            temp_dir,
        }
    }

    pub fn with_custom(custom: CustomRulesFile) -> Self {
        Self {
            custom,
            downloads_dir: known_folder(&FOLDERID_Downloads),
            local_appdata_dir: known_folder(&FOLDERID_LocalAppData),
            roaming_appdata_dir: known_folder(&FOLDERID_RoamingAppData),
            user_profile_dir: known_folder(&FOLDERID_Profile),
            temp_dir: std::env::var("TEMP").ok().map(|s| s.to_ascii_lowercase()),
        }
    }
}

pub fn classify_extension(ext_lower: &str, custom: &CustomRulesFile) -> TypeCategory {
    if let Some(&override_cat) = custom.extension_overrides.get(ext_lower) {
        return override_cat;
    }
    match ext_lower {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" => {
            TypeCategory::Video
        }
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tiff" | "tif"
        | "heic" | "raw" | "psd" | "avif" => TypeCategory::Image,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" => {
            TypeCategory::Audio
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "rtf" | "odt"
        | "ods" | "csv" | "epub" => TypeCategory::Documents,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "cab" => {
            TypeCategory::Archives
        }
        "exe" | "msi" | "msix" | "appx" | "bat" | "cmd" | "ps1" | "dll" | "sys" => {
            TypeCategory::Executables
        }
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp"
        | "cs" | "php" | "rb" | "html" | "css" | "scss" | "json" | "yaml" | "yml" | "toml"
        | "xml" | "sql" | "sh" | "lock" => TypeCategory::Code,
        _ => TypeCategory::Other,
    }
}

pub fn classify_origin_path(
    path_lower: &str,
    name_lower: &str,
    rules: &RulesConfig,
) -> OriginCategory {
    for custom_rule in &rules.custom.path_origin_overrides {
        let pat = custom_rule.pattern.to_ascii_lowercase();
        if !pat.is_empty() && path_lower.contains(&pat) {
            return custom_rule.origin;
        }
    }

    if path_lower.contains(r"\$recycle.bin") || path_lower.ends_with(r"\$recycle.bin") {
        return OriginCategory::RecycleBin;
    }

    if path_lower.contains(r"\windows\")
        || path_lower.ends_with(r"\windows")
        || path_lower.contains(r"\system volume information")
        || name_lower == "pagefile.sys"
        || name_lower == "hiberfil.sys"
        || name_lower == "swapfile.sys"
    {
        if path_lower.contains(r"\windows\temp") {
            return OriginCategory::Temp;
        }
        return OriginCategory::System;
    }

    if let Some(ref temp) = rules.temp_dir {
        if path_lower.starts_with(temp) {
            return OriginCategory::Temp;
        }
    }
    if path_lower.contains(r"\appdata\local\temp") || path_lower.contains(r"\windows\temp") {
        return OriginCategory::Temp;
    }

    // Browser caches
    if (path_lower.contains(r"\google\chrome\user data\")
        || path_lower.contains(r"\microsoft\edge\user data\")
        || path_lower.contains(r"\bravesoftware\brave-browser\user data\")
        || path_lower.contains(r"\mozilla\firefox\profiles\")
        || path_lower.contains(r"\opera software\"))
        && (path_lower.contains(r"\cache")
            || path_lower.contains(r"\code cache")
            || path_lower.contains(r"\gpucache")
            || path_lower.contains(r"\cache2"))
    {
        return OriginCategory::BrowserCache;
    }

    // Dev artifacts and package manager caches
    if matches!(
        name_lower,
        "node_modules" | ".venv" | "venv" | "target" | "__pycache__" | ".gradle"
    ) || path_lower.contains(r"\node_modules\")
        || path_lower.contains(r"\.venv\")
        || path_lower.contains(r"\npm-cache\")
        || path_lower.contains(r"\pnpm-store\")
        || path_lower.contains(r"\pip\cache\")
        || path_lower.contains(r"\composer\cache\")
        || path_lower.contains(r"\.cargo\registry\")
        || path_lower.contains(r"\vendor\")
    {
        return OriginCategory::DevArtifact;
    }

    if let Some(ref dl) = rules.downloads_dir {
        if path_lower.starts_with(dl) {
            return OriginCategory::Downloads;
        }
    }
    if path_lower.contains(r"\downloads\") || path_lower.ends_with(r"\downloads") {
        return OriginCategory::Downloads;
    }

    if path_lower.contains(r"\program files\")
        || path_lower.contains(r"\program files (x86)\")
        || path_lower.contains(r"\programdata\")
    {
        return OriginCategory::InstalledApps;
    }

    OriginCategory::UserOther
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBucket {
    pub key: String,
    pub label: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBreakdown {
    pub total_allocated_bytes: u64,
    pub by_type: Vec<CategoryBucket>,
    pub by_origin: Vec<CategoryBucket>,
}

pub fn classify_tree(tree: &mut ScanTree, rules: &RulesConfig) -> CategoryBreakdown {
    let mut type_alloc = [0u64; 8];
    let mut type_logical = [0u64; 8];
    let mut type_count = [0u64; 8];

    let mut origin_alloc = [0u64; 8];
    let mut origin_logical = [0u64; 8];
    let mut origin_count = [0u64; 8];

    let len = tree.nodes.len();
    let mut path_cache: Vec<String> = vec![String::new(); len];
    path_cache[0] = tree
        .root_path
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();

    for idx in 0..len {
        let node_id = idx as NodeId;
        let name_lower = tree.node_name(node_id).to_ascii_lowercase();
        let parent_opt = tree.nodes[idx].parent;
        let full_lower = if let Some(pid) = parent_opt {
            let parent_path = &path_cache[pid as usize];
            if parent_path.is_empty() {
                name_lower.clone()
            } else {
                format!(r"{parent_path}\{name_lower}")
            }
        } else {
            path_cache[0].clone()
        };

        if tree.nodes[idx].kind == NodeKind::Dir {
            path_cache[idx] = full_lower.clone();
        }

        let origin = classify_origin_path(&full_lower, &name_lower, rules);
        tree.nodes[idx].origin_cat = origin;

        if matches!(origin, OriginCategory::System) {
            tree.nodes[idx].flags |= NodeFlags::PROTECTED;
        }

        let is_file = matches!(
            tree.nodes[idx].kind,
            NodeKind::File | NodeKind::CloudPlaceholder
        );
        if is_file {
            let ext = Path::new(&name_lower)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let tcat = classify_extension(ext, &rules.custom);
            tree.nodes[idx].type_cat = tcat;

            let alloc = tree.nodes[idx].allocated_size;
            let logical = tree.nodes[idx].logical_size;

            type_alloc[tcat as usize] += alloc;
            type_logical[tcat as usize] += logical;
            type_count[tcat as usize] += 1;

            origin_alloc[origin as usize] += alloc;
            origin_logical[origin as usize] += logical;
            origin_count[origin as usize] += 1;
        }
    }

    let total_alloc = tree.stats.total_allocated_bytes.max(1);
    let mut by_type: Vec<CategoryBucket> = TypeCategory::ALL
        .iter()
        .map(|&cat| {
            let i = cat as usize;
            CategoryBucket {
                key: format!("{cat:?}").to_ascii_lowercase(),
                label: cat.label().to_string(),
                allocated_bytes: type_alloc[i],
                logical_bytes: type_logical[i],
                file_count: type_count[i],
                percentage: (type_alloc[i] as f64 / total_alloc as f64) * 100.0,
            }
        })
        .collect();
    by_type.sort_by_key(|a| std::cmp::Reverse(a.allocated_bytes));

    let mut by_origin: Vec<CategoryBucket> = OriginCategory::ALL
        .iter()
        .map(|&cat| {
            let i = cat as usize;
            CategoryBucket {
                key: format!("{cat:?}").to_ascii_lowercase(),
                label: cat.label().to_string(),
                allocated_bytes: origin_alloc[i],
                logical_bytes: origin_logical[i],
                file_count: origin_count[i],
                percentage: (origin_alloc[i] as f64 / total_alloc as f64) * 100.0,
            }
        })
        .collect();
    by_origin.sort_by_key(|a| std::cmp::Reverse(a.allocated_bytes));

    CategoryBreakdown {
        total_allocated_bytes: tree.stats.total_allocated_bytes,
        by_type,
        by_origin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_extensions_and_custom_override() {
        let mut overrides = HashMap::new();
        overrides.insert("xyz".to_string(), TypeCategory::Video);
        let custom = CustomRulesFile {
            extension_overrides: overrides,
            path_origin_overrides: vec![],
        };

        assert_eq!(classify_extension("mp4", &custom), TypeCategory::Video);
        assert_eq!(classify_extension("rs", &custom), TypeCategory::Code);
        assert_eq!(classify_extension("xyz", &custom), TypeCategory::Video);
        assert_eq!(
            classify_extension("unknownext", &custom),
            TypeCategory::Other
        );
    }

    #[test]
    fn classifies_origins_correctly() {
        let rules = RulesConfig::with_custom(CustomRulesFile {
            extension_overrides: HashMap::new(),
            path_origin_overrides: vec![PathOriginRule {
                pattern: r"\my_custom_cache".to_string(),
                origin: OriginCategory::Temp,
            }],
        });

        assert_eq!(
            classify_origin_path(r"c:\windows\winsxs\foo", "foo", &rules),
            OriginCategory::System
        );
        assert_eq!(
            classify_origin_path(r"c:\proj\node_modules\pkg\a.js", "a.js", &rules),
            OriginCategory::DevArtifact
        );
        assert_eq!(
            classify_origin_path(
                r"c:\users\u\appdata\local\google\chrome\user data\default\cache\cache_data\f_01",
                "f_01",
                &rules
            ),
            OriginCategory::BrowserCache
        );
        assert_eq!(
            classify_origin_path(r"d:\work\my_custom_cache\tmp.bin", "tmp.bin", &rules),
            OriginCategory::Temp
        );
    }
}
