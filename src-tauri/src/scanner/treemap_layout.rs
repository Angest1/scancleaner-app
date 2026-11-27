use serde::Serialize;

use super::model::{NodeFlags, NodeId, ScanTree};
use crate::protection::ProtectionEngine;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapLayoutRectDto {
    pub id: String,
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
    pub is_cloud_only: bool,
    pub is_pending_trash: bool,
    pub protection_reason: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreemapLayoutDto {
    pub node_id: NodeId,
    pub full_path: String,
    pub total_allocated_bytes: u64,
    pub file_count: u32,
    pub breadcrumbs: Vec<crate::commands::BreadcrumbItem>,
    pub rects: Vec<TreemapLayoutRectDto>,
}

struct LayoutCandidate {
    node_id: Option<NodeId>,
    name: String,
    full_path: String,
    kind: String,
    allocated_bytes: u64,
    logical_bytes: u64,
    file_count: u32,
    type_cat: String,
    origin_cat: String,
    percentage: f64,
    is_other_bucket: bool,
    is_protected: bool,
    is_cloud_only: bool,
    is_pending_trash: bool,
    protection_reason: Option<String>,
}

/// Computes squarified treemap layout natively in Rust off the UI thread,
/// culling sub-pixel rectangles below `min_px` and coalescing tail items into an "Otros" bucket.
pub fn compute_treemap_layout(
    tree: &ScanTree,
    node_id: NodeId,
    width: f64,
    height: f64,
    min_px: f64,
    max_items: usize,
) -> Result<TreemapLayoutDto, String> {
    let idx = node_id as usize;
    if idx >= tree.len() {
        return Err("Nodo fuera de rango.".to_string());
    }
    let width = width.max(1.0);
    let height = height.max(1.0);
    let min_px = min_px.clamp(1.0, 24.0);
    let total_alloc = tree.allocated_size[idx].max(1);

    // Breadcrumbs
    let mut breadcrumbs = Vec::new();
    let mut cur = node_id;
    let mut guard = 0usize;
    while cur != super::model::NONE_NODE && guard < 256 {
        breadcrumbs.push(crate::commands::BreadcrumbItem {
            node_id: cur,
            name: tree.node_name(cur).to_string(),
        });
        cur = tree.parent_of(cur);
        guard += 1;
    }
    breadcrumbs.reverse();

    let mut raw_children: Vec<NodeId> = tree.children_iter(node_id).collect();
    raw_children
        .sort_by(|&a, &b| tree.allocated_size[b as usize].cmp(&tree.allocated_size[a as usize]));

    let total_area = width * height;
    let min_area_threshold = min_px * min_px;
    let prot_engine = ProtectionEngine::load_default();

    let mut candidates: Vec<LayoutCandidate> = Vec::new();
    let mut other_bytes = 0u64;
    let mut other_logical = 0u64;
    let mut other_files = 0u32;
    let mut other_count = 0usize;

    for (pos, cid) in raw_children.into_iter().enumerate() {
        let cidx = cid as usize;
        let alloc = tree.allocated_size[cidx];
        if alloc == 0 {
            continue;
        }
        let est_area = (alloc as f64 / total_alloc as f64) * total_area;
        if pos < max_items && est_area >= min_area_threshold {
            let full_pb = tree.reconstruct_path(cid);
            let flags = tree.flags_of(cid);
            let is_prot_flag = flags.contains(NodeFlags::PROTECTED)
                || flags.contains(NodeFlags::PROTECTED_DESCENDANT);
            let prot_reason = if is_prot_flag {
                prot_engine.is_protected_for_ui(&full_pb).reason
            } else {
                None
            };
            candidates.push(LayoutCandidate {
                node_id: Some(cid),
                name: tree.node_name(cid).to_string(),
                full_path: full_pb.to_string_lossy().into_owned(),
                kind: format!("{:?}", tree.kind_of(cid)).to_ascii_lowercase(),
                allocated_bytes: alloc,
                logical_bytes: tree.logical_size[cidx],
                file_count: tree.file_count[cidx],
                type_cat: format!("{:?}", tree.type_cat_of(cid)).to_ascii_lowercase(),
                origin_cat: format!("{:?}", tree.origin_cat_of(cid)).to_ascii_lowercase(),
                percentage: (alloc as f64 / total_alloc as f64) * 100.0,
                is_other_bucket: false,
                is_protected: is_prot_flag,
                is_cloud_only: flags.contains(NodeFlags::CLOUD_ONLY),
                is_pending_trash: flags.contains(NodeFlags::PENDING_TRASH),
                protection_reason: prot_reason,
            });
        } else {
            other_bytes = other_bytes.saturating_add(alloc);
            other_logical = other_logical.saturating_add(tree.logical_size[cidx]);
            other_files = other_files.saturating_add(tree.file_count[cidx]);
            other_count += 1;
        }
    }

    if other_count > 0 && other_bytes > 0 {
        candidates.push(LayoutCandidate {
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
            is_cloud_only: false,
            is_pending_trash: false,
            protection_reason: None,
        });
    }

    let rects = squarify(candidates, width, height, min_px);

    Ok(TreemapLayoutDto {
        node_id,
        full_path: tree
            .reconstruct_path(node_id)
            .to_string_lossy()
            .into_owned(),
        total_allocated_bytes: tree.allocated_size[idx],
        file_count: tree.file_count[idx],
        breadcrumbs,
        rects,
    })
}

fn worst_aspect_ratio(row: &[f64], side: f64) -> f64 {
    if row.is_empty() || side <= 0.0 {
        return f64::INFINITY;
    }
    let mut sum = 0.0;
    let mut min = f64::INFINITY;
    let mut max = 0.0f64;
    for &v in row {
        sum += v;
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if sum <= 0.0 || min <= 0.0 {
        return f64::INFINITY;
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    ((s2 * max) / sum2).max(sum2 / (s2 * min))
}

fn squarify(
    items: Vec<LayoutCandidate>,
    width: f64,
    height: f64,
    min_px: f64,
) -> Vec<TreemapLayoutRectDto> {
    if items.is_empty() || width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let total_val: f64 = items.iter().map(|i| i.allocated_bytes as f64).sum();
    if total_val <= 0.0 {
        return Vec::new();
    }
    let total_area = width * height;
    let areas: Vec<f64> = items
        .iter()
        .map(|i| (i.allocated_bytes as f64 / total_val) * total_area)
        .collect();

    let mut items_opt: Vec<Option<LayoutCandidate>> = items.into_iter().map(Some).collect();
    let mut rects = Vec::with_capacity(items_opt.len());

    let mut rx = 0.0;
    let mut ry = 0.0;
    let mut rw = width;
    let mut rh = height;
    let mut idx = 0usize;

    while idx < items_opt.len() {
        let horizontal = rw >= rh;
        let side = if horizontal { rh } else { rw };
        let mut row_areas = vec![areas[idx]];
        let mut row_indices = vec![idx];
        idx += 1;

        while idx < items_opt.len() {
            let cand_area = areas[idx];
            let cur_worst = worst_aspect_ratio(&row_areas, side);
            row_areas.push(cand_area);
            let next_worst = worst_aspect_ratio(&row_areas, side);
            if next_worst <= cur_worst {
                row_indices.push(idx);
                idx += 1;
            } else {
                row_areas.pop();
                break;
            }
        }

        let row_sum: f64 = row_areas.iter().sum();
        if horizontal {
            let col_w = if side > 0.0 { row_sum / side } else { 0.0 };
            let mut cur_y = ry;
            for (pos, &item_i) in row_indices.iter().enumerate() {
                let item_h = if col_w > 0.0 {
                    row_areas[pos] / col_w
                } else {
                    0.0
                };
                if col_w >= min_px && item_h >= min_px {
                    if let Some(c) = items_opt[item_i].take() {
                        rects.push(to_rect_dto(c, item_i, rx, cur_y, col_w, item_h));
                    }
                }
                cur_y += item_h;
            }
            rx += col_w;
            rw = (rw - col_w).max(0.0);
        } else {
            let row_h = if side > 0.0 { row_sum / side } else { 0.0 };
            let mut cur_x = rx;
            for (pos, &item_i) in row_indices.iter().enumerate() {
                let item_w = if row_h > 0.0 {
                    row_areas[pos] / row_h
                } else {
                    0.0
                };
                if item_w >= min_px && row_h >= min_px {
                    if let Some(c) = items_opt[item_i].take() {
                        rects.push(to_rect_dto(c, item_i, cur_x, ry, item_w, row_h));
                    }
                }
                cur_x += item_w;
            }
            ry += row_h;
            rh = (rh - row_h).max(0.0);
        }
    }

    rects
}

fn to_rect_dto(
    c: LayoutCandidate,
    idx: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> TreemapLayoutRectDto {
    TreemapLayoutRectDto {
        id: match c.node_id {
            Some(nid) => format!("n-{nid}"),
            None => format!("other-{idx}"),
        },
        node_id: c.node_id,
        name: c.name,
        full_path: c.full_path,
        kind: c.kind,
        allocated_bytes: c.allocated_bytes,
        logical_bytes: c.logical_bytes,
        file_count: c.file_count,
        type_cat: c.type_cat,
        origin_cat: c.origin_cat,
        percentage: c.percentage,
        is_other_bucket: c.is_other_bucket,
        is_protected: c.is_protected,
        is_cloud_only: c.is_cloud_only,
        is_pending_trash: c.is_pending_trash,
        protection_reason: c.protection_reason,
        x,
        y,
        w,
        h,
    }
}
