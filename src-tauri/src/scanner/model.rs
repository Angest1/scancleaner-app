use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemType {
    Ntfs,
    Fat32,
    ExFat,
    Refs,
    Network,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Dir,
    File,
    Symlink,
    Junction,
    CloudPlaceholder,
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct NodeFlags: u8 {
        const HARDLINK_DUP = 1 << 0;
        const COMPRESSED   = 1 << 1;
        const SPARSE       = 1 << 2;
        const CLOUD_ONLY   = 1 << 3;
        const PROTECTED    = 1 << 4;
    }
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
    pub access_denied_count: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone)]
pub struct FsNode {
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub name_offset: u32,
    pub name_len: u16,
    pub kind: NodeKind,
    pub logical_size: u64,
    pub allocated_size: u64,
    pub last_write_ts: i64,
    pub file_count: u32,
    pub type_cat: TypeCategory,
    pub origin_cat: OriginCategory,
    pub flags: NodeFlags,
}

#[derive(Debug, Clone)]
pub struct ScanTree {
    pub root_path: PathBuf,
    pub volume_serial: u32,
    pub fs_type: FileSystemType,
    pub nodes: Vec<FsNode>,
    pub name_arena: String,
    pub access_denied: Vec<PathBuf>,
    pub stats: ScanStats,
}

impl ScanTree {
    pub fn new(root_path: PathBuf, volume_serial: u32, fs_type: FileSystemType) -> Self {
        let mut tree = Self {
            root_path: root_path.clone(),
            volume_serial,
            fs_type,
            nodes: Vec::with_capacity(4096),
            name_arena: String::with_capacity(64 * 1024),
            access_denied: Vec::new(),
            stats: ScanStats::default(),
        };
        let root_name = root_path.to_string_lossy();
        let (name_offset, name_len) = tree.intern_name(&root_name);
        tree.nodes.push(FsNode {
            parent: None,
            first_child: None,
            next_sibling: None,
            name_offset,
            name_len,
            kind: NodeKind::Dir,
            logical_size: 0,
            allocated_size: 0,
            last_write_ts: 0,
            file_count: 0,
            type_cat: TypeCategory::Other,
            origin_cat: OriginCategory::UserOther,
            flags: NodeFlags::empty(),
        });
        tree
    }

    pub fn intern_name(&mut self, name: &str) -> (u32, u16) {
        let offset = self.name_arena.len() as u32;
        let len = name.len().min(u16::MAX as usize) as u16;
        self.name_arena.push_str(&name[..len as usize]);
        (offset, len)
    }

    pub fn node_name(&self, id: NodeId) -> &str {
        let node = &self.nodes[id as usize];
        let start = node.name_offset as usize;
        let end = start + node.name_len as usize;
        &self.name_arena[start..end]
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
        let (name_offset, name_len) = self.intern_name(name);
        let id = self.nodes.len() as NodeId;
        let prev_first = self.nodes[parent as usize].first_child;
        self.nodes.push(FsNode {
            parent: Some(parent),
            first_child: None,
            next_sibling: prev_first,
            name_offset,
            name_len,
            kind,
            logical_size,
            allocated_size,
            last_write_ts,
            file_count: u32::from(matches!(kind, NodeKind::File | NodeKind::CloudPlaceholder)),
            type_cat: TypeCategory::Other,
            origin_cat: OriginCategory::UserOther,
            flags,
        });
        self.nodes[parent as usize].first_child = Some(id);
        id
    }

    pub fn reconstruct_path(&self, id: NodeId) -> PathBuf {
        if id == 0 {
            return self.root_path.clone();
        }
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = Some(id);
        while let Some(nid) = cur {
            if nid == 0 {
                break;
            }
            parts.push(self.node_name(nid));
            cur = self.nodes[nid as usize].parent;
        }
        let mut path = self.root_path.clone();
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path
    }

    pub fn children_iter(&self, parent: NodeId) -> ChildIter<'_> {
        ChildIter {
            tree: self,
            next: self.nodes[parent as usize].first_child,
        }
    }

    /// Aggregates sizes and file counts bottom-up across the arena.
    /// Since children are always appended after their parents, iterating in reverse index order
    /// guarantees children are aggregated before their parents.
    pub fn aggregate_bottom_up(&mut self) {
        let mut total_files: u64 = 0;
        let mut total_dirs: u64 = 0;

        for idx in (0..self.nodes.len()).rev() {
            let node = &self.nodes[idx];
            match node.kind {
                NodeKind::Dir => {
                    total_dirs += 1;
                }
                NodeKind::File | NodeKind::CloudPlaceholder => {
                    total_files += 1;
                }
                NodeKind::Symlink | NodeKind::Junction => {}
            }
            if let Some(parent_id) = node.parent {
                let logical = node.logical_size;
                let allocated = node.allocated_size;
                let files = node.file_count;
                let ts = node.last_write_ts;
                let p = &mut self.nodes[parent_id as usize];
                p.logical_size = p.logical_size.saturating_add(logical);
                p.allocated_size = p.allocated_size.saturating_add(allocated);
                p.file_count = p.file_count.saturating_add(files);
                if ts > p.last_write_ts {
                    p.last_write_ts = ts;
                }
            }
        }

        if let Some(root) = self.nodes.first() {
            self.stats.total_logical_bytes = root.logical_size;
            self.stats.total_allocated_bytes = root.allocated_size;
        }
        self.stats.total_files = total_files;
        self.stats.total_dirs = total_dirs;
        self.stats.access_denied_count = self.access_denied.len() as u64;
    }

    pub fn find_node_by_path(&self, target: &std::path::Path) -> Option<NodeId> {
        let root_str = self.root_path.to_string_lossy().to_ascii_lowercase();
        let target_str = target.to_string_lossy().to_ascii_lowercase();
        let root_trimmed = root_str.trim_end_matches('\\');
        let target_trimmed = target_str.trim_end_matches('\\');

        if target_trimmed == root_trimmed {
            return Some(0);
        }
        let rel = target.strip_prefix(&self.root_path).ok()?;
        let mut current: NodeId = 0;
        for comp in rel.components() {
            let name = comp.as_os_str().to_string_lossy();
            let mut found = None;
            for child in self.children_iter(current) {
                if self.node_name(child).eq_ignore_ascii_case(&name) {
                    found = Some(child);
                    break;
                }
            }
            current = found?;
        }
        Some(current)
    }
}

pub struct ChildIter<'a> {
    tree: &'a ScanTree,
    next: Option<NodeId>,
}

impl<'a> Iterator for ChildIter<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = self.next?;
        self.next = self.tree.nodes[cur as usize].next_sibling;
        Some(cur)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub scanned_files: u64,
    pub scanned_dirs: u64,
    pub total_allocated_bytes: u64,
    pub current_path: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub root_path: String,
    pub fs_type: FileSystemType,
    pub stats: ScanStats,
    pub snapshot_id: Option<i64>,
}
