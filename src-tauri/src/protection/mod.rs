use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetLongPathNameW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_MULTI_SZ,
    REG_SZ, REG_VALUE_TYPE,
};

use crate::cleaner::guardrails::{is_reparse_point, normalize_lexical_lower};
use crate::dev_cleaner::resolver::expand_env_vars;
use crate::scanner::model::ScanTree;
use crate::storage::app_data_dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtectionType {
    Archivo,
    CarpetaRecursiva,
    Glob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtectionOrigin {
    #[serde(rename = "crítica")]
    Critica,
    Usuario,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEntry {
    pub id: String,
    pub patron: String,
    pub tipo: ProtectionType,
    pub origen: ProtectionOrigin,
    pub motivo: String,
    pub todas_las_unidades: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionResult {
    pub protected: bool,
    pub entry: Option<ProtectedEntry>,
    pub reason: Option<String>,
}

impl ProtectionResult {
    pub fn allowed() -> Self {
        Self {
            protected: false,
            entry: None,
            reason: None,
        }
    }

    pub fn blocked(entry: ProtectedEntry, reason: String) -> Self {
        Self {
            protected: true,
            entry: Some(entry),
            reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SystemException {
    pub id: String,
    pub base_dir_norm: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserProtectionFile {
    #[serde(default)]
    entries: Vec<ProtectedEntry>,
}

pub fn user_protection_config_path() -> PathBuf {
    app_data_dir().join("protected-paths.json")
}

/// Immutable factory critical entries + runtime paging files from Registry.
pub fn factory_critical_entries() -> Vec<ProtectedEntry> {
    let mut entries = vec![
        // Files across all drives
        ProtectedEntry {
            id: "crit-pagefile".to_string(),
            patron: "pagefile.sys".to_string(),
            tipo: ProtectionType::Archivo,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivo de paginación de Windows (memoria virtual).".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-swapfile".to_string(),
            patron: "swapfile.sys".to_string(),
            tipo: ProtectionType::Archivo,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivo de intercambio administrado por Windows para apps Store/UWP.".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-hiberfil".to_string(),
            patron: "hiberfil.sys".to_string(),
            tipo: ProtectionType::Archivo,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivo de hibernación e inicio rápido de Windows.".to_string(),
            todas_las_unidades: true,
        },
        // System folders
        ProtectedEntry {
            id: "crit-windows".to_string(),
            patron: "%SystemRoot%".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Directorio principal del sistema operativo Windows.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-program-files".to_string(),
            patron: "%ProgramFiles%".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Carpeta de aplicaciones instaladas (64 bits); desinstalar solo desde Windows.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-program-files-x86".to_string(),
            patron: "%ProgramFiles(x86)%".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Carpeta de aplicaciones instaladas (32 bits); desinstalar solo desde Windows.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-programdata".to_string(),
            patron: "%ProgramData%".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Datos compartidos del sistema y aplicaciones instaladas.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-svi".to_string(),
            patron: "System Volume Information".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Puntos de restauración y metadatos NTFS reservados por Windows.".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-recycle-bin".to_string(),
            patron: "$Recycle.Bin".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Estructura de la Papelera de reciclaje (vaciar solo con API oficial).".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-recovery".to_string(),
            patron: "Recovery".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Entorno de recuperación de Windows (WinRE).".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-boot".to_string(),
            patron: "Boot".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivos de arranque del sistema (BCD).".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-winreagent".to_string(),
            patron: "$WinREAgent".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Carpeta temporal de mantenimiento de WinRE.".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-windows-old".to_string(),
            patron: "Windows.old".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Instalación previa de Windows: limpiar solo con Sensor de almacenamiento / Liberador de espacio.".to_string(),
            todas_las_unidades: true,
        },
        ProtectedEntry {
            id: "crit-windows-bt".to_string(),
            patron: "$Windows.~BT".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivos de actualización de Windows: limpiar solo con herramientas nativas.".to_string(),
            todas_las_unidades: true,
        },
        // Boot files on system drive root
        ProtectedEntry {
            id: "crit-bootmgr".to_string(),
            patron: "%SystemDrive%\\bootmgr".to_string(),
            tipo: ProtectionType::Archivo,
            origen: ProtectionOrigin::Critica,
            motivo: "Gestor de arranque crítico de Windows.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-bootnxt".to_string(),
            patron: "%SystemDrive%\\BOOTNXT".to_string(),
            tipo: ProtectionType::Archivo,
            origen: ProtectionOrigin::Critica,
            motivo: "Archivo crítico de arranque de Windows.".to_string(),
            todas_las_unidades: false,
        },
        // User sensitive data
        ProtectedEntry {
            id: "crit-user-ssh".to_string(),
            patron: "%USERPROFILE%\\.ssh".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Claves privadas SSH del usuario.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-user-gnupg".to_string(),
            patron: "%USERPROFILE%\\.gnupg".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Claves privadas GPG/PGP del usuario.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-roaming-credentials".to_string(),
            patron: "%APPDATA%\\Microsoft\\Credentials".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Almacén de credenciales de Windows (Roaming).".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-local-credentials".to_string(),
            patron: "%LOCALAPPDATA%\\Microsoft\\Credentials".to_string(),
            tipo: ProtectionType::CarpetaRecursiva,
            origen: ProtectionOrigin::Critica,
            motivo: "Almacén de credenciales de Windows (Local).".to_string(),
            todas_las_unidades: false,
        },
        // Glob patterns
        ProtectedEntry {
            id: "crit-glob-git".to_string(),
            patron: r"**\.git".to_string(),
            tipo: ProtectionType::Glob,
            origen: ProtectionOrigin::Critica,
            motivo: "Historial y objetos de repositorio Git.".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-glob-env".to_string(),
            patron: r"**\.env*".to_string(),
            tipo: ProtectionType::Glob,
            origen: ProtectionOrigin::Critica,
            motivo: "Variables de entorno y secretos locales (.env).".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-glob-jks".to_string(),
            patron: r"**\*.jks".to_string(),
            tipo: ProtectionType::Glob,
            origen: ProtectionOrigin::Critica,
            motivo: "Almacén de claves de firma Java/Android (.jks).".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-glob-keystore".to_string(),
            patron: r"**\*.keystore".to_string(),
            tipo: ProtectionType::Glob,
            origen: ProtectionOrigin::Critica,
            motivo: "Almacén de claves de firma (.keystore).".to_string(),
            todas_las_unidades: false,
        },
        ProtectedEntry {
            id: "crit-glob-local-properties".to_string(),
            patron: r"**\local.properties".to_string(),
            tipo: ProtectionType::Glob,
            origen: ProtectionOrigin::Critica,
            motivo: "Configuración local de SDK Android (local.properties).".to_string(),
            todas_las_unidades: false,
        },
    ];

    // Append runtime paging files read from Registry
    for (idx, pf) in read_registry_paging_files().into_iter().enumerate() {
        let already = entries
            .iter()
            .any(|e| e.patron.eq_ignore_ascii_case(&pf.0) && e.todas_las_unidades == pf.1);
        if !already {
            entries.push(ProtectedEntry {
                id: format!("crit-reg-pagefile-{idx}"),
                patron: pf.0,
                tipo: ProtectionType::Archivo,
                origen: ProtectionOrigin::Critica,
                motivo: "Ubicación real de paginación registrada en Windows (Memory Management)."
                    .to_string(),
                todas_las_unidades: pf.1,
            });
        }
    }

    entries
}

/// Reads `PagingFiles` and `ExistingPageFiles` from
/// `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management`.
/// Returns `(path_or_rel_name, todas_las_unidades)`.
pub fn read_registry_paging_files() -> Vec<(String, bool)> {
    let mut results = Vec::new();
    let subkey: Vec<u16> = r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\0"
        .encode_utf16()
        .collect();
    let mut hkey = HKEY::default();
    let open_res = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if open_res.is_err() {
        return results;
    }

    for value_name in ["PagingFiles", "ExistingPageFiles"] {
        for raw_entry in query_reg_multi_or_sz(hkey, value_name) {
            if let Some(parsed) = parse_registry_pagefile_entry(&raw_entry) {
                if !results.iter().any(|(p, all): &(String, bool)| {
                    p.eq_ignore_ascii_case(&parsed.0) && *all == parsed.1
                }) {
                    results.push(parsed);
                }
            }
        }
    }

    unsafe {
        let _ = RegCloseKey(hkey);
    }
    results
}

pub fn parse_registry_pagefile_entry(raw: &str) -> Option<(String, bool)> {
    let first_token = raw.split_whitespace().next()?.trim();
    if first_token.is_empty() {
        return None;
    }
    let stripped = first_token
        .strip_prefix(r"\??\")
        .or_else(|| first_token.strip_prefix(r"\\?\"))
        .unwrap_or(first_token);

    if let Some(rel) = stripped.strip_prefix(r"?:\") {
        if !rel.is_empty() {
            return Some((rel.to_string(), true));
        }
    }
    Some((stripped.to_string(), false))
}

fn query_reg_multi_or_sz(hkey: HKEY, name: &str) -> Vec<String> {
    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut reg_type = REG_VALUE_TYPE(0);
    let mut byte_len = 0u32;

    let size_res = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut reg_type),
            None,
            Some(&mut byte_len),
        )
    };
    if size_res.is_err() || byte_len < 2 {
        return Vec::new();
    }

    let mut buf = vec![0u8; byte_len as usize];
    let read_res = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut reg_type),
            Some(buf.as_mut_ptr()),
            Some(&mut byte_len),
        )
    };
    if read_res.is_err() {
        return Vec::new();
    }

    let u16_slice: Vec<u16> = buf[..byte_len as usize]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();

    if reg_type == REG_MULTI_SZ {
        u16_slice
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    } else if reg_type == REG_SZ {
        let end = u16_slice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(u16_slice.len());
        let s = String::from_utf16_lossy(&u16_slice[..end]);
        if s.is_empty() {
            Vec::new()
        } else {
            vec![s]
        }
    } else {
        Vec::new()
    }
}

/// Code-only bounded system exceptions (never editable by the user).
/// User entries ALWAYS take precedence over these exceptions.
pub fn factory_system_exceptions() -> Vec<SystemException> {
    let mut out = Vec::new();
    let templates = [
        (
            "exc-win-temp",
            r"%SystemRoot%\Temp",
            "Temporales de Windows",
        ),
        (
            "exc-wer-archive",
            r"%ProgramData%\Microsoft\Windows\WER\ReportArchive",
            "Reportes WER archivados",
        ),
        (
            "exc-wer-queue",
            r"%ProgramData%\Microsoft\Windows\WER\ReportQueue",
            "Cola de reportes WER",
        ),
        (
            "exc-wer-temp",
            r"%ProgramData%\Microsoft\Windows\WER\Temp",
            "Temporales WER",
        ),
    ];
    for (id, tmpl, desc) in templates {
        if let Some(p) = expand_env_vars(tmpl) {
            out.push(SystemException {
                id: id.to_string(),
                base_dir_norm: normalize_for_match_raw(&p),
                description: desc.to_string(),
            });
        }
    }
    out
}

/// Converts a Windows 8.3 short path to its long path name if the path exists on disk.
pub fn to_long_path_if_exists(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.is_empty() {
        return path.to_path_buf();
    }
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let needed = unsafe { GetLongPathNameW(PCWSTR(wide.as_ptr()), None) };
    if needed == 0 {
        return path.to_path_buf();
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { GetLongPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
    if written == 0 || (written as usize) >= buf.len() {
        return path.to_path_buf();
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..written as usize]))
}

/// Normalizes a path string: expands `%VAR%`, strips `\??\` / `\\?\`, resolves `.` and `..`,
/// lowercases, removes trailing backslash (except `x:\`), and resolves 8.3 short names when present.
pub fn normalize_for_match_raw(path: &Path) -> String {
    let raw_str = path.to_string_lossy();
    let expanded = if raw_str.contains('%') {
        expand_env_vars(&raw_str).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let s = expanded.to_string_lossy();
    let stripped = s.strip_prefix(r"\??\").unwrap_or(&s);
    let long_p = to_long_path_if_exists(Path::new(stripped));
    normalize_lexical_lower(&long_p)
}

/// Returns both the original normalized path AND the resolved canonical path (following junctions/symlinks)
/// so a path can never bypass the blacklist through a junction or symlink.
pub fn candidate_normalized_forms(path: &Path) -> Vec<String> {
    let orig = normalize_for_match_raw(path);
    let mut forms = vec![orig.clone()];

    // Resolve via canonicalize (follows junctions & symlinks)
    let raw_str = path.to_string_lossy();
    let expanded = if raw_str.contains('%') {
        expand_env_vars(&raw_str).unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    if let Ok(canon) = fs::canonicalize(&expanded) {
        let long_c = to_long_path_if_exists(&canon);
        let norm_c = normalize_lexical_lower(&long_c);
        if !norm_c.is_empty() && norm_c != orig {
            forms.push(norm_c);
        }
    }
    forms
}

#[derive(Debug, Clone)]
pub struct ProtectionEngine {
    pub critical_entries: Vec<ProtectedEntry>,
    pub user_entries: Vec<ProtectedEntry>,
    pub system_exceptions: Vec<SystemException>,
}

impl ProtectionEngine {
    pub fn load_default() -> Self {
        Self::load_with_user_file(&user_protection_config_path())
    }

    pub fn load_with_user_file(user_file: &Path) -> Self {
        static CACHED_CRITICAL: OnceLock<Vec<ProtectedEntry>> = OnceLock::new();
        static CACHED_EXCEPTIONS: OnceLock<Vec<SystemException>> = OnceLock::new();

        let critical_entries = CACHED_CRITICAL
            .get_or_init(factory_critical_entries)
            .clone();
        let system_exceptions = CACHED_EXCEPTIONS
            .get_or_init(factory_system_exceptions)
            .clone();
        let critical_ids: HashSet<String> = critical_entries
            .iter()
            .map(|e| e.id.to_ascii_lowercase())
            .collect();

        let user_entries = if user_file.exists() {
            fs::read_to_string(user_file)
                .ok()
                .and_then(|s| serde_json::from_str::<UserProtectionFile>(&s).ok())
                .map(|f| {
                    // Strictly ignore any attempt to override/inject critical entries in user config
                    f.entries
                        .into_iter()
                        .filter(|e| {
                            e.origen == ProtectionOrigin::Usuario
                                && !critical_ids.contains(&e.id.to_ascii_lowercase())
                                && !e.patron.trim().is_empty()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            critical_entries,
            user_entries,
            system_exceptions,
        }
    }

    pub fn save_user_entries(&self, user_file: &Path) -> Result<(), String> {
        if let Some(parent) = user_file.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let only_user: Vec<ProtectedEntry> = self
            .user_entries
            .iter()
            .filter(|e| e.origen == ProtectionOrigin::Usuario)
            .cloned()
            .collect();
        let data = UserProtectionFile { entries: only_user };
        let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        fs::write(user_file, json).map_err(|e| e.to_string())
    }

    pub fn all_entries(&self) -> Vec<ProtectedEntry> {
        let mut all = self.critical_entries.clone();
        all.extend(self.user_entries.clone());
        all
    }

    /// Fast, non-I/O protection check for UI rendering (Treemap / TopEntriesTable).
    /// Evaluates exact/recursive/glob/concrete-descendant rules purely lexically so rendering
    /// hundreds of rows never blocks the UI thread with disk walks or `canonicalize()`.
    /// Deletion layers ALWAYS use the full `is_protected()` check (including 8.3, junctions,
    /// and recursive descendant scanning).
    pub fn is_protected_for_ui(&self, path: &Path) -> ProtectionResult {
        let form = normalize_lexical_lower(path);

        if let Some(entry) = self.match_entries(&form, &self.user_entries) {
            return ProtectionResult::blocked(
                entry.clone(),
                format!("Protegida por usuario: {}", entry.motivo),
            );
        }

        let inside_sys_exception = self.matches_system_exception(&form);

        if !inside_sys_exception {
            if let Some(entry) = self.match_entries(&form, &self.critical_entries) {
                return ProtectionResult::blocked(
                    entry.clone(),
                    format!("Ruta crítica protegida: {}", entry.motivo),
                );
            }
        } else if let Some(entry) = self.match_glob_entries(&form, &self.critical_entries) {
            return ProtectionResult::blocked(
                entry.clone(),
                format!("Patrón crítico protegido: {}", entry.motivo),
            );
        }

        if let Some(entry) = self.find_protected_concrete_descendant(&form, &self.user_entries) {
            return ProtectionResult::blocked(
                entry.clone(),
                format!(
                    "Contiene una ruta protegida por usuario ({}): {}",
                    entry.patron, entry.motivo
                ),
            );
        }
        if !inside_sys_exception {
            if let Some(entry) =
                self.find_protected_concrete_descendant(&form, &self.critical_entries)
            {
                return ProtectionResult::blocked(
                    entry.clone(),
                    format!(
                        "Contiene una ruta crítica protegida ({}): {}",
                        entry.patron, entry.motivo
                    ),
                );
            }
        }

        ProtectionResult::allowed()
    }

    /// Central protection check following strict precedence:
    /// 1. `usuario` entry (always blocks, even over a system exception)
    /// 2. `SystemException` (bounded code-only exception inside a critical root)
    /// 3. `crítica` entry
    ///
    /// Also blocks if deleting a directory would delete a protected descendant.
    pub fn is_protected(&self, path: &Path) -> ProtectionResult {
        let forms = candidate_normalized_forms(path);

        // 1. Check user entries first (highest precedence — overrides system exceptions)
        for form in &forms {
            if let Some(entry) = self.match_entries(form, &self.user_entries) {
                return ProtectionResult::blocked(
                    entry.clone(),
                    format!("Protegida por usuario: {}", entry.motivo),
                );
            }
        }

        // 2. Check bounded system exceptions (e.g. C:\Windows\Temp\*).
        // If ALL forms fall strictly inside a system exception (not the exception root itself
        // unless allowed, actually children inside exception dir), skip critical root match,
        // while STILL checking glob/descendant protections below!
        let inside_sys_exception = forms.iter().all(|f| self.matches_system_exception(f));

        // 3. Check critical entries
        if !inside_sys_exception {
            for form in &forms {
                if let Some(entry) = self.match_entries(form, &self.critical_entries) {
                    return ProtectionResult::blocked(
                        entry.clone(),
                        format!("Ruta crítica protegida: {}", entry.motivo),
                    );
                }
            }
        } else {
            // Even inside a system exception (like C:\Windows\Temp), still block critical globs
            // such as `.git`, `.env*`, `*.jks`, `*.keystore` if someone placed them there.
            for form in &forms {
                if let Some(entry) = self.match_glob_entries(form, &self.critical_entries) {
                    return ProtectionResult::blocked(
                        entry.clone(),
                        format!("Patrón crítico protegido: {}", entry.motivo),
                    );
                }
            }
        }

        // 4. Containment check: would deleting `path` delete a protected entry inside it?
        for form in &forms {
            if let Some(entry) = self.find_protected_concrete_descendant(form, &self.user_entries) {
                return ProtectionResult::blocked(
                    entry.clone(),
                    format!(
                        "Contiene una ruta protegida por usuario ({}): {}",
                        entry.patron, entry.motivo
                    ),
                );
            }
            if !inside_sys_exception {
                if let Some(entry) =
                    self.find_protected_concrete_descendant(form, &self.critical_entries)
                {
                    return ProtectionResult::blocked(
                        entry.clone(),
                        format!(
                            "Contiene una ruta crítica protegida ({}): {}",
                            entry.patron, entry.motivo
                        ),
                    );
                }
            }
        }

        // 5. If target is an existing directory, scan bounded for protected glob descendants (e.g. `.git`, `.env`)
        if path.is_dir() && !is_reparse_point(path) {
            if let Some((entry, found_path)) = self.find_protected_glob_in_dir(path) {
                return ProtectionResult::blocked(
                    entry.clone(),
                    format!(
                        "Contiene elemento protegido '{}' ({}): {}",
                        found_path.display(),
                        entry.patron,
                        entry.motivo
                    ),
                );
            }
        }

        ProtectionResult::allowed()
    }

    fn matches_system_exception(&self, norm_lower: &str) -> bool {
        for exc in &self.system_exceptions {
            let prefix = format!(r"{}\", exc.base_dir_norm.trim_end_matches('\\'));
            if norm_lower.starts_with(&prefix) {
                return true;
            }
        }
        false
    }

    fn match_entries<'a>(
        &self,
        norm_lower: &str,
        entries: &'a [ProtectedEntry],
    ) -> Option<&'a ProtectedEntry> {
        entries
            .iter()
            .find(|entry| entry_matches_normalized(entry, norm_lower))
    }

    fn match_glob_entries<'a>(
        &self,
        norm_lower: &str,
        entries: &'a [ProtectedEntry],
    ) -> Option<&'a ProtectedEntry> {
        entries.iter().find(|entry| {
            entry.tipo == ProtectionType::Glob && entry_matches_normalized(entry, norm_lower)
        })
    }

    fn find_protected_concrete_descendant<'a>(
        &self,
        dir_norm_lower: &str,
        entries: &'a [ProtectedEntry],
    ) -> Option<&'a ProtectedEntry> {
        let dir_prefix = if dir_norm_lower.ends_with('\\') {
            dir_norm_lower.to_string()
        } else {
            format!(r"{dir_norm_lower}\")
        };

        for entry in entries {
            if entry.tipo == ProtectionType::Glob {
                continue;
            }
            if entry.todas_las_unidades {
                // If dir is a drive root like `d:\`, it contains `d:\<patron>`
                if dir_norm_lower.len() == 3 && dir_norm_lower.ends_with(r":\") {
                    return Some(entry);
                }
                continue;
            }
            let Some(entry_norm) = resolve_entry_pattern_norm(&entry.patron) else {
                continue;
            };
            if entry_norm.starts_with(&dir_prefix) {
                return Some(entry);
            }
        }
        None
    }

    fn find_protected_glob_in_dir(&self, root_dir: &Path) -> Option<(ProtectedEntry, PathBuf)> {
        let mut glob_entries: Vec<&ProtectedEntry> = self
            .user_entries
            .iter()
            .chain(self.critical_entries.iter())
            .filter(|e| e.tipo == ProtectionType::Glob)
            .collect();
        if glob_entries.is_empty() {
            return None;
        }
        // Keep user globs before critical globs
        glob_entries.sort_by_key(|e| {
            if e.origen == ProtectionOrigin::Usuario {
                0
            } else {
                1
            }
        });

        let mut stack = vec![(root_dir.to_path_buf(), 0usize)];
        let mut visited = 0usize;
        const MAX_DEPTH: usize = 6;
        const MAX_VISITED: usize = 5_000;

        while let Some((dir, depth)) = stack.pop() {
            if depth > MAX_DEPTH || visited >= MAX_VISITED || is_reparse_point(&dir) {
                continue;
            }
            let Ok(rd) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in rd.flatten() {
                visited += 1;
                if visited >= MAX_VISITED {
                    break;
                }
                let p = entry.path();
                let norm = normalize_lexical_lower(&p);
                for rule in &glob_entries {
                    if entry_matches_normalized(rule, &norm) {
                        return Some(((*rule).clone(), p));
                    }
                }
                if !is_reparse_point(&p) {
                    if let Ok(meta) = fs::symlink_metadata(&p) {
                        if meta.is_dir() && !meta.file_type().is_symlink() {
                            stack.push((p, depth + 1));
                        }
                    }
                }
            }
        }
        None
    }

    /// Counts how many nodes in the active `ScanTree` would match a candidate entry.
    pub fn preview_matches_in_tree(tree: &ScanTree, entry: &ProtectedEntry) -> usize {
        let mut count = 0usize;
        for idx in 1..tree.nodes.len() {
            let p = tree.reconstruct_path(idx as u32);
            let norm = normalize_lexical_lower(&p);
            if entry_matches_normalized(entry, &norm) {
                count += 1;
            }
        }
        count
    }
}

fn resolve_entry_pattern_norm(patron: &str) -> Option<String> {
    let trimmed = patron.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = if trimmed.contains('%') {
        expand_env_vars(trimmed)?
    } else {
        PathBuf::from(trimmed)
    };
    Some(normalize_for_match_raw(&path))
}

fn strip_drive_prefix(norm_lower: &str) -> Option<&str> {
    if norm_lower.len() >= 3
        && norm_lower.as_bytes()[0].is_ascii_alphabetic()
        && &norm_lower[1..3] == r":\"
    {
        Some(&norm_lower[3..])
    } else {
        None
    }
}

pub fn entry_matches_normalized(entry: &ProtectedEntry, norm_lower: &str) -> bool {
    match entry.tipo {
        ProtectionType::Archivo => {
            if entry.todas_las_unidades {
                let rel_pat = entry
                    .patron
                    .trim()
                    .trim_start_matches(['\\', '/'])
                    .replace('/', "\\")
                    .to_ascii_lowercase();
                if let Some(rel_target) = strip_drive_prefix(norm_lower) {
                    return rel_target == rel_pat;
                }
                false
            } else {
                let Some(pat_norm) = resolve_entry_pattern_norm(&entry.patron) else {
                    return false;
                };
                norm_lower == pat_norm
            }
        }
        ProtectionType::CarpetaRecursiva => {
            if entry.todas_las_unidades {
                let rel_pat = entry
                    .patron
                    .trim()
                    .trim_matches(['\\', '/'])
                    .replace('/', "\\")
                    .to_ascii_lowercase();
                if let Some(rel_target) = strip_drive_prefix(norm_lower) {
                    return rel_target == rel_pat
                        || rel_target.starts_with(&format!(r"{rel_pat}\"));
                }
                false
            } else {
                let Some(pat_norm) = resolve_entry_pattern_norm(&entry.patron) else {
                    return false;
                };
                let pat_clean = pat_norm.trim_end_matches('\\');
                norm_lower == pat_clean || norm_lower.starts_with(&format!(r"{pat_clean}\"))
            }
        }
        ProtectionType::Glob => glob_matches_path(&entry.patron, norm_lower),
    }
}

/// Supports glob patterns such as `**\.git`, `**\.env*`, `**\*.jks`, `C:\dev\**\secret*`, etc.
/// If a glob matches a directory segment (e.g. `**\.git`), it also protects all descendants
/// inside that directory (`C:\proj\.git\config`).
pub fn glob_matches_path(pattern: &str, norm_lower: &str) -> bool {
    let expanded = if pattern.contains('%') {
        expand_env_vars(pattern)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| pattern.to_string())
    } else {
        pattern.to_string()
    };
    let pat_norm = expanded
        .trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    if pat_norm.is_empty() {
        return false;
    }

    let segments: Vec<&str> = norm_lower.split('\\').filter(|s| !s.is_empty()).collect();

    // Fast path for common `**\<segment_glob>` patterns: matches if ANY path segment matches
    // (which also automatically covers everything inside a matched folder like `**\.git`).
    if let Some(seg_pat) = pat_norm.strip_prefix(r"**\") {
        if !seg_pat.contains('\\') {
            return segments.iter().any(|&seg| wildcard_match(seg_pat, seg));
        }
    }

    // General path-level glob matching on the target or any of its ancestor prefixes
    if wildcard_path_match(&pat_norm, norm_lower) {
        return true;
    }
    let mut cur = norm_lower;
    while let Some(pos) = cur.rfind('\\') {
        cur = &cur[..pos];
        if cur.len() <= 2 {
            break;
        }
        if wildcard_path_match(&pat_norm, cur) {
            return true;
        }
    }
    false
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_idx = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(si) = star_idx {
            pi = si + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn wildcard_path_match(pattern: &str, path: &str) -> bool {
    let pat_parts: Vec<&str> = pattern.split('\\').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('\\').filter(|s| !s.is_empty()).collect();
    match_segments(&pat_parts, &path_parts)
}

fn match_segments(pat: &[&str], path: &[&str]) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    if pat[0] == "**" {
        // Match 0 or more segments
        if match_segments(&pat[1..], path) {
            return true;
        }
        if !path.is_empty() && match_segments(pat, &path[1..]) {
            return true;
        }
        return false;
    }
    if path.is_empty() {
        return false;
    }
    if wildcard_match(pat[0], path[0]) {
        match_segments(&pat[1..], &path[1..])
    } else {
        false
    }
}

/// Convenience entry point using the default persisted user configuration + factory critical rules.
pub fn is_protected_default(path: &Path) -> ProtectionResult {
    let engine = ProtectionEngine::load_default();
    engine.is_protected(path)
}
