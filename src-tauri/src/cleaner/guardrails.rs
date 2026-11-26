use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailViolation {
    pub path: PathBuf,
    pub reason: String,
}

fn strip_extended_prefix(s: &str) -> String {
    let lower = s.replace('/', "\\");
    if let Some(rest) = lower.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = lower.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        lower
    }
}

/// Normalizes a Windows path lexically (resolving `.` and `..` without following symlinks)
/// and returns a lowercased string without trailing backslash (except drive root like `c:\`).
pub fn normalize_lexical_lower(path: &Path) -> String {
    let raw = strip_extended_prefix(&path.to_string_lossy());
    let mut prefix = String::new();
    let mut parts: Vec<String> = Vec::new();

    let mut rest = raw.as_str();
    if rest.len() >= 2 && rest.as_bytes()[1] == b':' {
        prefix = format!("{}\\", rest[..2].to_ascii_lowercase());
        rest = &rest[2..];
    } else if let Some(unc) = rest.strip_prefix(r"\\") {
        prefix = r"\\".to_string();
        rest = unc;
    }

    for seg in rest.split('\\') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            parts.pop();
            continue;
        }
        parts.push(seg.to_ascii_lowercase());
    }

    if parts.is_empty() {
        prefix
    } else if prefix.ends_with('\\') {
        format!("{prefix}{}", parts.join("\\"))
    } else {
        format!("{prefix}\\{}", parts.join("\\"))
    }
}

fn canonical_or_lexical_lower(path: &Path) -> String {
    if let Ok(canon) = std::fs::canonicalize(path) {
        normalize_lexical_lower(&canon)
    } else {
        normalize_lexical_lower(path)
    }
}

pub fn is_reparse_point(path: &Path) -> bool {
    let s = path.as_os_str().to_string_lossy();
    let extended = if s.starts_with(r"\\?\") {
        s.into_owned()
    } else {
        format!(r"\\?\{s}")
    };
    let wide: Vec<u16> = extended.encode_utf16().chain(std::iter::once(0)).collect();
    let attrs = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
    attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0
}

fn protected_roots() -> Vec<String> {
    let mut roots = vec![
        r"c:\windows".to_string(),
        r"c:\windows\system32".to_string(),
        r"c:\windows\syswow64".to_string(),
        r"c:\windows\winsxs".to_string(),
        r"c:\program files".to_string(),
        r"c:\program files (x86)".to_string(),
        r"c:\programdata".to_string(),
        r"c:\users".to_string(),
        r"c:\recovery".to_string(),
        r"c:\boot".to_string(),
        r"c:\system volume information".to_string(),
    ];

    if let Ok(win_dir) = std::env::var("SystemRoot") {
        let w = normalize_lexical_lower(Path::new(&win_dir));
        roots.push(w.clone());
        roots.push(format!(r"{w}\system32"));
        roots.push(format!(r"{w}\syswow64"));
        roots.push(format!(r"{w}\winsxs"));
    }

    if let Ok(profile) = std::env::var("USERPROFILE") {
        let p = normalize_lexical_lower(Path::new(&profile));
        roots.push(p.clone());
        for sub in ["desktop", "documents", "pictures", "videos", "music"] {
            roots.push(format!(r"{p}\{sub}"));
        }
    }

    roots.sort();
    roots.dedup();
    roots
}

fn is_system_critical_file(norm_lower: &str) -> bool {
    let file_name = norm_lower.rsplit('\\').next().unwrap_or("");
    matches!(
        file_name,
        "pagefile.sys" | "hiberfil.sys" | "swapfile.sys" | "bootmgr" | "ntldr"
    )
}

fn is_drive_root(norm_lower: &str) -> bool {
    norm_lower.len() == 3
        && norm_lower.as_bytes()[0].is_ascii_alphabetic()
        && &norm_lower[1..] == r":\"
}

pub fn validate_target_path(path: &Path) -> Result<(), GuardrailViolation> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Segmento '..' no permitido en rutas de limpieza.".to_string(),
        });
    }

    let lex_norm = normalize_lexical_lower(path);
    if lex_norm.is_empty() || is_drive_root(&lex_norm) {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "No se puede eliminar la raíz de una unidad.".to_string(),
        });
    }

    if is_system_critical_file(&lex_norm) {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Archivo crítico del sistema protegido.".to_string(),
        });
    }

    if lex_norm.ends_with(r"\$recycle.bin") || lex_norm.contains(r"\system volume information") {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Ruta reservada del sistema protegida.".to_string(),
        });
    }

    // Never follow symlinks or junctions on deletion
    if path.exists() && is_reparse_point(path) {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Reparse point (symlink/junction) bloqueado por seguridad.".to_string(),
        });
    }

    // Check any intermediate ancestor that might be a symlink/junction pointing to protected paths
    let resolved_norm = canonical_or_lexical_lower(path);
    let protected = protected_roots();

    for check in [&lex_norm, &resolved_norm] {
        if is_drive_root(check) || is_system_critical_file(check) {
            return Err(GuardrailViolation {
                path: path.to_path_buf(),
                reason: "Resuelve a una ruta crítica protegida.".to_string(),
            });
        }
        for p in &protected {
            if check == p {
                return Err(GuardrailViolation {
                    path: path.to_path_buf(),
                    reason: format!("Ruta protegida del sistema: {p}"),
                });
            }
            // Block anything inside System32, SysWOW64, WinSxS, Program Files, Recovery, Boot
            let is_strict_subtree = p.ends_with(r"\system32")
                || p.ends_with(r"\syswow64")
                || p.ends_with(r"\winsxs")
                || p.ends_with(r"\program files")
                || p.ends_with(r"\program files (x86)")
                || p.ends_with(r"\recovery")
                || p.ends_with(r"\boot")
                || p.ends_with(r"\system volume information");

            if is_strict_subtree && check.starts_with(&format!(r"{p}\")) {
                return Err(GuardrailViolation {
                    path: path.to_path_buf(),
                    reason: format!("Subruta protegida dentro de {p}"),
                });
            }
            // For C:\Windows allow ONLY C:\Windows\Temp\* explicitly
            if p.ends_with(r"\windows") && check.starts_with(&format!(r"{p}\")) {
                let win_temp_prefix = format!(r"{p}\temp\");
                if !check.starts_with(&win_temp_prefix) {
                    return Err(GuardrailViolation {
                        path: path.to_path_buf(),
                        reason: "Solo C:\\Windows\\Temp es limpiable dentro de Windows."
                            .to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}

/// Strictly asserts during tests that no cleanup operation can ever escape the test sandbox.
pub struct SandboxGuard {
    sandbox_root_norm: String,
}

impl SandboxGuard {
    pub fn new(sandbox_root: &Path) -> Self {
        Self {
            sandbox_root_norm: canonical_or_lexical_lower(sandbox_root),
        }
    }

    pub fn verify_inside_sandbox(&self, target: &Path) -> Result<(), GuardrailViolation> {
        validate_target_path(target)?;
        let target_norm = canonical_or_lexical_lower(target);
        let prefix = format!(r"{}\", self.sandbox_root_norm.trim_end_matches('\\'));
        if !target_norm.starts_with(&prefix) {
            return Err(GuardrailViolation {
                path: target.to_path_buf(),
                reason: "SandboxGuard: ruta fuera del TempDir de pruebas.".to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_all_protected_system_and_user_roots() {
        let forbidden = [
            r"C:\",
            r"c:\",
            r"C:\Windows",
            r"C:\WINDOWS\System32",
            r"\\?\C:\Windows\System32\kernel32.dll",
            r"C:\Windows\WinSxS\x86_something",
            r"C:\Windows\Temp\..\System32\drivers",
            r"C:\Windows\Temp\..\..\System32\drivers",
            r"C:\Program Files",
            r"C:\Program Files\MyApp\app.exe",
            r"C:\Program Files (x86)",
            r"C:\Users",
            r"C:\pagefile.sys",
            r"C:\hiberfil.sys",
            r"C:\System Volume Information",
        ];

        for path in forbidden {
            let res = validate_target_path(Path::new(path));
            assert!(res.is_err(), "Expected {path} to be blocked!");
        }
    }

    #[test]
    fn allows_windows_temp_children_and_user_temp_children() {
        assert!(validate_target_path(Path::new(r"C:\Windows\Temp\old_installer.tmp")).is_ok());
        assert!(validate_target_path(Path::new(r"D:\Projects\app\node_modules")).is_ok());
    }
}
