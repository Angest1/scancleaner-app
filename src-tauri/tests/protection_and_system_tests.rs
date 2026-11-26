use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::GetShortPathNameW;

use scancleaner_lib::cleaner::executor::{
    build_cleanup_plan, execute_cleanup_plan, CleanupMode, CleanupTargetInput,
    PreviewCleanupRequest,
};
use scancleaner_lib::cleaner::guardrails::SandboxGuard;
use scancleaner_lib::protection::{
    parse_registry_pagefile_entry, ProtectedEntry, ProtectionEngine, ProtectionOrigin,
    ProtectionType,
};
use scancleaner_lib::storage::read_cleanup_logs;
use scancleaner_lib::system_files::{
    open_power_options, open_virtual_memory_dialog, run_powercfg_elevated, PowercfgAction,
    SystemProcessLauncher,
};

#[derive(Default)]
struct MockLauncher {
    spawned: Mutex<Vec<(String, Vec<String>)>>,
    elevated: Mutex<Vec<(String, String)>>,
}

impl SystemProcessLauncher for MockLauncher {
    fn spawn(&self, program: &str, args: &[&str]) -> Result<(), String> {
        self.spawned.lock().unwrap().push((
            program.to_string(),
            args.iter().map(|s| (*s).to_string()).collect(),
        ));
        Ok(())
    }

    fn spawn_elevated(&self, program: &str, params: &str) -> Result<(), String> {
        self.elevated
            .lock()
            .unwrap()
            .push((program.to_string(), params.to_string()));
        Ok(())
    }
}

fn try_get_short_path(path: &Path) -> Option<String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let needed = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), None) };
    if needed == 0 {
        return None;
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buf)) };
    if written == 0 || (written as usize) >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..written as usize]))
}

#[test]
fn critical_entries_block_across_all_drives_and_cannot_be_removed_via_user_file() {
    let tmp = TempDir::new().unwrap();
    let user_cfg = tmp.path().join("protected-paths.json");

    // Tamper with user config trying to overwrite/inject a critical entry
    fs::write(
        &user_cfg,
        r#"{
          "entries": [
            {
              "id": "crit-pagefile",
              "patron": "nothing.tmp",
              "tipo": "archivo",
              "origen": "crítica",
              "motivo": "tampered",
              "todasLasUnidades": false
            }
          ]
        }"#,
    )
    .unwrap();

    let engine = ProtectionEngine::load_with_user_file(&user_cfg);
    // Tampered entry must be ignored; factory critical entries must still block on any drive
    assert!(engine.user_entries.is_empty());

    let must_block = [
        r"C:\pagefile.sys",
        r"D:\PAGEFILE.SYS",
        r"E:\swapfile.sys",
        r"F:\hiberfil.sys",
        r"D:\System Volume Information\tracking.log",
        r"E:\$Recycle.Bin\S-1-5-21",
        r"D:\Windows.old\Users\foo",
        r"C:\$Windows.~BT\Sources",
        r"C:\bootmgr",
        r"C:\BOOTNXT",
        r"%SystemRoot%\explorer.exe",
        r"%USERPROFILE%\.ssh\id_ed25519",
        r"%USERPROFILE%\.gnupg\secring.gpg",
        r"%APPDATA%\Microsoft\Credentials\cred1",
        r"%LOCALAPPDATA%\Microsoft\Credentials\cred2",
        r"\\?\C:\Windows\System32\cmd.exe",
    ];

    for p in must_block {
        let res = engine.is_protected(Path::new(p));
        assert!(res.protected, "Expected critical path to be blocked: {p}");
        assert!(res.entry.is_some());
    }
}

#[test]
fn parses_registry_paging_file_formats() {
    assert_eq!(
        parse_registry_pagefile_entry(r"?:\pagefile.sys"),
        Some(("pagefile.sys".to_string(), true))
    );
    assert_eq!(
        parse_registry_pagefile_entry(r"\??\D:\custom_pagefile.sys 4096 8192"),
        Some((r"D:\custom_pagefile.sys".to_string(), false))
    );
}

#[test]
fn matching_handles_83_short_names_junctions_globs_and_protected_descendants() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // 1. Directory with long name protected by user rule -> test 8.3 short name & junction
    let secret_dir = root.join("VeryLongProtectedFolderName");
    fs::create_dir_all(&secret_dir).unwrap();
    let secret_file = secret_dir.join("secrets.txt");
    fs::write(&secret_file, b"top secret").unwrap();

    // 2. Junction pointing to secret_dir -> must NOT bypass protection
    let junction = root.join("sneaky_link");
    let mk = Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &junction.to_string_lossy(),
            &secret_dir.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(mk.status.success(), "mklink /J failed");

    // 3. Project directory containing `.git` -> deleting parent must be blocked with `.git` entry
    let project_dir = root.join("my-project");
    fs::create_dir_all(project_dir.join(".git")).unwrap();
    fs::write(
        project_dir.join(".git").join("HEAD"),
        b"ref: refs/heads/main",
    )
    .unwrap();
    fs::write(project_dir.join("README.md"), b"hello").unwrap();

    let mut engine = ProtectionEngine::load_with_user_file(&root.join("none.json"));
    engine.user_entries.push(ProtectedEntry {
        id: "user-secret".to_string(),
        patron: secret_dir.to_string_lossy().into_owned(),
        tipo: ProtectionType::CarpetaRecursiva,
        origen: ProtectionOrigin::Usuario,
        motivo: "Carpeta secreta del usuario".to_string(),
        todas_las_unidades: false,
    });

    // Junction pointing to protected folder MUST be blocked
    let j_res = engine.is_protected(&junction);
    assert!(
        j_res.protected,
        "Junction to protected folder must be blocked"
    );
    assert_eq!(j_res.entry.as_ref().unwrap().id, "user-secret");

    // 8.3 short path (if volume has 8.3 enabled) MUST also resolve and block
    if let Some(short) = try_get_short_path(&secret_file) {
        let s_res = engine.is_protected(Path::new(&short));
        assert!(
            s_res.protected,
            "8.3 short path '{short}' must resolve to long path and be blocked"
        );
    }

    // Globs: `.git`, `.env.local`, `release.jks`, `debug.keystore`, `local.properties`
    for g in [
        r"D:\dev\app\.git\config",
        r"D:\dev\app\.env.local",
        r"D:\dev\app\release.jks",
        r"D:\dev\app\debug.keystore",
        r"D:\dev\app\local.properties",
    ] {
        assert!(
            engine.is_protected(Path::new(g)).protected,
            "Glob should protect: {g}"
        );
    }

    // Parent folder containing `.git` MUST be blocked and identify `crit-glob-git`
    let proj_res = engine.is_protected(&project_dir);
    assert!(
        proj_res.protected,
        "Deleting parent folder containing .git must be blocked"
    );
    assert_eq!(proj_res.entry.as_ref().unwrap().id, "crit-glob-git");
}

#[test]
fn precedence_user_beats_system_exception_and_system_exception_beats_critical() {
    let tmp = TempDir::new().unwrap();
    let mut engine = ProtectionEngine::load_with_user_file(&tmp.path().join("none.json"));

    let allowed_in_win_temp = Path::new(r"C:\Windows\Temp\installer_cache.tmp");
    let user_locked_in_win_temp = Path::new(r"C:\Windows\Temp\keep_me_forever.tmp");

    // SystemException (C:\Windows\Temp\*) beats Critical (C:\Windows)
    assert!(
        !engine.is_protected(allowed_in_win_temp).protected,
        "System exception should allow C:\\Windows\\Temp\\*"
    );

    // Add user rule inside C:\Windows\Temp\* -> User MUST beat SystemException
    engine.user_entries.push(ProtectedEntry {
        id: "user-win-temp-override".to_string(),
        patron: user_locked_in_win_temp.to_string_lossy().into_owned(),
        tipo: ProtectionType::Archivo,
        origen: ProtectionOrigin::Usuario,
        motivo: "Archivo protegido explícitamente por usuario".to_string(),
        todas_las_unidades: false,
    });

    let res = engine.is_protected(user_locked_in_win_temp);
    assert!(
        res.protected,
        "User entry MUST override system exception in C:\\Windows\\Temp"
    );
    assert_eq!(res.entry.as_ref().unwrap().id, "user-win-temp-override");
    assert!(!engine.is_protected(allowed_in_win_temp).protected);
}

#[test]
fn deletion_layers_block_protected_items_log_them_and_continue_rest() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let log_dir = root.join("logs");

    let disposable = root.join("cache_dir");
    fs::create_dir_all(&disposable).unwrap();
    fs::write(disposable.join("tmp.bin"), vec![b'a'; 2048]).unwrap();

    let protected_env = root.join(".env.production");
    fs::write(&protected_env, b"SECRET=1").unwrap();

    let plan = build_cleanup_plan(PreviewCleanupRequest {
        targets: vec![
            CleanupTargetInput {
                path: disposable.to_string_lossy().into_owned(),
                category: "user_temp".to_string(),
                is_temp_dir_contents: false,
            },
            CleanupTargetInput {
                path: protected_env.to_string_lossy().into_owned(),
                category: "user_temp".to_string(),
                is_temp_dir_contents: false,
            },
        ],
        mode: CleanupMode::Permanent,
    });

    // Dry-run blocks `.env.production` while keeping `cache_dir`
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.blocked_items.len(), 1);
    assert!(plan.blocked_items[0].path.ends_with(".env.production"));

    // Also verify execution-layer check directly by injecting a protected item into a plan
    let mut forged_plan = plan.clone();
    forged_plan
        .items
        .push(scancleaner_lib::cleaner::executor::PlannedItem {
            path: protected_env.to_string_lossy().into_owned(),
            category: "user_temp".to_string(),
            bytes: 8,
        });

    let guard = SandboxGuard::new(root);
    let result = execute_cleanup_plan(&forged_plan, true, Some(&log_dir), Some(&guard)).unwrap();

    // `cache_dir` deleted, `.env.production` blocked & logged, rest continued cleanly
    assert_eq!(result.succeeded_count, 1);
    assert_eq!(result.skipped_count, 1);
    assert!(!disposable.exists());
    assert!(
        protected_env.exists(),
        "Protected .env must NEVER be deleted"
    );

    let logs = read_cleanup_logs(&log_dir, 20);
    assert!(logs
        .iter()
        .any(|l| l.status == "blocked" && l.path.ends_with(".env.production")));
}

#[test]
fn system_tools_launcher_mocks_virtual_memory_fallback_power_options_and_powercfg() {
    let tmp = TempDir::new().unwrap();
    let sys32 = tmp.path().join("System32");
    fs::create_dir_all(&sys32).unwrap();

    // Case 1: SystemPropertiesPerformance.exe exists -> launch it
    let perf = sys32.join("SystemPropertiesPerformance.exe");
    let adv = sys32.join("SystemPropertiesAdvanced.exe");
    fs::write(&perf, b"").unwrap();
    fs::write(&adv, b"").unwrap();

    let launcher = MockLauncher::default();
    let opened = open_virtual_memory_dialog(tmp.path(), &launcher).unwrap();
    assert!(opened.ends_with("SystemPropertiesPerformance.exe"));

    // Case 2: Remove Performance exe -> fallback to SystemPropertiesAdvanced.exe
    fs::remove_file(&perf).unwrap();
    let opened_fb = open_virtual_memory_dialog(tmp.path(), &launcher).unwrap();
    assert!(opened_fb.ends_with("SystemPropertiesAdvanced.exe"));

    // Case 3: control powercfg.cpl
    open_power_options(&launcher).unwrap();
    {
        let spawned = launcher.spawned.lock().unwrap();
        assert!(spawned
            .iter()
            .any(|(p, a)| p == "control.exe" && a == &vec!["powercfg.cpl"]));
    }

    // Case 4: powercfg elevated actions
    run_powercfg_elevated(PowercfgAction::Reduced, &launcher).unwrap();
    run_powercfg_elevated(PowercfgAction::Off, &launcher).unwrap();
    run_powercfg_elevated(PowercfgAction::On, &launcher).unwrap();
    let elev = launcher.elevated.lock().unwrap();
    assert_eq!(
        *elev,
        vec![
            ("powercfg.exe".to_string(), "/h /type reduced".to_string()),
            ("powercfg.exe".to_string(), "/h off".to_string()),
            ("powercfg.exe".to_string(), "/h on".to_string()),
        ]
    );
}
