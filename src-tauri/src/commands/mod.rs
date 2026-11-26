use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

use crate::apps::{
    list_installed_apps, open_windows_apps_settings, trigger_native_uninstall, AppSource,
    InstalledApp,
};
use crate::categorizer::{classify_tree, user_rules_path, CategoryBreakdown, RulesConfig};
use crate::cleaner::detectors::{discover_cleanup_candidates, CleanupCandidatesReport};
use crate::cleaner::executor::{
    build_cleanup_plan, execute_cleanup_plan, CleanupPlan, CleanupResult, PreviewCleanupRequest,
};
use crate::dev_cleaner::executor::{build_dev_cleanup_plan, execute_dev_cleanup_plan};
use crate::dev_cleaner::folder_picker::{pick_file_native, pick_folder_native};
use crate::dev_cleaner::process::SystemCommandRunner;
use crate::dev_cleaner::scan_dev_cleanup;
use crate::dev_cleaner::schema::{
    DevCleanerConfig, DevCleanupPlan, DevCleanupResult, DevPreviewRequest, DevRulesCatalog,
    DevScanReport,
};
use crate::diagnostics::{log_info, log_timing, read_diagnostic_logs};
use crate::elevation::{current_elevation_status, relaunch_as_admin, ElevationStatus};
use crate::protection::{
    is_protected_default, user_protection_config_path, ProtectedEntry, ProtectionEngine,
    ProtectionOrigin, ProtectionResult, ProtectionType,
};
use crate::scanner::drives::{list_drives, DriveInfo};
use crate::scanner::model::{NodeId, NodeKind, ScanProgress, ScanSummary};
use crate::scanner::scan_directory;
use crate::state::AppState;
use crate::storage::{logs_dir, read_cleanup_logs, CleanupLogEntry, ScanSnapshot, SnapshotDiff};
use crate::system_files::{
    inspect_system_files, open_power_options, open_virtual_memory_dialog, run_powercfg_elevated,
    PowercfgAction, SystemFilesReport, WinSystemProcessLauncher,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupContext {
    pub elevation: ElevationStatus,
    pub startup_drive: Option<String>,
    pub rules_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadcrumbItem {
    pub node_id: NodeId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapNodeDto {
    pub node_id: Option<NodeId>,
    pub name: String,
    pub full_path: String,
    pub kind: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u32,
    pub type_cat: String,
    pub origin_cat: String,
    pub percentage: f64,
    pub is_other_bucket: bool,
    pub is_protected: bool,
    pub protection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapViewDto {
    pub node_id: NodeId,
    pub full_path: String,
    pub total_allocated_bytes: u64,
    pub file_count: u32,
    pub breadcrumbs: Vec<BreadcrumbItem>,
    pub children: Vec<TreemapNodeDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopEntryDto {
    pub node_id: NodeId,
    pub name: String,
    pub full_path: String,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub file_count: u32,
    pub last_write_ts: i64,
    pub type_cat: String,
    pub origin_cat: String,
    pub is_protected: bool,
    pub protection_reason: Option<String>,
}

#[tauri::command]
pub fn get_startup_context(state: State<'_, AppState>) -> StartupContext {
    StartupContext {
        elevation: current_elevation_status(),
        startup_drive: state.startup_drive.clone(),
        rules_path: user_rules_path().to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn cmd_list_drives(include_network: bool) -> Vec<DriveInfo> {
    list_drives(include_network)
}

#[tauri::command]
pub async fn cmd_start_scan(
    root_path: String,
    on_progress: Channel<ScanProgress>,
    state: State<'_, AppState>,
) -> Result<ScanSummary, String> {
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.cancel_flag.lock().map_err(|e| e.to_string())?;
        if let Some(prev) = guard.as_ref() {
            prev.store(true, Ordering::Relaxed);
        }
        *guard = Some(cancel_flag.clone());
    }

    let t0 = std::time::Instant::now();
    log_info(&format!("cmd_start_scan begin: {root_path}"));
    let root = PathBuf::from(&root_path);
    let tree_res = tauri::async_runtime::spawn_blocking(move || {
        let mut tree = scan_directory(root, cancel_flag, |prog| {
            let _ = on_progress.send(prog);
        });
        let rules = RulesConfig::load();
        let breakdown = classify_tree(&mut tree, &rules);
        (tree, breakdown)
    })
    .await
    .map_err(|e| format!("Error ejecutando el escaneo: {e}"))?;

    let (tree, breakdown) = tree_res;
    log_timing(
        "cmd_start_scan",
        t0.elapsed().as_millis(),
        &format!(
            "root={root_path} files={} dirs={} cancelled={}",
            tree.stats.total_files, tree.stats.total_dirs, tree.stats.cancelled
        ),
    );

    let mut snapshot_id = None;
    if !tree.stats.cancelled {
        let drives = list_drives(false);
        let (total_disk, free_disk) = drives
            .iter()
            .find(|d| d.root_path.eq_ignore_ascii_case(&root_path))
            .map_or((0, 0), |d| (d.total_bytes, d.free_bytes));

        if let Ok(id) =
            state
                .storage
                .save_snapshot(&tree, total_disk, free_disk, &breakdown.by_origin)
        {
            snapshot_id = Some(id);
        }
    }

    let summary = ScanSummary {
        root_path: tree.root_path.to_string_lossy().into_owned(),
        fs_type: tree.fs_type,
        stats: tree.stats.clone(),
        snapshot_id,
    };

    {
        let mut tree_guard = state.active_tree.lock().map_err(|e| e.to_string())?;
        *tree_guard = Some(tree);
    }
    {
        let mut bd_guard = state.active_breakdown.lock().map_err(|e| e.to_string())?;
        *bd_guard = Some(breakdown);
    }

    Ok(summary)
}

#[tauri::command]
pub fn cmd_cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.cancel_flag.lock().map_err(|e| e.to_string())?;
    if let Some(flag) = guard.as_ref() {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_get_treemap_children(
    node_id: NodeId,
    max_items: Option<usize>,
    state: State<'_, AppState>,
) -> Result<TreemapViewDto, String> {
    let limit = max_items.unwrap_or(140).clamp(10, 400);
    let tree_guard = state.active_tree.lock().map_err(|e| e.to_string())?;
    let tree = tree_guard
        .as_ref()
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;

    let idx = node_id as usize;
    if idx >= tree.nodes.len() {
        return Err("Nodo fuera de rango.".to_string());
    }

    let current = &tree.nodes[idx];
    let total_alloc = current.allocated_size.max(1);

    // Build breadcrumbs
    let mut breadcrumbs = Vec::new();
    let mut cur = Some(node_id);
    while let Some(nid) = cur {
        breadcrumbs.push(BreadcrumbItem {
            node_id: nid,
            name: tree.node_name(nid).to_string(),
        });
        cur = tree.nodes[nid as usize].parent;
    }
    breadcrumbs.reverse();

    let mut raw_children: Vec<NodeId> = tree.children_iter(node_id).collect();
    raw_children.sort_by(|&a, &b| {
        tree.nodes[b as usize]
            .allocated_size
            .cmp(&tree.nodes[a as usize].allocated_size)
    });

    let mut children = Vec::new();
    let prot_engine = ProtectionEngine::load_default();
    let mut other_bytes = 0u64;
    let mut other_logical = 0u64;
    let mut other_files = 0u32;
    let mut other_count = 0usize;

    for (pos, cid) in raw_children.into_iter().enumerate() {
        let cnode = &tree.nodes[cid as usize];
        if cnode.allocated_size == 0 && pos >= 30 {
            continue;
        }
        if pos < limit {
            let full_pb = tree.reconstruct_path(cid);
            let prot = prot_engine.is_protected_for_ui(&full_pb);
            children.push(TreemapNodeDto {
                node_id: Some(cid),
                name: tree.node_name(cid).to_string(),
                full_path: full_pb.to_string_lossy().into_owned(),
                kind: format!("{:?}", cnode.kind).to_ascii_lowercase(),
                allocated_bytes: cnode.allocated_size,
                logical_bytes: cnode.logical_size,
                file_count: cnode.file_count,
                type_cat: format!("{:?}", cnode.type_cat).to_ascii_lowercase(),
                origin_cat: format!("{:?}", cnode.origin_cat).to_ascii_lowercase(),
                percentage: (cnode.allocated_size as f64 / total_alloc as f64) * 100.0,
                is_other_bucket: false,
                is_protected: prot.protected,
                protection_reason: prot.reason,
            });
        } else {
            other_bytes = other_bytes.saturating_add(cnode.allocated_size);
            other_logical = other_logical.saturating_add(cnode.logical_size);
            other_files = other_files.saturating_add(cnode.file_count);
            other_count += 1;
        }
    }

    if other_count > 0 && other_bytes > 0 {
        children.push(TreemapNodeDto {
            node_id: None,
            name: format!("Otros ({other_count} elementos)"),
            full_path: tree
                .reconstruct_path(node_id)
                .to_string_lossy()
                .into_owned(),
            kind: "other".to_string(),
            allocated_bytes: other_bytes,
            logical_bytes: other_logical,
            file_count: other_files,
            type_cat: "other".to_string(),
            origin_cat: "userother".to_string(),
            percentage: (other_bytes as f64 / total_alloc as f64) * 100.0,
            is_other_bucket: true,
            is_protected: false,
            protection_reason: None,
        });
    }

    Ok(TreemapViewDto {
        node_id,
        full_path: tree
            .reconstruct_path(node_id)
            .to_string_lossy()
            .into_owned(),
        total_allocated_bytes: current.allocated_size,
        file_count: current.file_count,
        breadcrumbs,
        children,
    })
}

#[tauri::command]
pub fn cmd_get_top_entries(
    scope_node_id: Option<NodeId>,
    kind: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<TopEntryDto>, String> {
    let max_rows = limit.unwrap_or(100).clamp(1, 500);
    let tree_guard = state.active_tree.lock().map_err(|e| e.to_string())?;
    let tree = tree_guard
        .as_ref()
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;

    let scope = scope_node_id.unwrap_or(0);
    let want_dirs = kind == "dirs";
    let mut matches: Vec<NodeId> = Vec::new();

    // Collect subtree nodes via stack so ranking honors the current folder scope
    let mut stack = vec![scope];
    while let Some(nid) = stack.pop() {
        for cid in tree.children_iter(nid) {
            let n = &tree.nodes[cid as usize];
            if n.kind == NodeKind::Dir {
                stack.push(cid);
                if want_dirs && n.allocated_size > 0 {
                    matches.push(cid);
                }
            } else if !want_dirs && n.kind == NodeKind::File && n.allocated_size > 0 {
                matches.push(cid);
            }
        }
    }

    matches.sort_by(|&a, &b| {
        tree.nodes[b as usize]
            .allocated_size
            .cmp(&tree.nodes[a as usize].allocated_size)
    });
    matches.truncate(max_rows);

    let prot_engine = ProtectionEngine::load_default();
    Ok(matches
        .into_iter()
        .map(|nid| {
            let n = &tree.nodes[nid as usize];
            let full_pb = tree.reconstruct_path(nid);
            let prot = prot_engine.is_protected_for_ui(&full_pb);
            TopEntryDto {
                node_id: nid,
                name: tree.node_name(nid).to_string(),
                full_path: full_pb.to_string_lossy().into_owned(),
                allocated_bytes: n.allocated_size,
                logical_bytes: n.logical_size,
                file_count: n.file_count,
                last_write_ts: n.last_write_ts,
                type_cat: format!("{:?}", n.type_cat).to_ascii_lowercase(),
                origin_cat: format!("{:?}", n.origin_cat).to_ascii_lowercase(),
                is_protected: prot.protected,
                protection_reason: prot.reason,
            }
        })
        .collect())
}

#[tauri::command]
pub fn cmd_get_category_breakdown(state: State<'_, AppState>) -> Result<CategoryBreakdown, String> {
    let guard = state.active_breakdown.lock().map_err(|e| e.to_string())?;
    guard
        .clone()
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())
}

#[tauri::command]
pub fn cmd_reload_rules(state: State<'_, AppState>) -> Result<CategoryBreakdown, String> {
    let mut tree_guard = state.active_tree.lock().map_err(|e| e.to_string())?;
    let tree = tree_guard
        .as_mut()
        .ok_or_else(|| "No hay ningún escaneo activo.".to_string())?;
    let rules = RulesConfig::load();
    let breakdown = classify_tree(tree, &rules);
    let mut bd_guard = state.active_breakdown.lock().map_err(|e| e.to_string())?;
    *bd_guard = Some(breakdown.clone());
    Ok(breakdown)
}

#[tauri::command]
pub fn cmd_get_access_denied(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let max_items = limit.unwrap_or(200);
    let guard = state.active_tree.lock().map_err(|e| e.to_string())?;
    let Some(tree) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    Ok(tree
        .access_denied
        .iter()
        .take(max_items)
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub fn cmd_open_in_explorer(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("La ruta ya no existe en disco.".to_string());
    }
    Command::new("explorer.exe")
        .arg(format!("/select,{path}"))
        .spawn()
        .map_err(|e| format!("No se pudo abrir el Explorador: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn cmd_list_installed_apps(
    state: State<'_, AppState>,
) -> Result<Vec<InstalledApp>, String> {
    let tree_clone = {
        let guard = state.active_tree.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    tauri::async_runtime::spawn_blocking(move || list_installed_apps(tree_clone.as_ref()))
        .await
        .map_err(|e| format!("Error listando aplicaciones: {e}"))
}

#[tauri::command]
pub fn cmd_uninstall_app(
    source: AppSource,
    uninstall_string: Option<String>,
) -> Result<String, String> {
    trigger_native_uninstall(source, uninstall_string.as_deref())
}

#[tauri::command]
pub fn cmd_open_windows_apps_settings() -> Result<(), String> {
    open_windows_apps_settings()
}

#[tauri::command]
pub async fn cmd_discover_cleanup_candidates(
    min_large_mb: Option<u64>,
    min_age_days: Option<u64>,
    state: State<'_, AppState>,
) -> Result<CleanupCandidatesReport, String> {
    let tree_clone = {
        let guard = state.active_tree.lock().map_err(|e| e.to_string())?;
        guard.clone()
    };
    let mb = min_large_mb.unwrap_or(100);
    let days = min_age_days.unwrap_or(30);
    tauri::async_runtime::spawn_blocking(move || {
        discover_cleanup_candidates(tree_clone.as_ref(), mb, days)
    })
    .await
    .map_err(|e| format!("Error analizando candidatos de limpieza: {e}"))
}

#[tauri::command]
pub async fn cmd_preview_cleanup(
    request: PreviewCleanupRequest,
    state: State<'_, AppState>,
) -> Result<CleanupPlan, String> {
    let plan = tauri::async_runtime::spawn_blocking(move || build_cleanup_plan(request))
        .await
        .map_err(|e| format!("Error generando vista previa: {e}"))?;

    let mut guard = state.pending_plans.lock().map_err(|e| e.to_string())?;
    guard.insert(plan.plan_id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
pub async fn cmd_execute_cleanup(
    plan_id: String,
    confirmed_permanent: bool,
    state: State<'_, AppState>,
) -> Result<CleanupResult, String> {
    let plan = {
        let mut guard = state.pending_plans.lock().map_err(|e| e.to_string())?;
        guard
            .remove(&plan_id)
            .ok_or_else(|| "El plan de limpieza expiró o ya fue ejecutado.".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || {
        execute_cleanup_plan(&plan, confirmed_permanent, None, None)
    })
    .await
    .map_err(|e| format!("Error ejecutando limpieza: {e}"))?
}

#[tauri::command]
pub fn cmd_relaunch_as_admin(
    active_drive: Option<String>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    relaunch_as_admin(active_drive)?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn cmd_list_snapshots(
    drive_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ScanSnapshot>, String> {
    state.storage.list_snapshots(drive_root.as_deref())
}

#[tauri::command]
pub fn cmd_diff_snapshots(
    older_id: i64,
    newer_id: i64,
    state: State<'_, AppState>,
) -> Result<SnapshotDiff, String> {
    state.storage.diff_snapshots(older_id, newer_id)
}

#[tauri::command]
pub fn cmd_list_cleanup_logs(limit: Option<usize>) -> Vec<CleanupLogEntry> {
    read_cleanup_logs(&logs_dir(), limit.unwrap_or(200))
}

#[tauri::command]
pub fn cmd_dev_get_config() -> DevCleanerConfig {
    DevCleanerConfig::load()
}

#[tauri::command]
pub fn cmd_dev_save_config(config: DevCleanerConfig) -> Result<DevCleanerConfig, String> {
    config.save()?;
    Ok(config)
}

#[tauri::command]
pub async fn cmd_dev_pick_project_root() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(pick_folder_native)
        .await
        .map_err(|e| format!("Error abriendo selector de carpetas: {e}"))?
}

#[tauri::command]
pub async fn cmd_dev_scan(config: Option<DevCleanerConfig>) -> Result<DevScanReport, String> {
    let cfg = config.unwrap_or_else(DevCleanerConfig::load);
    tauri::async_runtime::spawn_blocking(move || scan_dev_cleanup(&cfg))
        .await
        .map_err(|e| format!("Error ejecutando escaneo Dev: {e}"))?
}

#[tauri::command]
pub async fn cmd_dev_preview(
    request: DevPreviewRequest,
    state: State<'_, AppState>,
) -> Result<DevCleanupPlan, String> {
    let plan = tauri::async_runtime::spawn_blocking(move || -> Result<DevCleanupPlan, String> {
        let catalog = DevRulesCatalog::load_embedded()?;
        let runner = SystemCommandRunner;
        Ok(build_dev_cleanup_plan(request, &catalog, &runner))
    })
    .await
    .map_err(|e| format!("Error generando dry-run Dev: {e}"))??;

    let mut guard = state.pending_dev_plans.lock().map_err(|e| e.to_string())?;
    guard.insert(plan.plan_id.clone(), plan.clone());
    Ok(plan)
}

#[tauri::command]
pub async fn cmd_dev_execute(
    plan_id: String,
    state: State<'_, AppState>,
) -> Result<DevCleanupResult, String> {
    let plan = {
        let mut guard = state.pending_dev_plans.lock().map_err(|e| e.to_string())?;
        guard
            .remove(&plan_id)
            .ok_or_else(|| "El plan de Limpieza Dev expiró o ya fue ejecutado.".to_string())?
    };

    tauri::async_runtime::spawn_blocking(move || -> Result<DevCleanupResult, String> {
        let catalog = DevRulesCatalog::load_embedded()?;
        let runner = SystemCommandRunner;
        Ok(execute_dev_cleanup_plan(
            &plan, &catalog, &runner, None, None,
        ))
    })
    .await
    .map_err(|e| format!("Error ejecutando Limpieza Dev: {e}"))?
}

#[tauri::command]
pub fn cmd_protection_list() -> Vec<ProtectedEntry> {
    ProtectionEngine::load_default().all_entries()
}

#[tauri::command]
pub fn cmd_protection_add(
    patron: String,
    tipo: ProtectionType,
    motivo: String,
    todas_las_unidades: bool,
) -> Result<Vec<ProtectedEntry>, String> {
    let trimmed = patron.trim();
    if trimmed.is_empty() {
        return Err("El patrón o ruta no puede estar vacío.".to_string());
    }
    let mut engine = ProtectionEngine::load_default();
    let entry = ProtectedEntry {
        id: format!("user-{}", uuid::Uuid::new_v4()),
        patron: trimmed.to_string(),
        tipo,
        origen: ProtectionOrigin::Usuario,
        motivo: if motivo.trim().is_empty() {
            "Protegida manualmente por el usuario.".to_string()
        } else {
            motivo.trim().to_string()
        },
        todas_las_unidades,
    };
    engine.user_entries.push(entry);
    engine.save_user_entries(&user_protection_config_path())?;
    Ok(engine.all_entries())
}

#[tauri::command]
pub fn cmd_protection_remove(id: String) -> Result<Vec<ProtectedEntry>, String> {
    let mut engine = ProtectionEngine::load_default();
    if engine.critical_entries.iter().any(|e| e.id == id) {
        return Err("Las entradas críticas de fábrica no se pueden eliminar.".to_string());
    }
    let before = engine.user_entries.len();
    engine.user_entries.retain(|e| e.id != id);
    if engine.user_entries.len() == before {
        return Err("Entrada de usuario no encontrada.".to_string());
    }
    engine.save_user_entries(&user_protection_config_path())?;
    Ok(engine.all_entries())
}

#[tauri::command]
pub fn cmd_protection_check(path: String) -> ProtectionResult {
    is_protected_default(std::path::Path::new(&path))
}

#[tauri::command]
pub fn cmd_protection_preview_matches(
    patron: String,
    tipo: ProtectionType,
    todas_las_unidades: bool,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let trimmed = patron.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let guard = state.active_tree.lock().map_err(|e| e.to_string())?;
    let Some(tree) = guard.as_ref() else {
        return Ok(0);
    };
    let candidate = ProtectedEntry {
        id: "preview".to_string(),
        patron: trimmed.to_string(),
        tipo,
        origen: ProtectionOrigin::Usuario,
        motivo: String::new(),
        todas_las_unidades,
    };
    Ok(ProtectionEngine::preview_matches_in_tree(tree, &candidate))
}

#[tauri::command]
pub async fn cmd_pick_file() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(pick_file_native)
        .await
        .map_err(|e| format!("Error abriendo selector de archivos: {e}"))?
}

#[tauri::command]
pub fn cmd_system_files_report(drive_root: Option<String>) -> SystemFilesReport {
    inspect_system_files(drive_root.as_deref())
}

#[tauri::command]
pub fn cmd_open_virtual_memory() -> Result<String, String> {
    let sys_root = std::env::var("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows"));
    open_virtual_memory_dialog(&sys_root, &WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_open_power_options() -> Result<(), String> {
    open_power_options(&WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_run_powercfg_elevated(action: PowercfgAction) -> Result<String, String> {
    run_powercfg_elevated(action, &WinSystemProcessLauncher)
}

#[tauri::command]
pub fn cmd_list_diagnostic_logs(limit: Option<usize>) -> Vec<String> {
    read_diagnostic_logs(limit.unwrap_or(300))
}

#[tauri::command]
pub fn cmd_open_logs_folder() -> Result<(), String> {
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("No se pudo abrir la carpeta de logs: {e}"))?;
    Ok(())
}
