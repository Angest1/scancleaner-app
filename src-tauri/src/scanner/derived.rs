use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::categorizer::{
    classify_extension, classify_origin_path, CategoryBreakdown, CategoryBucket, RulesConfig,
};
use crate::dev_cleaner::blacklist::matches_simple_glob;
use crate::dev_cleaner::resolver::expand_env_vars;
use crate::dev_cleaner::schema::{DevRulesCatalog, RuleKind, SafetyLevel};
use crate::protection::{CompiledFastProtection, ProtectionEngine};

use super::model::{
    NodeFlags, NodeId, NodeKind, OriginCategory, ScanTree, TypeCategory, NONE_NODE,
};

pub const TOP_INDEX_LIMIT: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevArtifactIndexEntry {
    pub node_id: NodeId,
    pub project_dir_id: NodeId,
    pub rule_id: String,
    pub pattern: String,
    pub manifest_found: String,
    pub size_bytes: u64,
    pub file_count: u32,
    pub last_write_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalCacheIndexEntry {
    pub node_id: NodeId,
    pub cache_key: String,
    pub label: String,
    pub category: String,
    pub rule_id: Option<String>,
    pub size_bytes: u64,
    pub file_count: u32,
    pub last_write_ts: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedIndexes {
    pub type_alloc: [u64; 8],
    pub type_logical: [u64; 8],
    pub type_count: [u64; 8],
    pub origin_alloc: [u64; 8],
    pub origin_logical: [u64; 8],
    pub origin_count: [u64; 8],
    pub top_files: Vec<NodeId>,
    pub top_dirs: Vec<NodeId>,
    pub dev_artifacts: Vec<DevArtifactIndexEntry>,
    pub global_caches: Vec<GlobalCacheIndexEntry>,
    pub pending_trash_nodes: Vec<NodeId>,
}

impl DerivedIndexes {
    pub fn to_category_breakdown(&self, total_allocated_bytes: u64) -> CategoryBreakdown {
        let denom = total_allocated_bytes.max(1) as f64;
        let mut by_type: Vec<CategoryBucket> = TypeCategory::ALL
            .iter()
            .map(|&cat| {
                let i = cat as usize;
                CategoryBucket {
                    key: format!("{cat:?}").to_ascii_lowercase(),
                    label: cat.label().to_string(),
                    allocated_bytes: self.type_alloc[i],
                    logical_bytes: self.type_logical[i],
                    file_count: self.type_count[i],
                    percentage: (self.type_alloc[i] as f64 / denom) * 100.0,
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
                    allocated_bytes: self.origin_alloc[i],
                    logical_bytes: self.origin_logical[i],
                    file_count: self.origin_count[i],
                    percentage: (self.origin_alloc[i] as f64 / denom) * 100.0,
                }
            })
            .collect();
        by_origin.sort_by_key(|a| std::cmp::Reverse(a.allocated_bytes));

        CategoryBreakdown {
            total_allocated_bytes,
            by_type,
            by_origin,
        }
    }

    pub fn apply_file_delta(
        &mut self,
        type_cat: TypeCategory,
        origin_cat: OriginCategory,
        delta_alloc: i64,
        delta_logical: i64,
        delta_count: i64,
    ) {
        let ti = type_cat as usize;
        let oi = origin_cat as usize;
        self.type_alloc[ti] = apply_signed(self.type_alloc[ti], delta_alloc);
        self.type_logical[ti] = apply_signed(self.type_logical[ti], delta_logical);
        self.type_count[ti] = apply_signed(self.type_count[ti], delta_count);
        self.origin_alloc[oi] = apply_signed(self.origin_alloc[oi], delta_alloc);
        self.origin_logical[oi] = apply_signed(self.origin_logical[oi], delta_logical);
        self.origin_count[oi] = apply_signed(self.origin_count[oi], delta_count);
    }
}

fn apply_signed(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}

/// Precompiled matcher context used during the single-pass scan & subtree incremental updates
/// so we never allocate per-file rule structures or re-parse JSON inside tight loops.
pub struct DerivedBuildContext {
    pub rules_config: RulesConfig,
    pub protection: ProtectionEngine,
    pub fast_protection: CompiledFastProtection,
    pub dev_artifact_rules: Vec<CompiledArtifactRule>,
    pub known_global_caches: HashMap<String, KnownGlobalCacheSpec>,
    pub stop_artifact_names: HashSet<String>,
}

#[derive(Clone)]
pub struct CompiledArtifactRule {
    pub rule_id: String,
    pub folder_patterns: Vec<String>,
    pub single_segment_patterns: HashSet<String>,
    pub required_markers: Vec<String>,
}

#[derive(Clone)]
pub struct KnownGlobalCacheSpec {
    pub cache_key: String,
    pub label: String,
    pub category: String,
    pub rule_id: Option<String>,
}

impl DerivedBuildContext {
    pub fn load_default() -> Self {
        let rules_config = RulesConfig::load();
        let protection = ProtectionEngine::load_default();
        let catalog = DevRulesCatalog::load_embedded().ok();
        Self::new(rules_config, protection, catalog.as_ref())
    }

    pub fn new(
        rules_config: RulesConfig,
        protection: ProtectionEngine,
        catalog: Option<&DevRulesCatalog>,
    ) -> Self {
        let mut dev_artifact_rules = Vec::new();
        let mut stop_artifact_names = HashSet::new();
        let mut known_global_caches = HashMap::new();

        // Standard Windows / browser global caches for instant CleanerView
        if let Ok(temp) = std::env::var("TEMP") {
            insert_cache(
                &mut known_global_caches,
                &temp,
                "temp:UserTemp",
                "Archivos temporales del usuario (%TEMP%)",
                "user_temp",
                Some("sys-user-temp"),
            );
        }
        let win_temp = std::env::var("SystemRoot")
            .map(|s| format!(r"{s}\Temp"))
            .unwrap_or_else(|_| r"C:\Windows\Temp".to_string());
        insert_cache(
            &mut known_global_caches,
            &win_temp,
            "temp:SystemTemp",
            "Archivos temporales del sistema (C:\\Windows\\Temp)",
            "system_temp",
            Some("sys-win-temp"),
        );

        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let browsers = [
                ("Google Chrome", r"Google\Chrome\User Data\Default"),
                ("Microsoft Edge", r"Microsoft\Edge\User Data\Default"),
                ("Brave", r"BraveSoftware\Brave-Browser\User Data\Default"),
            ];
            for (name, rel) in browsers {
                for sub in [r"Cache\Cache_Data", "Code Cache", "GPUCache"] {
                    let full = format!(r"{local}\{rel}\{sub}");
                    insert_cache(
                        &mut known_global_caches,
                        &full,
                        &format!("browser:{name}:{sub}"),
                        &format!("{name} — {sub}"),
                        "browser_cache",
                        None,
                    );
                }
            }
            for (label, rel) in [
                ("Caché de npm", "npm-cache"),
                ("Caché de pip", r"pip\Cache"),
                ("Caché de Composer", r"Composer\cache"),
                ("Store de pnpm", "pnpm-store"),
            ] {
                let full = format!(r"{local}\{rel}");
                insert_cache(
                    &mut known_global_caches,
                    &full,
                    &format!("devcache:{label}"),
                    label,
                    "dev_artifact",
                    None,
                );
            }
        }

        if let Some(cat) = catalog {
            for rule in &cat.rules {
                if rule.nivel == SafetyLevel::Nunca {
                    continue;
                }
                if rule.tipo == RuleKind::ArtefactoProyecto {
                    let mut single_seg = HashSet::new();
                    for pat in &rule.ubicacion.folder_patterns {
                        let low = pat.to_ascii_lowercase();
                        if !low.contains('\\') && !low.contains('/') {
                            stop_artifact_names.insert(low.clone());
                            single_seg.insert(low);
                        }
                    }
                    dev_artifact_rules.push(CompiledArtifactRule {
                        rule_id: rule.id.clone(),
                        folder_patterns: rule
                            .ubicacion
                            .folder_patterns
                            .iter()
                            .map(|p| p.to_ascii_lowercase())
                            .collect(),
                        single_segment_patterns: single_seg,
                        required_markers: rule
                            .deteccion
                            .required_markers
                            .iter()
                            .map(|m| m.to_ascii_lowercase())
                            .collect(),
                    });
                } else if rule.ubicacion.special_detector.is_none()
                    || rule.ubicacion.special_detector.as_deref() == Some("dir_contents")
                {
                    // CRITICAL: Rules with a specialized detector (e.g. `jetbrains_caches`,
                    // `android_studio_old`, `gradle_old_dists`, `vscode_orphan_ws`) list a PARENT
                    // directory in `env_fallback` (such as `%LOCALAPPDATA%\Google`). Never register
                    // those parent roots directly as deletable global caches!
                    for (idx, tmpl) in rule.ubicacion.env_fallback.iter().enumerate() {
                        if let Some(expanded) = expand_env_vars(tmpl) {
                            let s = expanded.to_string_lossy().into_owned();
                            let key = format!("rule:{}:{idx}", rule.id);
                            insert_cache(
                                &mut known_global_caches,
                                &s,
                                &key,
                                &rule.nombre,
                                &format!("dev:{:?}", rule.categoria).to_ascii_lowercase(),
                                Some(&rule.id),
                            );
                        }
                    }
                }
            }
        }

        let fast_protection = protection.compile_fast_matcher();
        Self {
            rules_config,
            protection,
            fast_protection,
            dev_artifact_rules,
            known_global_caches,
            stop_artifact_names,
        }
    }
}

fn insert_cache(
    map: &mut HashMap<String, KnownGlobalCacheSpec>,
    raw_path: &str,
    cache_key: &str,
    label: &str,
    category: &str,
    rule_id: Option<&str>,
) {
    let norm = raw_path
        .trim()
        .trim_end_matches('\\')
        .replace('/', "\\")
        .to_ascii_lowercase();
    if norm.is_empty() {
        return;
    }
    map.entry(norm).or_insert_with(|| KnownGlobalCacheSpec {
        cache_key: cache_key.to_string(),
        label: label.to_string(),
        category: category.to_string(),
        rule_id: rule_id.map(ToString::to_string),
    });
}

/// Finalizes a freshly built `ScanTree` in a single bottom-up aggregation + top-down/inline
/// derived index pass, populating categories, Top-N files/dirs, Dev artifacts, Global caches,
/// pending trash markers, and `isProtected()` subtree flags without per-file heap objects.
pub fn finalize_tree_and_derived(tree: &mut ScanTree, ctx: &DerivedBuildContext) {
    // 1. Reset derived & compute categories/protection/caches in a single DFS stack pass
    //    (reusing a single scratch path buffer instead of allocating `Vec<String>` of N strings!)
    tree.derived = DerivedIndexes::default();
    let len = tree.len();
    if len == 0 {
        return;
    }

    // First ensure bottom-up sizes/counts are aggregated.
    // Note: MFT trees may not have parent-before-child index order, so we build a topological
    // DFS visit order starting from root (0) once and reuse it for both top-down classification
    // and reverse bottom-up aggregation!
    let mut dfs_order: Vec<NodeId> = Vec::with_capacity(len);
    let mut path_buf = tree
        .root_path
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();

    // Iterative DFS with explicit enter/exit markers so `path_buf` stays O(depth) memory!
    enum StackOp {
        Enter(NodeId, usize), // (node_id, prev_path_buf_len)
        Leave(usize),
    }

    let mut stack: Vec<StackOp> = Vec::with_capacity(256);
    stack.push(StackOp::Enter(0, path_buf.len()));

    let mut candidate_dev_dirs: Vec<(NodeId, NodeId, String, String, String)> = Vec::new();
    let mut candidate_cache_dirs: Vec<(NodeId, KnownGlobalCacheSpec)> = Vec::new();
    let mut pending_trash: Vec<NodeId> = Vec::new();

    while let Some(op) = stack.pop() {
        match op {
            StackOp::Leave(prev_len) => {
                path_buf.truncate(prev_len);
            }
            StackOp::Enter(nid, prev_len) => {
                let idx = nid as usize;
                let flags = tree.flags_of(nid);
                if flags.contains(NodeFlags::TOMBSTONE) {
                    continue;
                }

                if nid != 0 {
                    let name = tree.node_name(nid);
                    path_buf.push('\\');
                    let start = path_buf.len();
                    path_buf.push_str(name);
                    // Lowercase only the newly appended segment in-place for ASCII fast path
                    if path_buf[start..].is_ascii() {
                        path_buf[start..].make_ascii_lowercase();
                    } else {
                        let lowered = path_buf[start..].to_lowercase();
                        path_buf.truncate(start);
                        path_buf.push_str(&lowered);
                    }
                    stack.push(StackOp::Leave(prev_len));
                }

                dfs_order.push(nid);

                let kind = tree.kind_of(nid);
                let name_slice = if nid == 0 {
                    path_buf.as_str()
                } else {
                    path_buf.rsplit('\\').next().unwrap_or("")
                };

                // Track pending trash folder `.scancleaner-trash`
                if kind == NodeKind::Dir && name_slice == ".scancleaner-trash" {
                    tree.add_flag(nid, NodeFlags::PENDING_TRASH);
                    pending_trash.push(nid);
                }

                // Origin classification
                let origin = classify_origin_path(&path_buf, name_slice, &ctx.rules_config);
                tree.set_origin_cat(nid, origin);

                // Zero-allocation precompiled protection check (propagates down if parent protected)
                let parent_id = tree.parent_of(nid);
                let parent_prot = parent_id != NONE_NODE
                    && tree.flags_of(parent_id).contains(NodeFlags::PROTECTED);
                let is_prot = if parent_prot && !matches!(origin, OriginCategory::Temp) {
                    true
                } else {
                    ctx.fast_protection
                        .is_protected_lower(&path_buf, name_slice)
                };
                if is_prot {
                    tree.add_flag(nid, NodeFlags::PROTECTED);
                }

                if kind == NodeKind::Dir {
                    // Check global caches map in O(1)
                    if let Some(spec) = ctx.known_global_caches.get(&path_buf) {
                        candidate_cache_dirs.push((nid, spec.clone()));
                    } else if path_buf.contains(r"\mozilla\firefox\profiles\")
                        && path_buf.ends_with(r"\cache2\entries")
                    {
                        candidate_cache_dirs.push((
                            nid,
                            KnownGlobalCacheSpec {
                                cache_key: format!("browser:firefox:{nid}"),
                                label: "Mozilla Firefox — Caché".to_string(),
                                category: "browser_cache".to_string(),
                                rule_id: None,
                            },
                        ));
                    }

                    // Check Dev artifact patterns against sibling manifests (no extra pass!)
                    if nid != 0
                        && parent_id != NONE_NODE
                        && ctx.stop_artifact_names.contains(name_slice)
                    {
                        // Avoid nested artifacts like node_modules/pkg/node_modules
                        let parent_name = tree.node_name(parent_id);
                        let parent_is_artifact = ctx
                            .stop_artifact_names
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(parent_name));
                        if !parent_is_artifact {
                            if let Some((rule_id, manifest)) =
                                match_dev_artifact_in_parent(tree, parent_id, name_slice, ctx)
                            {
                                candidate_dev_dirs.push((
                                    nid,
                                    parent_id,
                                    rule_id,
                                    name_slice.to_string(),
                                    manifest,
                                ));
                            }
                        }
                    }

                    // Push children onto DFS stack
                    let cur_len = path_buf.len();
                    let mut child = tree.first_child_of(nid);
                    while child != NONE_NODE {
                        stack.push(StackOp::Enter(child, cur_len));
                        child = tree.next_sibling_of(child);
                    }
                } else if matches!(kind, NodeKind::File | NodeKind::CloudPlaceholder) {
                    let ext = name_slice.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
                    let tcat = classify_extension(ext, &ctx.rules_config.custom);
                    tree.set_type_cat(nid, tcat);

                    if !flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
                        let alloc = tree.allocated_size[idx];
                        let logical = tree.logical_size[idx];
                        tree.derived.type_alloc[tcat as usize] += alloc;
                        tree.derived.type_logical[tcat as usize] += logical;
                        tree.derived.type_count[tcat as usize] += 1;
                        tree.derived.origin_alloc[origin as usize] += alloc;
                        tree.derived.origin_logical[origin as usize] += logical;
                        tree.derived.origin_count[origin as usize] += 1;
                    }
                }
            }
        }
    }

    // 2. Single reverse bottom-up pass over `dfs_order`: resets dir aggregates then rolls up!
    for &nid in &dfs_order {
        if tree.kind_of(nid) == NodeKind::Dir {
            let idx = nid as usize;
            tree.allocated_size[idx] = 0;
            tree.logical_size[idx] = 0;
            tree.file_count[idx] = 0;
        }
    }

    let mut total_files = 0u64;
    let mut total_dirs = 0u64;
    let mut top_files: Vec<(u64, NodeId)> = Vec::new();
    let mut top_dirs: Vec<(u64, NodeId)> = Vec::new();

    for &nid in dfs_order.iter().rev() {
        let idx = nid as usize;
        let flags = tree.flags_of(nid);
        if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
            continue;
        }

        let kind = tree.kind_of(nid);
        let alloc = tree.allocated_size[idx];
        let logical = tree.logical_size[idx];
        let fcount = tree.file_count[idx];
        let ts = tree.last_write_ts[idx];

        match kind {
            NodeKind::Dir => {
                total_dirs += 1;
                if nid != 0 && alloc > 0 {
                    insert_top_bounded(&mut top_dirs, (alloc, nid), TOP_INDEX_LIMIT);
                }
            }
            NodeKind::File | NodeKind::CloudPlaceholder => {
                total_files += 1;
                if alloc > 0 {
                    insert_top_bounded(&mut top_files, (alloc, nid), TOP_INDEX_LIMIT);
                }
            }
            NodeKind::Symlink | NodeKind::Junction => {}
        }

        let pid = tree.parent[idx];
        if pid != NONE_NODE {
            let pidx = pid as usize;
            tree.logical_size[pidx] = tree.logical_size[pidx].saturating_add(logical);
            tree.allocated_size[pidx] = tree.allocated_size[pidx].saturating_add(alloc);
            tree.file_count[pidx] = tree.file_count[pidx].saturating_add(fcount);
            if ts > tree.last_write_ts[pidx] {
                tree.last_write_ts[pidx] = ts;
            }
            if flags.contains(NodeFlags::PROTECTED)
                || flags.contains(NodeFlags::PROTECTED_DESCENDANT)
            {
                tree.add_flag(pid, NodeFlags::PROTECTED_DESCENDANT);
            }
        }
    }

    top_files.sort_by_key(|a| std::cmp::Reverse(a.0));
    top_dirs.sort_by_key(|a| std::cmp::Reverse(a.0));
    tree.derived.top_files = top_files.into_iter().map(|(_, id)| id).collect();
    tree.derived.top_dirs = top_dirs.into_iter().map(|(_, id)| id).collect();

    // Populate dev_artifacts & global_caches with finalized sizes
    for (nid, proj_id, rule_id, pattern, manifest) in candidate_dev_dirs {
        let idx = nid as usize;
        let sz = tree.allocated_size[idx];
        if sz == 0 && tree.file_count[idx] == 0 {
            continue;
        }
        tree.derived.dev_artifacts.push(DevArtifactIndexEntry {
            node_id: nid,
            project_dir_id: proj_id,
            rule_id,
            pattern,
            manifest_found: manifest,
            size_bytes: sz,
            file_count: tree.file_count[idx],
            last_write_ts: tree.last_write_ts[idx] as i64,
        });
    }
    tree.derived
        .dev_artifacts
        .sort_by_key(|e| std::cmp::Reverse(e.size_bytes));

    for (nid, spec) in candidate_cache_dirs {
        let idx = nid as usize;
        tree.derived.global_caches.push(GlobalCacheIndexEntry {
            node_id: nid,
            cache_key: spec.cache_key,
            label: spec.label,
            category: spec.category,
            rule_id: spec.rule_id,
            size_bytes: tree.allocated_size[idx],
            file_count: tree.file_count[idx],
            last_write_ts: tree.last_write_ts[idx] as i64,
        });
    }
    tree.derived
        .global_caches
        .sort_by_key(|e| std::cmp::Reverse(e.size_bytes));

    tree.derived.pending_trash_nodes = pending_trash;

    tree.stats.total_logical_bytes = tree.logical_size[0];
    tree.stats.total_allocated_bytes = tree.allocated_size[0];
    tree.stats.total_files = total_files;
    tree.stats.total_dirs = total_dirs;
    tree.stats.access_denied_count = tree.access_denied.len() as u64;
}

/// Incrementally updates derived indexes for a modified subtree (used by USN deltas,
/// optimistic deletion, and the Section 9 reconciliation queue) without rescanning the volume.
pub fn refresh_derived_for_subtree(
    tree: &mut ScanTree,
    subtree_root: NodeId,
    ctx: &DerivedBuildContext,
) {
    if (subtree_root as usize) >= tree.len() {
        return;
    }

    // Collect all node IDs currently inside the subtree to prune stale entries from derived vectors
    let mut subtree_set: HashSet<NodeId> = HashSet::new();
    let mut stack = vec![subtree_root];
    while let Some(nid) = stack.pop() {
        subtree_set.insert(nid);
        let mut child = tree.first_child_of(nid);
        while child != NONE_NODE {
            stack.push(child);
            child = tree.next_sibling_of(child);
        }
    }

    tree.derived
        .dev_artifacts
        .retain(|e| !subtree_set.contains(&e.node_id));
    tree.derived
        .global_caches
        .retain(|e| !subtree_set.contains(&e.node_id));
    tree.derived
        .pending_trash_nodes
        .retain(|id| !subtree_set.contains(id));

    // Re-evaluate dev artifacts, global caches, categories & protection inside subtree
    let base_path = tree
        .reconstruct_path(subtree_root)
        .to_string_lossy()
        .trim_end_matches('\\')
        .to_ascii_lowercase();

    let mut visit_stack: Vec<(NodeId, String)> = vec![(subtree_root, base_path)];
    while let Some((nid, full_low)) = visit_stack.pop() {
        let flags = tree.flags_of(nid);
        if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
            continue;
        }
        let kind = tree.kind_of(nid);
        let name_low = full_low.rsplit('\\').next().unwrap_or("");
        let origin = classify_origin_path(&full_low, name_low, &ctx.rules_config);
        tree.set_origin_cat(nid, origin);

        let is_prot = ctx.fast_protection.is_protected_lower(&full_low, name_low);
        if is_prot {
            tree.add_flag(nid, NodeFlags::PROTECTED);
        } else {
            tree.remove_flag(nid, NodeFlags::PROTECTED);
        }

        if kind == NodeKind::Dir {
            if name_low == ".scancleaner-trash" {
                tree.add_flag(nid, NodeFlags::PENDING_TRASH);
                tree.derived.pending_trash_nodes.push(nid);
            }
            if let Some(spec) = ctx.known_global_caches.get(&full_low) {
                let idx = nid as usize;
                tree.derived.global_caches.push(GlobalCacheIndexEntry {
                    node_id: nid,
                    cache_key: spec.cache_key.clone(),
                    label: spec.label.clone(),
                    category: spec.category.clone(),
                    rule_id: spec.rule_id.clone(),
                    size_bytes: tree.allocated_size[idx],
                    file_count: tree.file_count[idx],
                    last_write_ts: tree.last_write_ts[idx] as i64,
                });
            }
            let parent_id = tree.parent_of(nid);
            if parent_id != NONE_NODE && ctx.stop_artifact_names.contains(name_low) {
                if let Some((rule_id, manifest)) =
                    match_dev_artifact_in_parent(tree, parent_id, name_low, ctx)
                {
                    let idx = nid as usize;
                    if tree.allocated_size[idx] > 0 || tree.file_count[idx] > 0 {
                        tree.derived.dev_artifacts.push(DevArtifactIndexEntry {
                            node_id: nid,
                            project_dir_id: parent_id,
                            rule_id,
                            pattern: name_low.to_string(),
                            manifest_found: manifest,
                            size_bytes: tree.allocated_size[idx],
                            file_count: tree.file_count[idx],
                            last_write_ts: tree.last_write_ts[idx] as i64,
                        });
                    }
                }
            }

            let mut child = tree.first_child_of(nid);
            while child != NONE_NODE {
                let cname = tree.node_name(child).to_ascii_lowercase();
                let child_path = if full_low.ends_with('\\') {
                    format!("{full_low}{cname}")
                } else {
                    format!(r"{full_low}\{cname}")
                };
                visit_stack.push((child, child_path));
                child = tree.next_sibling_of(child);
            }
        } else if matches!(kind, NodeKind::File | NodeKind::CloudPlaceholder) {
            let ext = name_low.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            let tcat = classify_extension(ext, &ctx.rules_config.custom);
            tree.set_type_cat(nid, tcat);
        }
    }

    // Update top_files & top_dirs incrementally (prune deleted/hidden & merge subtree top items)
    let flags_col = &tree.flags;
    let total_nodes = tree.parent.len();
    let hidden_mask = (NodeFlags::TOMBSTONE | NodeFlags::OPTIMISTIC_HIDDEN).bits();
    tree.derived.top_files.retain(|&id| {
        let i = id as usize;
        i < total_nodes && (flags_col[i] & hidden_mask) == 0 && !subtree_set.contains(&id)
    });
    tree.derived.top_dirs.retain(|&id| {
        let i = id as usize;
        i < total_nodes && (flags_col[i] & hidden_mask) == 0 && !subtree_set.contains(&id)
    });

    for &nid in &subtree_set {
        let flags = tree.flags_of(nid);
        if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
            continue;
        }
        let alloc = tree.allocated_size[nid as usize];
        if alloc == 0 {
            continue;
        }
        match tree.kind_of(nid) {
            NodeKind::Dir if nid != 0 => tree.derived.top_dirs.push(nid),
            NodeKind::File | NodeKind::CloudPlaceholder => tree.derived.top_files.push(nid),
            _ => {}
        }
    }

    tree.derived
        .top_files
        .sort_by(|&a, &b| tree.allocated_size[b as usize].cmp(&tree.allocated_size[a as usize]));
    tree.derived.top_files.truncate(TOP_INDEX_LIMIT);
    tree.derived
        .top_dirs
        .sort_by(|&a, &b| tree.allocated_size[b as usize].cmp(&tree.allocated_size[a as usize]));
    tree.derived.top_dirs.truncate(TOP_INDEX_LIMIT);

    // Refresh ancestor global caches sizes if any ancestor is a tracked global cache
    let mut anc = tree.parent_of(subtree_root);
    let mut anc_set = HashSet::new();
    while anc != NONE_NODE {
        anc_set.insert(anc);
        anc = tree.parent_of(anc);
    }
    for entry in &mut tree.derived.global_caches {
        if anc_set.contains(&entry.node_id) {
            let idx = entry.node_id as usize;
            entry.size_bytes = tree.allocated_size[idx];
            entry.file_count = tree.file_count[idx];
            entry.last_write_ts = tree.last_write_ts[idx] as i64;
        }
    }

    tree.derived
        .dev_artifacts
        .sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
    tree.derived
        .global_caches
        .sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
}

fn match_dev_artifact_in_parent(
    tree: &ScanTree,
    parent_id: NodeId,
    dir_name_lower: &str,
    ctx: &DerivedBuildContext,
) -> Option<(String, String)> {
    // Collect sibling names once only when `dir_name_lower` matches a candidate artifact folder name
    let mut sibling_names: Vec<String> = Vec::new();
    let mut sib = tree.first_child_of(parent_id);
    while sib != NONE_NODE {
        if !tree.flags_of(sib).contains(NodeFlags::TOMBSTONE) {
            sibling_names.push(tree.node_name(sib).to_ascii_lowercase());
        }
        sib = tree.next_sibling_of(sib);
    }

    for rule in &ctx.dev_artifact_rules {
        if !rule.single_segment_patterns.contains(dir_name_lower) {
            continue;
        }
        if rule.required_markers.is_empty() {
            return Some((rule.rule_id.clone(), String::new()));
        }
        for marker in &rule.required_markers {
            if let Some(found) = sibling_names
                .iter()
                .find(|s| matches_simple_glob(marker, s))
            {
                return Some((rule.rule_id.clone(), found.clone()));
            }
        }
    }
    None
}

fn insert_top_bounded(vec: &mut Vec<(u64, NodeId)>, item: (u64, NodeId), limit: usize) {
    if vec.len() < limit {
        vec.push(item);
        if vec.len() == limit {
            // Keep minimum at the end for O(1) rejection of small items
            vec.sort_by_key(|a| std::cmp::Reverse(a.0));
        }
    } else if let Some(&(min_val, _)) = vec.last() {
        if item.0 > min_val {
            vec.pop();
            let pos = vec.partition_point(|&(v, _)| v >= item.0);
            vec.insert(pos, item);
        }
    }
}

/// Convenience helper for converting a derived cache path to `PathBuf`.
pub fn normalize_volume_key(root: &str) -> String {
    let t = root.trim().replace('/', "\\").to_ascii_uppercase();
    if t.len() == 2 && t.ends_with(':') {
        format!("{t}\\")
    } else if t.len() >= 3 && t.as_bytes()[1] == b':' {
        format!("{}\\", &t[..2])
    } else {
        t
    }
}

pub fn volume_root_of_path(path: &Path) -> Option<String> {
    let s = path.to_string_lossy();
    let stripped = s
        .strip_prefix(r"\\?\")
        .or_else(|| s.strip_prefix(r"\??\"))
        .unwrap_or(&s);
    if stripped.len() >= 2 && stripped.as_bytes()[1] == b':' {
        Some(format!(
            "{}:\\",
            (stripped.as_bytes()[0] as char).to_ascii_uppercase()
        ))
    } else {
        None
    }
}
