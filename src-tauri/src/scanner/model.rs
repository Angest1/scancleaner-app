use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::derived::DerivedIndexes;

pub type NodeId = u32;
pub const NONE_NODE: NodeId = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemType {
    Ntfs = 0,
    Fat32 = 1,
    ExFat = 2,
    Refs = 3,
    Network = 4,
    Unknown = 5,
}

impl FileSystemType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Ntfs,
            1 => Self::Fat32,
            2 => Self::ExFat,
            3 => Self::Refs,
            4 => Self::Network,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Dir = 0,
    File = 1,
    Symlink = 2,
    Junction = 3,
    CloudPlaceholder = 4,
}

impl NodeKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Dir,
            1 => Self::File,
            2 => Self::Symlink,
            3 => Self::Junction,
            4 => Self::CloudPlaceholder,
            _ => Self::File,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum TypeCategory {
    Video = 0,
    Image = 1,
    Audio = 2,
    Documents = 3,
    Archives = 4,
    Executables = 5,
    Code = 6,
    Other = 7,
}

impl TypeCategory {
    pub const ALL: [TypeCategory; 8] = [
        TypeCategory::Video,
        TypeCategory::Image,
        TypeCategory::Audio,
        TypeCategory::Documents,
        TypeCategory::Archives,
        TypeCategory::Executables,
        TypeCategory::Code,
        TypeCategory::Other,
    ];

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Video,
            1 => Self::Image,
            2 => Self::Audio,
            3 => Self::Documents,
            4 => Self::Archives,
            5 => Self::Executables,
            6 => Self::Code,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TypeCategory::Video => "Videos",
            TypeCategory::Image => "Imágenes",
            TypeCategory::Audio => "Audio",
            TypeCategory::Documents => "Documentos",
            TypeCategory::Archives => "Comprimidos",
            TypeCategory::Executables => "Ejecutables e instaladores",
            TypeCategory::Code => "Código y datos",
            TypeCategory::Other => "Otros",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum OriginCategory {
    InstalledApps = 0,
    System = 1,
    Temp = 2,
    BrowserCache = 3,
    DevArtifact = 4,
    RecycleBin = 5,
    Downloads = 6,
    UserOther = 7,
}

impl OriginCategory {
    pub const ALL: [OriginCategory; 8] = [
        OriginCategory::InstalledApps,
        OriginCategory::System,
        OriginCategory::Temp,
        OriginCategory::BrowserCache,
        OriginCategory::DevArtifact,
        OriginCategory::RecycleBin,
        OriginCategory::Downloads,
        OriginCategory::UserOther,
    ];

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::InstalledApps,
            1 => Self::System,
            2 => Self::Temp,
            3 => Self::BrowserCache,
            4 => Self::DevArtifact,
            5 => Self::RecycleBin,
            6 => Self::Downloads,
            _ => Self::UserOther,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OriginCategory::InstalledApps => "Aplicaciones instaladas",
            OriginCategory::System => "Sistema Windows",
            OriginCategory::Temp => "Temporales",
            OriginCategory::BrowserCache => "Cachés de navegadores",
            OriginCategory::DevArtifact => "Artefactos de desarrollo",
            OriginCategory::RecycleBin => "Papelera de reciclaje",
            OriginCategory::Downloads => "Descargas",
            OriginCategory::UserOther => "Archivos de usuario",
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub struct NodeFlags: u16 {
        const HARDLINK_DUP        = 1 << 0;
        const COMPRESSED          = 1 << 1;
        const SPARSE              = 1 << 2;
        const CLOUD_ONLY          = 1 << 3;
        const PROTECTED           = 1 << 4;
        const PROTECTED_DESCENDANT = 1 << 5;
        const RECALL_ON_OPEN      = 1 << 6;
        const OFFLINE             = 1 << 7;
        const HAS_ADS             = 1 << 8;
        const PENDING_TRASH       = 1 << 9;
        const OPTIMISTIC_HIDDEN   = 1 << 10;
        const TOMBSTONE           = 1 << 11;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanEngineMode {
    MftDirect,
    MftService,
    #[default]
    FallbackWorkStealing,
    SnapshotCached,
    UsnIncremental,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub duration_ms: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub total_logical_bytes: u64,
    pub total_allocated_bytes: u64,
    pub hardlinks_deduplicated: u64,
    pub symlinks_skipped: u64,
    pub junctions_skipped: u64,
    pub cloud_placeholders: u64,
    pub ads_streams_counted: u64,
    pub access_denied_count: u64,
    pub cancelled: bool,
    pub engine_mode: ScanEngineMode,
    pub stale: bool,
}

/// Flat Structure-of-Arrays (SoA) file system tree with interned UTF-8 name pool,
/// NTFS FRN reverse lookup for O(1) USN deltas, optimistic deletion support,
/// and incrementally-maintained derived indexes.
#[derive(Debug, Clone)]
pub struct ScanTree {
    pub root_path: PathBuf,
    pub volume_serial: u32,
    pub fs_type: FileSystemType,
    pub usn_journal_id: u64,
    pub last_usn: i64,
    pub include_ads: bool,

    // Flat SoA columns (indexed by NodeId)
    pub parent: Vec<NodeId>,
    pub first_child: Vec<NodeId>,
    pub next_sibling: Vec<NodeId>,
    pub name_offset: Vec<u32>,
    pub name_len: Vec<u16>,
    pub logical_size: Vec<u64>,
    pub allocated_size: Vec<u64>,
    pub file_count: Vec<u32>,
    pub last_write_ts: Vec<u32>,
    pub kind: Vec<u8>,
    pub flags: Vec<u16>,
    pub type_cat: Vec<u8>,
    pub origin_cat: Vec<u8>,
    pub file_ref: Vec<u64>,

    /// Interned UTF-8 name pool
    pub name_pool: Vec<u8>,
    /// Optional interning map (populated during build to deduplicate repeated file names)
    intern_map: HashMap<u64, Vec<(u32, u16)>>,
    /// Free list of recycled `TOMBSTONE` node IDs so incremental churn never grows the SoA vectors
    free_list: Vec<NodeId>,
    tombstone_count: usize,
    dead_name_bytes: usize,
    track_frns: bool,

    /// NTFS FRN / FileId -> NodeId lookup for O(1) incremental USN updates (populated lazily when needed)
    pub frn_to_node: HashMap<u64, NodeId>,
    pub access_denied: Vec<PathBuf>,
    pub stats: ScanStats,
    pub derived: DerivedIndexes,
}

impl ScanTree {
    pub fn new(root_path: PathBuf, volume_serial: u32, fs_type: FileSystemType) -> Self {
        Self::with_capacity(root_path, volume_serial, fs_type, 4096)
    }

    pub fn with_capacity(
        root_path: PathBuf,
        volume_serial: u32,
        fs_type: FileSystemType,
        cap: usize,
    ) -> Self {
        let mut tree = Self {
            root_path: root_path.clone(),
            volume_serial,
            fs_type,
            usn_journal_id: 0,
            last_usn: 0,
            include_ads: false,
            parent: Vec::with_capacity(cap),
            first_child: Vec::with_capacity(cap),
            next_sibling: Vec::with_capacity(cap),
            name_offset: Vec::with_capacity(cap),
            name_len: Vec::with_capacity(cap),
            logical_size: Vec::with_capacity(cap),
            allocated_size: Vec::with_capacity(cap),
            file_count: Vec::with_capacity(cap),
            last_write_ts: Vec::with_capacity(cap),
            kind: Vec::with_capacity(cap),
            flags: Vec::with_capacity(cap),
            type_cat: Vec::with_capacity(cap),
            origin_cat: Vec::with_capacity(cap),
            file_ref: Vec::with_capacity(cap),
            name_pool: Vec::with_capacity(cap.saturating_mul(14)),
            intern_map: HashMap::new(),
            free_list: Vec::new(),
            tombstone_count: 0,
            dead_name_bytes: 0,
            track_frns: false,
            frn_to_node: HashMap::new(),
            access_denied: Vec::new(),
            stats: ScanStats::default(),
            derived: DerivedIndexes::default(),
        };
        let root_name = root_path.to_string_lossy().into_owned();
        let (off, len) = tree.intern_name(&root_name);
        tree.push_raw_node(
            NONE_NODE,
            off,
            len,
            NodeKind::Dir,
            0,
            0,
            0,
            NodeFlags::empty(),
            0,
        );
        tree
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    #[inline]
    pub fn tombstone_count(&self) -> usize {
        self.tombstone_count
    }

    #[inline]
    pub fn dead_name_bytes(&self) -> usize {
        self.dead_name_bytes
    }

    /// Enables FRN reverse-lookup population with pre-reserved capacity (for MFT builds & USN sync).
    pub fn enable_frn_tracking(&mut self, reserve: usize) {
        self.track_frns = true;
        if reserve > 0 {
            self.frn_to_node.reserve(reserve);
        }
    }

    /// Lazily populates `frn_to_node` on demand when USN incremental sync actually runs.
    pub fn ensure_frn_index(&mut self) {
        if !self.track_frns || (self.frn_to_node.is_empty() && self.len() > 1) {
            self.track_frns = true;
            self.rebuild_frn_index();
        }
    }

    /// Drops build-time caches and shrinks excess `Vec` / `HashMap` capacity across all SoA columns.
    pub fn shrink_build_caches(&mut self) {
        self.intern_map = HashMap::new();
        self.parent.shrink_to_fit();
        self.first_child.shrink_to_fit();
        self.next_sibling.shrink_to_fit();
        self.name_offset.shrink_to_fit();
        self.name_len.shrink_to_fit();
        self.logical_size.shrink_to_fit();
        self.allocated_size.shrink_to_fit();
        self.file_count.shrink_to_fit();
        self.last_write_ts.shrink_to_fit();
        self.kind.shrink_to_fit();
        self.flags.shrink_to_fit();
        self.type_cat.shrink_to_fit();
        self.origin_cat.shrink_to_fit();
        self.file_ref.shrink_to_fit();
        self.name_pool.shrink_to_fit();
        self.free_list.shrink_to_fit();
        self.access_denied.shrink_to_fit();
        self.frn_to_node.shrink_to_fit();
    }

    #[inline]
    pub fn parent_of(&self, id: NodeId) -> NodeId {
        self.parent[id as usize]
    }

    #[inline]
    pub fn first_child_of(&self, id: NodeId) -> NodeId {
        self.first_child[id as usize]
    }

    #[inline]
    pub fn next_sibling_of(&self, id: NodeId) -> NodeId {
        self.next_sibling[id as usize]
    }

    #[inline]
    pub fn kind_of(&self, id: NodeId) -> NodeKind {
        NodeKind::from_u8(self.kind[id as usize])
    }

    #[inline]
    pub fn flags_of(&self, id: NodeId) -> NodeFlags {
        NodeFlags::from_bits_truncate(self.flags[id as usize])
    }

    #[inline]
    pub fn add_flag(&mut self, id: NodeId, f: NodeFlags) {
        self.flags[id as usize] |= f.bits();
    }

    #[inline]
    pub fn remove_flag(&mut self, id: NodeId, f: NodeFlags) {
        self.flags[id as usize] &= !f.bits();
    }

    #[inline]
    pub fn type_cat_of(&self, id: NodeId) -> TypeCategory {
        TypeCategory::from_u8(self.type_cat[id as usize])
    }

    #[inline]
    pub fn set_type_cat(&mut self, id: NodeId, cat: TypeCategory) {
        self.type_cat[id as usize] = cat as u8;
    }

    #[inline]
    pub fn origin_cat_of(&self, id: NodeId) -> OriginCategory {
        OriginCategory::from_u8(self.origin_cat[id as usize])
    }

    #[inline]
    pub fn set_origin_cat(&mut self, id: NodeId, cat: OriginCategory) {
        self.origin_cat[id as usize] = cat as u8;
    }

    #[inline]
    pub fn last_write_ts_of(&self, id: NodeId) -> i64 {
        self.last_write_ts[id as usize] as i64
    }

    /// Interns a UTF-8 name into `name_pool`. Short/common filenames (<= 32 bytes) are
    /// deduplicated via a lightweight FNV-1a bucket map to cut RAM usage on deep trees (`node_modules`).
    pub fn intern_name(&mut self, name: &str) -> (u32, u16) {
        let bytes = name.as_bytes();
        let clipped_len = bytes.len().min(u16::MAX as usize);
        let slice = &bytes[..clipped_len];

        if clipped_len <= 32 {
            let h = fnv1a_64(slice);
            if let Some(bucket) = self.intern_map.get(&h) {
                for &(off, len) in bucket {
                    let s = off as usize;
                    let e = s + len as usize;
                    if &self.name_pool[s..e] == slice {
                        return (off, len);
                    }
                }
            }
            let offset = self.name_pool.len() as u32;
            let len = clipped_len as u16;
            self.name_pool.extend_from_slice(slice);
            self.intern_map.entry(h).or_default().push((offset, len));
            (offset, len)
        } else {
            let offset = self.name_pool.len() as u32;
            let len = clipped_len as u16;
            self.name_pool.extend_from_slice(slice);
            (offset, len)
        }
    }

    #[inline]
    pub fn node_name(&self, id: NodeId) -> &str {
        let idx = id as usize;
        let start = self.name_offset[idx] as usize;
        let end = start + self.name_len[idx] as usize;
        std::str::from_utf8(&self.name_pool[start..end]).unwrap_or("")
    }

    #[allow(clippy::too_many_arguments)]
    fn push_raw_node(
        &mut self,
        parent: NodeId,
        name_offset: u32,
        name_len: u16,
        kind: NodeKind,
        logical_size: u64,
        allocated_size: u64,
        last_write_ts: i64,
        flags: NodeFlags,
        file_ref: u64,
    ) -> NodeId {
        let fcount = u32::from(matches!(kind, NodeKind::File | NodeKind::CloudPlaceholder));
        let ts_u32 = last_write_ts.clamp(0, u32::MAX as i64) as u32;
        let id = if let Some(reused) = self.free_list.pop() {
            self.tombstone_count = self.tombstone_count.saturating_sub(1);
            let idx = reused as usize;
            self.parent[idx] = parent;
            self.first_child[idx] = NONE_NODE;
            self.next_sibling[idx] = NONE_NODE;
            self.name_offset[idx] = name_offset;
            self.name_len[idx] = name_len;
            self.logical_size[idx] = logical_size;
            self.allocated_size[idx] = allocated_size;
            self.file_count[idx] = fcount;
            self.last_write_ts[idx] = ts_u32;
            self.kind[idx] = kind as u8;
            self.flags[idx] = flags.bits();
            self.type_cat[idx] = TypeCategory::Other as u8;
            self.origin_cat[idx] = OriginCategory::UserOther as u8;
            self.file_ref[idx] = file_ref;
            reused
        } else {
            let new_id = self.parent.len() as NodeId;
            self.parent.push(parent);
            self.first_child.push(NONE_NODE);
            self.next_sibling.push(NONE_NODE);
            self.name_offset.push(name_offset);
            self.name_len.push(name_len);
            self.logical_size.push(logical_size);
            self.allocated_size.push(allocated_size);
            self.file_count.push(fcount);
            self.last_write_ts.push(ts_u32);
            self.kind.push(kind as u8);
            self.flags.push(flags.bits());
            self.type_cat.push(TypeCategory::Other as u8);
            self.origin_cat.push(OriginCategory::UserOther as u8);
            self.file_ref.push(file_ref);
            new_id
        };
        if self.track_frns && file_ref != 0 {
            self.frn_to_node.entry(file_ref).or_insert(id);
        }
        id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_child(
        &mut self,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        logical_size: u64,
        allocated_size: u64,
        last_write_ts: i64,
        flags: NodeFlags,
    ) -> NodeId {
        self.add_child_with_frn(
            parent,
            name,
            kind,
            logical_size,
            allocated_size,
            last_write_ts,
            flags,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_child_with_frn(
        &mut self,
        parent: NodeId,
        name: &str,
        kind: NodeKind,
        logical_size: u64,
        allocated_size: u64,
        last_write_ts: i64,
        flags: NodeFlags,
        file_ref: u64,
    ) -> NodeId {
        let (name_offset, name_len) = self.intern_name(name);
        let id = self.push_raw_node(
            parent,
            name_offset,
            name_len,
            kind,
            logical_size,
            allocated_size,
            last_write_ts,
            flags,
            file_ref,
        );
        let prev_first = self.first_child[parent as usize];
        self.next_sibling[id as usize] = prev_first;
        self.first_child[parent as usize] = id;
        id
    }

    /// Rebuilds the `frn_to_node` HashMap from the flat `file_ref` column with exact capacity.
    pub fn rebuild_frn_index(&mut self) {
        self.track_frns = true;
        self.frn_to_node.clear();
        let live_frns = self
            .file_ref
            .iter()
            .enumerate()
            .filter(|&(idx, &frn)| {
                frn != 0 && (self.flags[idx] & NodeFlags::TOMBSTONE.bits()) == 0
            })
            .count();
        self.frn_to_node.reserve(live_frns);
        for (idx, &frn) in self.file_ref.iter().enumerate() {
            if frn != 0 && (self.flags[idx] & NodeFlags::TOMBSTONE.bits()) == 0 {
                self.frn_to_node.entry(frn).or_insert(idx as NodeId);
            }
        }
        self.frn_to_node.shrink_to_fit();
    }

    pub fn reconstruct_path(&self, id: NodeId) -> PathBuf {
        if id == 0 || (id as usize) >= self.len() {
            return self.root_path.clone();
        }
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = id;
        let mut guard = 0usize;
        while cur != NONE_NODE && cur != 0 && guard < 256 {
            parts.push(self.node_name(cur));
            cur = self.parent[cur as usize];
            guard += 1;
        }
        let mut path = self.root_path.clone();
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path
    }

    pub fn children_iter(&self, parent: NodeId) -> ChildIter<'_> {
        let next = if (parent as usize) < self.len() {
            self.first_child[parent as usize]
        } else {
            NONE_NODE
        };
        ChildIter { tree: self, next }
    }

    /// Aggregates sizes and file counts bottom-up across the flat SoA arrays.
    /// Safe for both parent-before-child and arbitrary MFT ordering (via iterative DFS post-order).
    pub fn aggregate_bottom_up(&mut self) {
        if self.is_empty() {
            return;
        }
        let mut order: Vec<NodeId> = Vec::with_capacity(self.len());
        let mut stack = vec![0u32];
        while let Some(nid) = stack.pop() {
            let flags = self.flags_of(nid);
            if flags.contains(NodeFlags::TOMBSTONE) || flags.contains(NodeFlags::OPTIMISTIC_HIDDEN)
            {
                continue;
            }
            order.push(nid);
            if self.kind_of(nid) == NodeKind::Dir {
                let idx = nid as usize;
                self.allocated_size[idx] = 0;
                self.logical_size[idx] = 0;
                self.file_count[idx] = 0;
                let mut child = self.first_child[idx];
                while child != NONE_NODE {
                    stack.push(child);
                    child = self.next_sibling[child as usize];
                }
            }
        }

        let mut total_files: u64 = 0;
        let mut total_dirs: u64 = 0;

        for &nid in order.iter().rev() {
            let idx = nid as usize;
            match self.kind_of(nid) {
                NodeKind::Dir => total_dirs += 1,
                NodeKind::File | NodeKind::CloudPlaceholder => total_files += 1,
                NodeKind::Symlink | NodeKind::Junction => {}
            }
            let pid = self.parent[idx];
            if pid != NONE_NODE {
                let pidx = pid as usize;
                self.logical_size[pidx] =
                    self.logical_size[pidx].saturating_add(self.logical_size[idx]);
                self.allocated_size[pidx] =
                    self.allocated_size[pidx].saturating_add(self.allocated_size[idx]);
                self.file_count[pidx] = self.file_count[pidx].saturating_add(self.file_count[idx]);
                if self.last_write_ts[idx] > self.last_write_ts[pidx] {
                    self.last_write_ts[pidx] = self.last_write_ts[idx];
                }
            }
        }

        self.stats.total_logical_bytes = self.logical_size[0];
        self.stats.total_allocated_bytes = self.allocated_size[0];
        self.stats.total_files = total_files;
        self.stats.total_dirs = total_dirs;
        self.stats.access_denied_count = self.access_denied.len() as u64;
    }

    /// Propagates a signed delta up the parent chain starting at `start_parent` (inclusive).
    pub fn propagate_delta_upwards(
        &mut self,
        start_parent: NodeId,
        delta_alloc: i64,
        delta_logical: i64,
        delta_files: i64,
    ) {
        let mut cur = start_parent;
        let mut guard = 0usize;
        while cur != NONE_NODE && (cur as usize) < self.len() && guard < 256 {
            let idx = cur as usize;
            self.allocated_size[idx] = apply_i64_u64(self.allocated_size[idx], delta_alloc);
            self.logical_size[idx] = apply_i64_u64(self.logical_size[idx], delta_logical);
            self.file_count[idx] = apply_i64_u32(self.file_count[idx], delta_files);
            cur = self.parent[idx];
            guard += 1;
        }
        self.stats.total_allocated_bytes = self.allocated_size[0];
        self.stats.total_logical_bytes = self.logical_size[0];
        self.stats.total_files = apply_i64_u64(self.stats.total_files, delta_files);
    }

    /// Optimistically hides a node/subtree from UI queries immediately while background deletion runs.
    /// Returns true if state changed (so caller can revert on failure).
    pub fn optimistic_hide(&mut self, nid: NodeId) -> bool {
        if nid == 0 || (nid as usize) >= self.len() {
            return false;
        }
        let flags = self.flags_of(nid);
        if flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) || flags.contains(NodeFlags::TOMBSTONE) {
            return false;
        }
        self.add_flag(nid, NodeFlags::OPTIMISTIC_HIDDEN);

        let idx = nid as usize;
        let alloc = self.allocated_size[idx] as i64;
        let logical = self.logical_size[idx] as i64;
        let files = self.file_count[idx] as i64;
        let pid = self.parent[idx];
        self.propagate_delta_upwards(pid, -alloc, -logical, -files);

        // Subtract subtree file contributions from derived category totals
        self.adjust_subtree_category_totals(nid, -1);
        true
    }

    /// Reverts a previous `optimistic_hide` if deletion failed or was cancelled.
    pub fn optimistic_revert(&mut self, nid: NodeId) -> bool {
        if nid == 0 || (nid as usize) >= self.len() {
            return false;
        }
        let flags = self.flags_of(nid);
        if !flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) || flags.contains(NodeFlags::TOMBSTONE) {
            return false;
        }
        self.remove_flag(nid, NodeFlags::OPTIMISTIC_HIDDEN);

        let idx = nid as usize;
        let alloc = self.allocated_size[idx] as i64;
        let logical = self.logical_size[idx] as i64;
        let files = self.file_count[idx] as i64;
        let pid = self.parent[idx];
        self.propagate_delta_upwards(pid, alloc, logical, files);
        self.adjust_subtree_category_totals(nid, 1);
        true
    }

    /// Unlinks `nid` from its parent's sibling list and marks the entire subtree as TOMBSTONE,
    /// subtracting its aggregate totals from ancestors (if not already optimistically hidden).
    pub fn remove_subtree(&mut self, nid: NodeId) -> Option<NodeId> {
        if nid == 0 || (nid as usize) >= self.len() {
            return None;
        }
        let flags = self.flags_of(nid);
        if flags.contains(NodeFlags::TOMBSTONE) {
            return None;
        }

        let pid = self.parent[nid as usize];
        if !flags.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
            let idx = nid as usize;
            let alloc = self.allocated_size[idx] as i64;
            let logical = self.logical_size[idx] as i64;
            let files = self.file_count[idx] as i64;
            self.propagate_delta_upwards(pid, -alloc, -logical, -files);
            self.adjust_subtree_category_totals(nid, -1);
        }

        // Unlink from parent's child linked list
        if pid != NONE_NODE {
            self.unlink_child(pid, nid);
        }

        // Mark entire subtree as TOMBSTONE, remove FRN entries, and collect slots for recycling
        let mut stack = vec![nid];
        let mut removed_dirs = 0u64;
        let mut newly_recycled: Vec<NodeId> = Vec::new();
        while let Some(cur) = stack.pop() {
            let cidx = cur as usize;
            let was_tomb = (self.flags[cidx] & NodeFlags::TOMBSTONE.bits()) != 0;
            if !was_tomb && self.kind_of(cur) == NodeKind::Dir {
                removed_dirs += 1;
            }
            self.flags[cidx] = NodeFlags::TOMBSTONE.bits();
            let frn = self.file_ref[cidx];
            if frn != 0 && self.frn_to_node.get(&frn).copied() == Some(cur) {
                self.frn_to_node.remove(&frn);
            }
            let mut ch = self.first_child[cidx];
            while ch != NONE_NODE {
                stack.push(ch);
                ch = self.next_sibling[ch as usize];
            }
            self.first_child[cidx] = NONE_NODE;
            self.next_sibling[cidx] = NONE_NODE;
            self.parent[cidx] = NONE_NODE;
            self.logical_size[cidx] = 0;
            self.allocated_size[cidx] = 0;
            self.file_count[cidx] = 0;
            self.file_ref[cidx] = 0;
            if !was_tomb && cur != 0 {
                self.dead_name_bytes = self
                    .dead_name_bytes
                    .saturating_add(self.name_len[cidx] as usize);
                self.name_len[cidx] = 0;
                self.name_offset[cidx] = 0;
                newly_recycled.push(cur);
            }
        }
        self.stats.total_dirs = self.stats.total_dirs.saturating_sub(removed_dirs);

        // Immediately purge TOMBSTONE entries from derived vectors BEFORE pushing recycled IDs
        // to `free_list` so a recycled NodeId can never collide with a stale derived reference!
        if !newly_recycled.is_empty() {
            let flags_ref = &self.flags;
            let n_len = flags_ref.len();
            let tomb_bits = NodeFlags::TOMBSTONE.bits();
            let is_live = |id: NodeId| (id as usize) < n_len && (flags_ref[id as usize] & tomb_bits) == 0;
            self.derived.top_files.retain(|&id| is_live(id));
            self.derived.top_dirs.retain(|&id| is_live(id));
            if removed_dirs > 0 {
                self.derived
                    .dev_artifacts
                    .retain(|e| is_live(e.node_id) && is_live(e.project_dir_id));
                self.derived.global_caches.retain(|e| is_live(e.node_id));
                self.derived.pending_trash_nodes.retain(|&id| is_live(id));
            }
            self.tombstone_count += newly_recycled.len();
            self.free_list.extend(newly_recycled);
        }

        if self.dead_name_bytes > 4 * 1024 * 1024 || self.intern_map.len() > 8_192 {
            self.compact_name_pool();
        }

        Some(pid)
    }

    fn unlink_child(&mut self, parent: NodeId, target: NodeId) {
        let mut prev = NONE_NODE;
        let mut cur = self.first_child[parent as usize];
        while cur != NONE_NODE {
            let next = self.next_sibling[cur as usize];
            if cur == target {
                if prev == NONE_NODE {
                    self.first_child[parent as usize] = next;
                } else {
                    self.next_sibling[prev as usize] = next;
                }
                self.next_sibling[target as usize] = NONE_NODE;
                return;
            }
            prev = cur;
            cur = next;
        }
    }

    /// Renames a node in place (no-op if name is identical; tracks dead string bytes).
    pub fn rename_node(&mut self, nid: NodeId, new_name: &str) {
        if nid == 0 || (nid as usize) >= self.len() {
            return;
        }
        if self.node_name(nid) == new_name {
            return;
        }
        let old_len = self.name_len[nid as usize] as usize;
        self.dead_name_bytes = self.dead_name_bytes.saturating_add(old_len);
        let (off, len) = self.intern_name(new_name);
        self.name_offset[nid as usize] = off;
        self.name_len[nid as usize] = len;
        if self.dead_name_bytes > 4 * 1024 * 1024 || self.intern_map.len() > 8_192 {
            self.compact_name_pool();
        }
    }

    /// Rebuilds `name_pool` in-place from live nodes (deduplicating names <= 32 bytes)
    /// WITHOUT changing any `NodeId`!
    pub fn compact_name_pool(&mut self) {
        let n = self.len();
        let mut new_pool: Vec<u8> = Vec::with_capacity(self.name_pool.len().min(n.saturating_mul(14)));
        let mut tmp_intern: HashMap<u64, Vec<(u32, u16)>> = HashMap::new();
        let tomb_bits = NodeFlags::TOMBSTONE.bits();

        for idx in 0..n {
            if (self.flags[idx] & tomb_bits) != 0 {
                self.name_offset[idx] = 0;
                self.name_len[idx] = 0;
                continue;
            }
            let s = self.name_offset[idx] as usize;
            let len_u16 = self.name_len[idx];
            let e = s + (len_u16 as usize);
            if e > self.name_pool.len() || len_u16 == 0 {
                self.name_offset[idx] = 0;
                self.name_len[idx] = 0;
                continue;
            }
            let slice = &self.name_pool[s..e];
            if slice.len() <= 32 {
                let h = fnv1a_64(slice);
                let mut found = None;
                if let Some(bucket) = tmp_intern.get(&h) {
                    for &(off, l) in bucket {
                        if &new_pool[off as usize..(off as usize + l as usize)] == slice {
                            found = Some((off, l));
                            break;
                        }
                    }
                }
                if let Some((off, l)) = found {
                    self.name_offset[idx] = off;
                    self.name_len[idx] = l;
                } else {
                    let off = new_pool.len() as u32;
                    new_pool.extend_from_slice(slice);
                    tmp_intern.entry(h).or_default().push((off, len_u16));
                    self.name_offset[idx] = off;
                    self.name_len[idx] = len_u16;
                }
            } else {
                let off = new_pool.len() as u32;
                new_pool.extend_from_slice(slice);
                self.name_offset[idx] = off;
                self.name_len[idx] = len_u16;
            }
        }

        new_pool.shrink_to_fit();
        self.name_pool = new_pool;
        self.intern_map = HashMap::new();
        self.dead_name_bytes = 0;
    }

    /// Returns true if the tree contains any `TOMBSTONE` slots or unreachable orphaned nodes
    /// (e.g. when loading a legacy bloated `.scidx` snapshot).
    pub fn has_dead_or_unreachable_nodes(&self) -> bool {
        if self.tombstone_count > 0 || self.dead_name_bytes > 0 {
            return true;
        }
        let tomb_bits = NodeFlags::TOMBSTONE.bits();
        if self.flags.iter().any(|&f| (f & tomb_bits) != 0) {
            return true;
        }
        if self.len() > 1 {
            // Quick check: root is parent=NONE_NODE; any other node with parent==NONE_NODE is orphaned
            if self.parent[1..].contains(&NONE_NODE) {
                return true;
            }
        }
        false
    }

    /// Full structural compaction: retains only live nodes reachable from root `0`,
    /// remapping all links and `derived` indexes and compacting `name_pool` + SoA capacities.
    pub fn compact(&mut self) -> bool {
        if self.is_empty() {
            return false;
        }
        if self
            .flags
            .iter()
            .any(|&f| (f & NodeFlags::OPTIMISTIC_HIDDEN.bits()) != 0)
        {
            return false;
        }

        let old_len = self.len();
        let tomb_bits = NodeFlags::TOMBSTONE.bits();
        let mut remap = vec![NONE_NODE; old_len];
        let mut order: Vec<NodeId> =
            Vec::with_capacity(old_len.saturating_sub(self.tombstone_count));

        remap[0] = 0;
        order.push(0);
        let mut qi = 0usize;
        while qi < order.len() {
            let old_nid = order[qi];
            qi += 1;
            let mut ch = self.first_child[old_nid as usize];
            while ch != NONE_NODE && (ch as usize) < old_len {
                let cidx = ch as usize;
                let next = self.next_sibling[cidx];
                if (self.flags[cidx] & tomb_bits) == 0 && remap[cidx] == NONE_NODE {
                    let new_id = order.len() as NodeId;
                    remap[cidx] = new_id;
                    order.push(ch);
                }
                ch = next;
            }
        }

        let live_n = order.len();
        if live_n == old_len
            && self.tombstone_count == 0
            && self.dead_name_bytes == 0
            && self.name_pool.len() <= live_n.saturating_mul(28)
        {
            self.shrink_build_caches();
            return false;
        }

        let mut new_parent = vec![NONE_NODE; live_n];
        let mut new_first_child = vec![NONE_NODE; live_n];
        let mut new_next_sibling = vec![NONE_NODE; live_n];
        let mut new_name_offset = vec![0u32; live_n];
        let mut new_name_len = vec![0u16; live_n];
        let mut new_logical_size = vec![0u64; live_n];
        let mut new_allocated_size = vec![0u64; live_n];
        let mut new_file_count = vec![0u32; live_n];
        let mut new_last_write_ts = vec![0u32; live_n];
        let mut new_kind = vec![0u8; live_n];
        let mut new_flags = vec![0u16; live_n];
        let mut new_type_cat = vec![0u8; live_n];
        let mut new_origin_cat = vec![0u8; live_n];
        let mut new_file_ref = vec![0u64; live_n];
        let mut new_pool: Vec<u8> = Vec::with_capacity(live_n.saturating_mul(14));
        let mut tmp_intern: HashMap<u64, Vec<(u32, u16)>> = HashMap::new();

        for (new_idx, &old_nid) in order.iter().enumerate() {
            let oidx = old_nid as usize;
            new_parent[new_idx] = if new_idx == 0 {
                NONE_NODE
            } else {
                let op = self.parent[oidx];
                if (op as usize) < old_len {
                    remap[op as usize]
                } else {
                    NONE_NODE
                }
            };

            // Preserve exact live sibling order under `old_nid`
            let mut prev_mapped = NONE_NODE;
            let mut ch = self.first_child[oidx];
            while ch != NONE_NODE && (ch as usize) < old_len {
                let next = self.next_sibling[ch as usize];
                let mapped_ch = remap[ch as usize];
                if mapped_ch != NONE_NODE {
                    if prev_mapped == NONE_NODE {
                        new_first_child[new_idx] = mapped_ch;
                    } else {
                        new_next_sibling[prev_mapped as usize] = mapped_ch;
                    }
                    prev_mapped = mapped_ch;
                }
                ch = next;
            }

            let s = self.name_offset[oidx] as usize;
            let len_u16 = self.name_len[oidx];
            let e = s + (len_u16 as usize);
            if len_u16 > 0 && e <= self.name_pool.len() {
                let slice = &self.name_pool[s..e];
                if slice.len() <= 32 {
                    let h = fnv1a_64(slice);
                    let mut found = None;
                    if let Some(bucket) = tmp_intern.get(&h) {
                        for &(off, l) in bucket {
                            if &new_pool[off as usize..(off as usize + l as usize)] == slice {
                                found = Some((off, l));
                                break;
                            }
                        }
                    }
                    if let Some((off, l)) = found {
                        new_name_offset[new_idx] = off;
                        new_name_len[new_idx] = l;
                    } else {
                        let off = new_pool.len() as u32;
                        new_pool.extend_from_slice(slice);
                        tmp_intern.entry(h).or_default().push((off, len_u16));
                        new_name_offset[new_idx] = off;
                        new_name_len[new_idx] = len_u16;
                    }
                } else {
                    let off = new_pool.len() as u32;
                    new_pool.extend_from_slice(slice);
                    new_name_offset[new_idx] = off;
                    new_name_len[new_idx] = len_u16;
                }
            }

            new_logical_size[new_idx] = self.logical_size[oidx];
            new_allocated_size[new_idx] = self.allocated_size[oidx];
            new_file_count[new_idx] = self.file_count[oidx];
            new_last_write_ts[new_idx] = self.last_write_ts[oidx];
            new_kind[new_idx] = self.kind[oidx];
            new_flags[new_idx] = self.flags[oidx];
            new_type_cat[new_idx] = self.type_cat[oidx];
            new_origin_cat[new_idx] = self.origin_cat[oidx];
            new_file_ref[new_idx] = self.file_ref[oidx];
        }
        drop(tmp_intern);
        drop(order);

        let map_id = |id: NodeId| -> Option<NodeId> {
            remap.get(id as usize).copied().filter(|&m| m != NONE_NODE)
        };
        self.derived.top_files = self
            .derived
            .top_files
            .iter()
            .filter_map(|&id| map_id(id))
            .collect();
        self.derived.top_files.dedup();
        self.derived.top_dirs = self
            .derived
            .top_dirs
            .iter()
            .filter_map(|&id| map_id(id))
            .collect();
        self.derived.top_dirs.dedup();
        self.derived.pending_trash_nodes = self
            .derived
            .pending_trash_nodes
            .iter()
            .filter_map(|&id| map_id(id))
            .collect();
        self.derived.pending_trash_nodes.dedup();

        let mut new_dev = Vec::with_capacity(self.derived.dev_artifacts.len());
        for mut entry in std::mem::take(&mut self.derived.dev_artifacts) {
            if let (Some(nid), Some(pid)) = (map_id(entry.node_id), map_id(entry.project_dir_id)) {
                entry.node_id = nid;
                entry.project_dir_id = pid;
                entry.size_bytes = new_allocated_size[nid as usize];
                entry.file_count = new_file_count[nid as usize];
                entry.last_write_ts = new_last_write_ts[nid as usize] as i64;
                new_dev.push(entry);
            }
        }
        new_dev.sort_by_key(|e| e.node_id);
        new_dev.dedup_by_key(|e| e.node_id);
        new_dev.sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
        self.derived.dev_artifacts = new_dev;

        let mut new_caches = Vec::with_capacity(self.derived.global_caches.len());
        for mut entry in std::mem::take(&mut self.derived.global_caches) {
            if let Some(nid) = map_id(entry.node_id) {
                entry.node_id = nid;
                entry.size_bytes = new_allocated_size[nid as usize];
                entry.file_count = new_file_count[nid as usize];
                entry.last_write_ts = new_last_write_ts[nid as usize] as i64;
                new_caches.push(entry);
            }
        }
        new_caches.sort_by_key(|e| e.node_id);
        new_caches.dedup_by_key(|e| e.node_id);
        new_caches.sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
        self.derived.global_caches = new_caches;

        self.parent = new_parent;
        self.first_child = new_first_child;
        self.next_sibling = new_next_sibling;
        self.name_offset = new_name_offset;
        self.name_len = new_name_len;
        self.logical_size = new_logical_size;
        self.allocated_size = new_allocated_size;
        self.file_count = new_file_count;
        self.last_write_ts = new_last_write_ts;
        self.kind = new_kind;
        self.flags = new_flags;
        self.type_cat = new_type_cat;
        self.origin_cat = new_origin_cat;
        self.file_ref = new_file_ref;
        self.name_pool = new_pool;
        self.free_list = Vec::new();
        self.tombstone_count = 0;
        self.dead_name_bytes = 0;

        if self.track_frns {
            self.rebuild_frn_index();
        } else {
            self.frn_to_node = HashMap::new();
        }
        self.shrink_build_caches();
        true
    }

    /// Moves a node (and its entire subtree) to a new parent directory, updating aggregates
    /// along both the old and new parent chains.
    pub fn move_node(&mut self, nid: NodeId, new_parent: NodeId) -> bool {
        if nid == 0
            || (nid as usize) >= self.len()
            || (new_parent as usize) >= self.len()
            || nid == new_parent
        {
            return false;
        }
        let old_parent = self.parent[nid as usize];
        if old_parent == new_parent {
            return true;
        }
        // Prevent cycles: ensure `new_parent` is not a descendant of `nid`
        let mut anc = new_parent;
        let mut guard = 0usize;
        while anc != NONE_NODE && guard < 256 {
            if anc == nid {
                return false;
            }
            anc = self.parent[anc as usize];
            guard += 1;
        }

        let idx = nid as usize;
        let hidden = self.flags_of(nid).contains(NodeFlags::OPTIMISTIC_HIDDEN);
        let alloc = self.allocated_size[idx] as i64;
        let logical = self.logical_size[idx] as i64;
        let files = self.file_count[idx] as i64;

        if old_parent != NONE_NODE {
            self.unlink_child(old_parent, nid);
            if !hidden {
                self.propagate_delta_upwards(old_parent, -alloc, -logical, -files);
            }
        }

        self.parent[idx] = new_parent;
        let prev_first = self.first_child[new_parent as usize];
        self.next_sibling[idx] = prev_first;
        self.first_child[new_parent as usize] = nid;

        if !hidden {
            self.propagate_delta_upwards(new_parent, alloc, logical, files);
        }
        true
    }

    /// Updates a file node's logical & allocated sizes and bubbles the delta up parent chains
    /// + category totals.
    pub fn update_file_sizes(
        &mut self,
        nid: NodeId,
        new_logical: u64,
        new_allocated: u64,
        new_ts: i64,
    ) {
        if (nid as usize) >= self.len() {
            return;
        }
        let idx = nid as usize;
        let old_alloc = self.allocated_size[idx];
        let old_logical = self.logical_size[idx];
        self.allocated_size[idx] = new_allocated;
        self.logical_size[idx] = new_logical;
        self.last_write_ts[idx] = new_ts.clamp(0, u32::MAX as i64) as u32;

        let d_alloc = new_allocated as i64 - old_alloc as i64;
        let d_logical = new_logical as i64 - old_logical as i64;
        if d_alloc != 0 || d_logical != 0 {
            let pid = self.parent[idx];
            if !self.flags_of(nid).contains(NodeFlags::OPTIMISTIC_HIDDEN) {
                self.propagate_delta_upwards(pid, d_alloc, d_logical, 0);
                let tcat = self.type_cat_of(nid);
                let ocat = self.origin_cat_of(nid);
                self.derived
                    .apply_file_delta(tcat, ocat, d_alloc, d_logical, 0);
            }
        }
    }

    fn adjust_subtree_category_totals(&mut self, root: NodeId, sign: i64) {
        let mut stack = vec![root];
        while let Some(cur) = stack.pop() {
            let cidx = cur as usize;
            if self.flags_of(cur).contains(NodeFlags::TOMBSTONE) {
                continue;
            }
            match self.kind_of(cur) {
                NodeKind::File | NodeKind::CloudPlaceholder => {
                    let alloc = (self.allocated_size[cidx] as i64) * sign;
                    let logical = (self.logical_size[cidx] as i64) * sign;
                    let tcat = self.type_cat_of(cur);
                    let ocat = self.origin_cat_of(cur);
                    self.derived
                        .apply_file_delta(tcat, ocat, alloc, logical, sign);
                }
                NodeKind::Dir => {
                    let mut ch = self.first_child[cidx];
                    while ch != NONE_NODE {
                        stack.push(ch);
                        ch = self.next_sibling[ch as usize];
                    }
                }
                _ => {}
            }
        }
    }

    pub fn find_node_by_path(&self, target: &Path) -> Option<NodeId> {
        let root_str = self.root_path.to_string_lossy().to_ascii_lowercase();
        let target_str = target.to_string_lossy().to_ascii_lowercase();
        let root_trimmed = root_str.trim_end_matches('\\');
        let target_trimmed = target_str.trim_end_matches('\\');

        if target_trimmed == root_trimmed {
            return Some(0);
        }
        let rel_str = target_trimmed
            .strip_prefix(&format!(r"{root_trimmed}\"))
            .or_else(|| target_trimmed.strip_prefix(root_trimmed))?;
        let mut current: NodeId = 0;
        for seg in rel_str.split('\\').filter(|s| !s.is_empty()) {
            let mut found = None;
            for child in self.children_iter(current) {
                if self.node_name(child).eq_ignore_ascii_case(seg) {
                    found = Some(child);
                    break;
                }
            }
            current = found?;
        }
        Some(current)
    }

    /// Finds the nearest existing ancestor node if `target` disappeared.
    pub fn find_nearest_existing_ancestor(&self, target: &Path) -> NodeId {
        let mut cur = Some(target);
        while let Some(p) = cur {
            if let Some(nid) = self.find_node_by_path(p) {
                let f = self.flags_of(nid);
                if !f.contains(NodeFlags::TOMBSTONE) && !f.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
                    return nid;
                }
            }
            cur = p.parent();
        }
        0
    }

    /// Ensures intermediate directory nodes exist for `rel_components` under root, returning parent id.
    pub fn ensure_dir_path(&mut self, target_dir: &Path) -> Option<NodeId> {
        let root_str = self.root_path.to_string_lossy().to_ascii_lowercase();
        let target_str = target_dir.to_string_lossy().to_ascii_lowercase();
        let root_trimmed = root_str.trim_end_matches('\\');
        let target_trimmed = target_str.trim_end_matches('\\');
        if target_trimmed == root_trimmed {
            return Some(0);
        }
        // Preserve original case from target_dir
        let orig_str = target_dir.to_string_lossy();
        let orig_trimmed = orig_str.trim_end_matches('\\');
        if !target_trimmed.starts_with(root_trimmed) {
            return None;
        }
        let rel = orig_trimmed
            .get(root_trimmed.len()..)
            .unwrap_or("")
            .trim_start_matches('\\');
        let mut current = 0u32;
        for seg in rel.split('\\').filter(|s| !s.is_empty()) {
            let mut found = None;
            for child in self.children_iter(current) {
                if self.node_name(child).eq_ignore_ascii_case(seg) {
                    found = Some(child);
                    break;
                }
            }
            if let Some(id) = found {
                current = id;
            } else {
                current = self.add_child(current, seg, NodeKind::Dir, 0, 0, 0, NodeFlags::empty());
                self.stats.total_dirs += 1;
            }
        }
        Some(current)
    }

    /// Searches node names case-insensitively, returning up to `limit` largest matching nodes.
    pub fn search_by_name(&self, query: &str, limit: usize) -> Vec<NodeId> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut matches: Vec<NodeId> = Vec::new();
        for idx in 1..self.len() {
            let nid = idx as NodeId;
            let f = self.flags_of(nid);
            if f.contains(NodeFlags::TOMBSTONE) || f.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
                continue;
            }
            let name = self.node_name(nid);
            if name.to_ascii_lowercase().contains(&q) {
                matches.push(nid);
            }
        }
        matches.sort_by(|&a, &b| {
            self.allocated_size[b as usize].cmp(&self.allocated_size[a as usize])
        });
        matches.truncate(limit);
        matches
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn apply_i64_u64(base: u64, delta: i64) -> u64 {
    if delta >= 0 {
        base.saturating_add(delta as u64)
    } else {
        base.saturating_sub(delta.unsigned_abs())
    }
}

fn apply_i64_u32(base: u32, delta: i64) -> u32 {
    if delta >= 0 {
        base.saturating_add(delta.min(u32::MAX as i64) as u32)
    } else {
        base.saturating_sub(delta.unsigned_abs().min(u32::MAX as u64) as u32)
    }
}

pub struct ChildIter<'a> {
    tree: &'a ScanTree,
    next: NodeId,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next != NONE_NODE {
            let cur = self.next;
            self.next = self.tree.next_sibling[cur as usize];
            let f = self.tree.flags_of(cur);
            if !f.contains(NodeFlags::TOMBSTONE) && !f.contains(NodeFlags::OPTIMISTIC_HIDDEN) {
                return Some(cur);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub drive_root: String,
    pub phase: String,
    pub engine_mode: ScanEngineMode,
    pub scanned_files: u64,
    pub scanned_dirs: u64,
    pub total_allocated_bytes: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
    pub mft_processed: Option<u64>,
    pub mft_total: Option<u64>,
    pub volume_used_bytes: Option<u64>,
    pub usn_processed: Option<u64>,
    pub usn_total: Option<u64>,
    pub progress_ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub root_path: String,
    pub fs_type: FileSystemType,
    pub stats: ScanStats,
    pub snapshot_id: Option<i64>,
}
