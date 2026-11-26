use std::path::Path;

use crate::cleaner::guardrails::{
    normalize_lexical_lower, validate_target_path, GuardrailViolation,
};

use super::schema::{BlacklistNeverSpec, CleanMethodKind, DevRule, SafetyLevel};

/// Evaluates whether a path is unconditionally forbidden (level `nunca`).
/// This check prevails over every rule in the catalog.
pub fn validate_dev_target_path(
    path: &Path,
    spec: &BlacklistNeverSpec,
) -> Result<(), GuardrailViolation> {
    // 1. Always enforce base system guardrails (drive roots, Windows, System32, Program Files,
    //    User profile roots, .., reparse points / symlinks / junctions).
    validate_target_path(path)?;

    let norm = normalize_lexical_lower(path);
    let segments: Vec<&str> = norm.split('\\').filter(|s| !s.is_empty()).collect();
    let leaf = segments.last().copied().unwrap_or("");

    // 2. Exact forbidden names anywhere in the path (e.g. `.git`, `local.properties`)
    for exact in &spec.exact_names {
        let exact_low = exact.to_ascii_lowercase();
        if segments.iter().any(|&seg| seg == exact_low) {
            return Err(GuardrailViolation {
                path: path.to_path_buf(),
                reason: format!("Lista negra 'nunca': ruta protegida ({exact})."),
            });
        }
    }

    // 3. Forbidden glob patterns on any segment (`.env`, `.env.*`, `*.jks`, `*.keystore`)
    for pat in &spec.glob_patterns {
        let pat_low = pat.to_ascii_lowercase();
        if segments
            .iter()
            .any(|&seg| matches_simple_glob(&pat_low, seg))
        {
            return Err(GuardrailViolation {
                path: path.to_path_buf(),
                reason: format!("Lista negra 'nunca': coincide con patrón protegido ({pat})."),
            });
        }
    }

    // 4. Local database directories
    for db_name in &spec.db_dir_names {
        let db_low = db_name.to_ascii_lowercase();
        if segments.iter().any(|&seg| seg == db_low) {
            return Err(GuardrailViolation {
                path: path.to_path_buf(),
                reason: format!("Lista negra 'nunca': carpeta de base de datos local ({db_name})."),
            });
        }
    }
    if norm.contains(r"\data\db") {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Lista negra 'nunca': carpeta de datos de MongoDB (data\\db).".to_string(),
        });
    }

    // 5. Docker volumes path protection
    if spec.forbid_docker_volumes
        && (norm.contains(r"\docker\volumes")
            || norm.contains(r"\docker-desktop-data")
            || norm.ends_with(".vhdx"))
    {
        return Err(GuardrailViolation {
            path: path.to_path_buf(),
            reason: "Lista negra 'nunca': volúmenes o discos virtuales de Docker/WSL protegidos."
                .to_string(),
        });
    }

    // 6. Source code files can never be targeted directly
    if let Some(ext) = Path::new(leaf).extension().and_then(|e| e.to_str()) {
        let ext_low = ext.to_ascii_lowercase();
        if spec
            .source_extensions
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&ext_low))
        {
            return Err(GuardrailViolation {
                path: path.to_path_buf(),
                reason: format!("Lista negra 'nunca': archivo de código fuente (.{ext_low})."),
            });
        }
    }

    Ok(())
}

/// Validates that a rule + optional command cannot violate level `nunca` invariants.
pub fn validate_dev_rule_and_command(
    rule: &DevRule,
    command: Option<&[String]>,
    spec: &BlacklistNeverSpec,
) -> Result<(), String> {
    if rule.nivel == SafetyLevel::Nunca {
        return Err(format!(
            "Regla '{}' tiene nivel 'nunca' y no puede ejecutarse.",
            rule.id
        ));
    }

    if let Some(cmd) = command {
        if spec.forbid_docker_volumes {
            let is_docker = cmd.first().is_some_and(|bin| {
                bin.eq_ignore_ascii_case("docker") || bin.eq_ignore_ascii_case("docker.exe")
            });
            if is_docker {
                let has_forbidden = cmd.iter().any(|arg| {
                    let a = arg.to_ascii_lowercase();
                    a == "--volumes" || a == "-v" || a == "volume"
                });
                if has_forbidden {
                    return Err(
                        "Lista negra 'nunca': prohibido usar --volumes o docker volume prune."
                            .to_string(),
                    );
                }
            }
        }
    }

    // Never allow manual deletion of Android SDK directories
    if rule.id == "android-sdk-unused" && rule.metodo.tipo != CleanMethodKind::ComandoOficial {
        return Err(
            "Android SDK solo puede limpiarse mediante `sdkmanager --uninstall`, nunca borrado manual."
                .to_string(),
        );
    }

    Ok(())
}

pub fn matches_simple_glob(pattern_lower: &str, text_lower: &str) -> bool {
    if pattern_lower == text_lower {
        return true;
    }
    if let Some(suffix) = pattern_lower.strip_prefix('*') {
        return text_lower.ends_with(suffix);
    }
    if let Some(prefix) = pattern_lower.strip_suffix('*') {
        return text_lower.starts_with(prefix);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_cleaner::schema::DevRulesCatalog;

    #[test]
    fn never_list_blocks_all_forbidden_dev_and_system_paths() {
        let catalog = DevRulesCatalog::load_embedded().expect("valid catalog");
        let spec = &catalog.blacklist_never;

        let forbidden = [
            r"D:\proyectos\mi-app\.git",
            r"D:\proyectos\mi-app\.git\config",
            r"D:\proyectos\mi-app\.env",
            r"D:\proyectos\mi-app\.env.local",
            r"D:\proyectos\mi-app\.env.production",
            r"D:\proyectos\android\app\release.jks",
            r"D:\proyectos\android\debug.keystore",
            r"D:\proyectos\android\local.properties",
            r"D:\proyectos\mi-app\src\main.rs",
            r"D:\proyectos\mi-app\index.ts",
            r"D:\proyectos\mi-app\pgdata",
            r"D:\proyectos\mi-app\data\db",
            r"C:\Users\dev\AppData\Local\Docker\wsl\data\ext4.vhdx",
            r"C:\Windows\System32",
            r"C:\",
            r"D:\proyectos\mi-app\node_modules\..\..\.git",
        ];

        for p in forbidden {
            assert!(
                validate_dev_target_path(Path::new(p), spec).is_err(),
                "Expected path to be blocked by blacklist 'nunca': {p}"
            );
        }
    }

    #[test]
    fn allows_legitimate_dev_artifacts_and_caches() {
        let catalog = DevRulesCatalog::load_embedded().expect("valid catalog");
        let spec = &catalog.blacklist_never;

        let allowed = [
            r"D:\proyectos\mi-app\node_modules",
            r"D:\proyectos\mi-app\.next",
            r"D:\proyectos\php-app\vendor",
            r"D:\proyectos\py-app\.venv",
            r"C:\Users\dev\AppData\Local\npm-cache",
            r"C:\Users\dev\.gradle\caches",
            r"C:\Windows\Temp\installer_123",
        ];

        for p in allowed {
            assert!(
                validate_dev_target_path(Path::new(p), spec).is_ok(),
                "Expected path to be allowed: {p}"
            );
        }
    }

    #[test]
    fn blocks_docker_volumes_command() {
        let catalog = DevRulesCatalog::load_embedded().expect("valid catalog");
        let docker_rule = catalog
            .rules
            .iter()
            .find(|r| r.id == "docker-builder-prune")
            .unwrap();
        let bad_cmd = vec![
            "docker".to_string(),
            "system".to_string(),
            "prune".to_string(),
            "--volumes".to_string(),
        ];
        assert!(validate_dev_rule_and_command(
            docker_rule,
            Some(&bad_cmd),
            &catalog.blacklist_never
        )
        .is_err());
    }
}
