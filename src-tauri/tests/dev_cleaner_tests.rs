use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;

use scancleaner_lib::cleaner::guardrails::SandboxGuard;
use scancleaner_lib::dev_cleaner::blacklist::validate_dev_target_path;
use scancleaner_lib::dev_cleaner::executor::{build_dev_cleanup_plan, execute_dev_cleanup_plan};
use scancleaner_lib::dev_cleaner::process::{CommandOutput, CommandRunner};
use scancleaner_lib::dev_cleaner::project_scanner::scan_project_artifacts;
use scancleaner_lib::dev_cleaner::resolver::parse_cli_path_stdout;
use scancleaner_lib::dev_cleaner::schema::{
    CleanMethodKind, DevCandidate, DevCleanerConfig, DevPreviewItemInput, DevPreviewRequest,
    DevRulesCatalog, RuleCategory, RuleKind, SafetyLevel,
};
use scancleaner_lib::storage::read_cleanup_logs;

#[derive(Default)]
struct MockCommandRunner {
    running: HashSet<String>,
    calls: Mutex<Vec<Vec<String>>>,
    git_ignored_paths: HashSet<String>,
    git_commit_ts: Option<i64>,
    docker_reclaimed_output: Option<String>,
}

impl MockCommandRunner {
    fn recorded_calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl CommandRunner for MockCommandRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _timeout: Duration,
    ) -> CommandOutput {
        let mut full = vec![program.to_string()];
        full.extend(args.iter().map(|s| (*s).to_string()));
        self.calls.lock().unwrap().push(full);

        if program == "git" && args.first() == Some(&"check-ignore") {
            let target = args.last().copied().unwrap_or("").to_ascii_lowercase();
            let ignored = self
                .git_ignored_paths
                .iter()
                .any(|p| p.to_ascii_lowercase() == target);
            return CommandOutput {
                success: ignored,
                exit_code: Some(if ignored { 0 } else { 1 }),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                not_found: false,
            };
        }

        if program == "git" && args.first() == Some(&"log") {
            if let Some(ts) = self.git_commit_ts {
                return CommandOutput {
                    success: true,
                    exit_code: Some(0),
                    stdout: format!("{ts}\n"),
                    stderr: String::new(),
                    timed_out: false,
                    not_found: false,
                };
            }
        }

        if program == "docker" {
            if let Some(ref out) = self.docker_reclaimed_output {
                return CommandOutput {
                    success: true,
                    exit_code: Some(0),
                    stdout: out.clone(),
                    stderr: String::new(),
                    timed_out: false,
                    not_found: false,
                };
            }
        }

        CommandOutput {
            success: true,
            exit_code: Some(0),
            stdout: "ok\n".to_string(),
            stderr: String::new(),
            timed_out: false,
            not_found: false,
        }
    }

    fn running_processes(&self) -> HashSet<String> {
        self.running.clone()
    }
}

#[test]
fn embedded_rules_catalog_is_valid_and_has_expected_categories() {
    let catalog = DevRulesCatalog::load_embedded().expect("embedded catalog must load");
    assert!(catalog.rules.len() >= 30);
    assert!(catalog.blacklist_never.forbid_docker_volumes);

    // Ensure no rule in catalog ever uses `--volumes`
    for rule in &catalog.rules {
        if let Some(ref cmd) = rule.metodo.comando {
            assert!(
                !cmd.iter().any(|a| a.eq_ignore_ascii_case("--volumes")),
                "Rule {} must never use --volumes",
                rule.id
            );
        }
    }
}

#[test]
fn parses_corepack_multiline_yarn_stdout_correctly() {
    let sample = "! Corepack is about to download https://registry.yarnpkg.com/yarn/-/yarn-1.22.22.tgz\r\nC:\\Users\\Dev\\AppData\\Local\\Yarn\\Cache\\v6\r\n";
    let parsed = parse_cli_path_stdout(sample).expect("should extract Windows path");
    assert_eq!(
        parsed.to_string_lossy(),
        r"C:\Users\Dev\AppData\Local\Yarn\Cache\v6"
    );
}

#[test]
fn project_scanner_enforces_manifest_git_ignore_age_no_nested_and_no_junctions() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let now_ts = 1_800_000_000i64;
    let old_ts = now_ts - (45 * 86_400); // 45 days old
    let recent_ts = now_ts - (5 * 86_400); // 5 days old

    // 1. Valid old git project with ignored `node_modules` AND nested `node_modules/pkg/package.json`
    let valid_proj = root.join("old-web-app");
    fs::create_dir_all(valid_proj.join(".git")).unwrap();
    fs::write(valid_proj.join("package.json"), r#"{"name":"old"}"#).unwrap();
    fs::create_dir_all(
        valid_proj
            .join("node_modules")
            .join("nested-pkg")
            .join("node_modules"),
    )
    .unwrap();
    fs::write(
        valid_proj
            .join("node_modules")
            .join("nested-pkg")
            .join("package.json"),
        r#"{"name":"nested"}"#,
    )
    .unwrap();
    fs::write(
        valid_proj.join("node_modules").join("bundle.js"),
        vec![b'x'; 4096],
    )
    .unwrap();
    // Also a tracked/non-ignored `dist` folder that MUST be excluded
    fs::create_dir_all(valid_proj.join("dist")).unwrap();
    fs::write(valid_proj.join("dist").join("out.js"), vec![b'y'; 2048]).unwrap();

    // 2. Folder with `node_modules` but NO `package.json` manifest (MUST be excluded)
    let no_manifest = root.join("no-manifest-dir");
    fs::create_dir_all(no_manifest.join("node_modules")).unwrap();
    fs::write(
        no_manifest.join("node_modules").join("a.js"),
        vec![b'z'; 4096],
    )
    .unwrap();

    // 3. Project with `.env`, `release.jks`, `local.properties` (MUST never be selected)
    fs::write(valid_proj.join(".env"), b"SECRET=123").unwrap();
    fs::write(valid_proj.join("release.jks"), b"KEYSTORE").unwrap();
    fs::write(valid_proj.join("local.properties"), b"sdk.dir=C:\\Sdk").unwrap();

    // 4. Junction pointing to valid_proj (MUST be skipped)
    let junction_dir = root.join("junction-to-old");
    let mklink = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &junction_dir.to_string_lossy(),
            &valid_proj.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(mklink.status.success(), "mklink /J failed");

    let catalog = DevRulesCatalog::load_embedded().unwrap();
    let mut runner = MockCommandRunner {
        git_commit_ts: Some(old_ts),
        ..Default::default()
    };
    runner.git_ignored_paths.insert("node_modules".to_string());

    let mut warnings = Vec::new();
    let found = scan_project_artifacts(
        &[root.to_string_lossy().into_owned()],
        30,
        now_ts,
        &catalog.rules,
        &catalog.blacklist_never,
        &HashSet::new(),
        &HashSet::new(),
        &runner,
        &mut warnings,
    );

    // Only `old-web-app/node_modules` should match:
    // - `dist` excluded because not git-ignored
    // - `no-manifest-dir/node_modules` excluded because no package.json
    // - nested `node_modules/nested-pkg/node_modules` excluded because scanner stops at artifact
    // - `junction-to-old` excluded because it is a junction
    assert_eq!(found.len(), 1, "Unexpected candidates: {found:#?}");
    assert_eq!(found[0].rule_id, "proj-node-modules");
    assert!(!found[0].no_git_repo);
    assert!(!found[0].preselected);
    assert_eq!(found[0].nivel, SafetyLevel::Revisar);
    assert_eq!(found[0].project_age_days, Some(45));

    // Re-run with recent commit (5 days old) -> MUST be excluded by 30-day threshold
    runner.git_commit_ts = Some(recent_ts);
    let found_recent = scan_project_artifacts(
        &[root.to_string_lossy().into_owned()],
        30,
        now_ts,
        &catalog.rules,
        &catalog.blacklist_never,
        &HashSet::new(),
        &HashSet::new(),
        &runner,
        &mut warnings,
    );
    assert!(
        found_recent.is_empty(),
        "Recent project (<30d) must be excluded"
    );
}

#[test]
fn dry_run_blocks_never_paths_and_executor_skips_running_processes_and_mocks_official_cmds() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let log_dir = root.join("logs");

    let artifact_dir = root.join("old-proj").join("node_modules");
    fs::create_dir_all(&artifact_dir).unwrap();
    fs::write(artifact_dir.join("lib.js"), vec![b'a'; 8192]).unwrap();

    let git_dir = root.join("old-proj").join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    let env_file = root.join("old-proj").join(".env");
    fs::write(&env_file, b"API_KEY=secret").unwrap();

    let catalog = DevRulesCatalog::load_embedded().unwrap();

    // Verify blacklist rejects .git and .env directly
    assert!(validate_dev_target_path(&git_dir, &catalog.blacklist_never).is_err());
    assert!(validate_dev_target_path(&env_file, &catalog.blacklist_never).is_err());

    // Build preview with:
    // 1. Legitimate artifact (permanent delete)
    // 2. Malicious attempt targeting `.git` under a valid rule ID (MUST be blocked in dry-run)
    // 3. Official Docker builder prune (mocked)
    // 4. Gradle cache with `studio64.exe` running (MUST be skipped without forcing)
    let make_cand = |id: &str,
                     rule_id: &str,
                     path: &str,
                     method: CleanMethodKind,
                     cmd: Option<Vec<String>>,
                     procs: Vec<String>| DevCandidate {
        id: id.to_string(),
        rule_id: rule_id.to_string(),
        nombre: id.to_string(),
        categoria: RuleCategory::Node,
        tipo: RuleKind::ArtefactoProyecto,
        nivel: SafetyLevel::Revisar,
        path: path.to_string(),
        extra_paths: Vec::new(),
        delete_dir_contents_only: false,
        project_name: None,
        project_root: None,
        project_age_days: Some(40),
        no_git_repo: false,
        size_bytes: 8192,
        item_count: 1,
        last_modified_ts: 0,
        method,
        command: cmd,
        pre_commands: Vec::new(),
        allow_recycle_bin_option: true,
        requires_admin: false,
        blocking_processes_running: Vec::new(),
        blocking_processes_spec: procs,
        warning: None,
        blocked_reason: None,
        como_regenerar: "npm install".to_string(),
        preselected: false,
    };

    let mut runner = MockCommandRunner {
        docker_reclaimed_output: Some("Total reclaimed space: 1.5GB\n".to_string()),
        ..Default::default()
    };
    runner.running.insert("studio64.exe".to_string());

    let gradle_dir = root.join("fake-gradle-caches");
    fs::create_dir_all(&gradle_dir).unwrap();
    fs::write(gradle_dir.join("cache.bin"), vec![b'g'; 1024]).unwrap();

    let req = DevPreviewRequest {
        items: vec![
            DevPreviewItemInput {
                candidate: make_cand(
                    "ok-nm",
                    "proj-node-modules",
                    &artifact_dir.to_string_lossy(),
                    CleanMethodKind::BorradoPermanente,
                    None,
                    vec![],
                ),
                use_recycle_bin: false,
            },
            DevPreviewItemInput {
                candidate: make_cand(
                    "bad-git",
                    "proj-node-modules",
                    &git_dir.to_string_lossy(),
                    CleanMethodKind::BorradoPermanente,
                    None,
                    vec![],
                ),
                use_recycle_bin: false,
            },
            DevPreviewItemInput {
                candidate: make_cand(
                    "docker-prune",
                    "docker-builder-prune",
                    "Docker (Build Cache)",
                    CleanMethodKind::ComandoOficial,
                    Some(vec![
                        "docker".to_string(),
                        "builder".to_string(),
                        "prune".to_string(),
                        "-f".to_string(),
                    ]),
                    vec![],
                ),
                use_recycle_bin: false,
            },
            DevPreviewItemInput {
                candidate: make_cand(
                    "gradle-blocked-proc",
                    "android-gradle-caches",
                    &gradle_dir.to_string_lossy(),
                    CleanMethodKind::BorradoPermanente,
                    None,
                    vec!["studio64.exe".to_string()],
                ),
                use_recycle_bin: false,
            },
        ],
    };

    let plan = build_dev_cleanup_plan(req, &catalog, &runner);
    assert_eq!(
        plan.blocked_items.len(),
        1,
        ".git must be blocked in dry-run"
    );
    assert_eq!(plan.blocked_items[0].id, "bad-git");
    assert_eq!(plan.items.len(), 3);

    let guard = SandboxGuard::new(root);
    let res = execute_dev_cleanup_plan(&plan, &catalog, &runner, Some(&log_dir), Some(&guard));

    assert_eq!(res.succeeded_count, 2);
    assert_eq!(res.skipped_count, 1);
    assert_eq!(res.failed_count, 0);
    assert!(!artifact_dir.exists(), "node_modules should be deleted");
    assert!(git_dir.exists(), ".git must never be deleted");
    assert!(
        gradle_dir.exists(),
        "gradle cache must be skipped while studio64.exe runs"
    );

    // Verify mocked docker command was called without --volumes
    let calls = runner.recorded_calls();
    assert!(calls
        .iter()
        .any(|c| c == &vec!["docker", "builder", "prune", "-f"]));

    // Verify JSONL log was persisted with rule_id and estimated_bytes
    let logs = read_cleanup_logs(&log_dir, 20);
    assert_eq!(logs.len(), 3);
    assert!(logs
        .iter()
        .any(|l| l.rule_id.as_deref() == Some("proj-node-modules") && l.bytes == 8192));
    assert!(logs
        .iter()
        .any(|l| l.rule_id.as_deref() == Some("android-gradle-caches") && l.status == "skipped"));
}

#[test]
fn dev_config_roundtrips_in_tempdir() {
    let tmp = TempDir::new().unwrap();
    let cfg_path = tmp.path().join("dev-cleaner-config.json");
    let cfg = DevCleanerConfig {
        project_roots: vec![r"D:\proyectos".to_string(), r"C:\dev".to_string()],
        min_project_age_days: 45,
        disabled_rule_ids: vec!["sys-dism-cleanup".to_string()],
    };
    cfg.save_to(&cfg_path).unwrap();
    let loaded = DevCleanerConfig::load_from(&cfg_path);
    assert_eq!(loaded.project_roots, cfg.project_roots);
    assert_eq!(loaded.min_project_age_days, 45);
    assert_eq!(loaded.disabled_rule_ids, cfg.disabled_rule_ids);
}
