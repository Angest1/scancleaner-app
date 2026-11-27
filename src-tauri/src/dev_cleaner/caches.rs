use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::index::default_index_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedToolQuery {
    pub exe_path: Option<String>,
    pub exe_mtime: i64,
    pub available: bool,
    pub stdout: String,
    pub cached_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCacheFile {
    entries: HashMap<String, CachedToolQuery>,
}

static TOOL_CACHE: OnceLock<Mutex<ToolCacheFile>> = OnceLock::new();

fn tool_cache_path() -> PathBuf {
    default_index_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
        .join("tool-cache.json")
}

fn load_tool_cache() -> &'static Mutex<ToolCacheFile> {
    TOOL_CACHE.get_or_init(|| {
        let p = tool_cache_path();
        let data = fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str::<ToolCacheFile>(&s).ok())
            .unwrap_or_default();
        Mutex::new(data)
    })
}

fn save_tool_cache(cache: &ToolCacheFile) {
    let p = tool_cache_path();
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(p, json);
    }
}

/// Resolves an executable on `PATH` and returns `(resolved_path, mtime_unix_secs)`.
pub fn resolve_executable_mtime(program: &str) -> Option<(PathBuf, i64)> {
    let path_env = std::env::var_os("PATH")?;
    let exts = if program.contains('.') {
        vec![""]
    } else {
        vec![".exe", ".cmd", ".bat", ""]
    };
    for dir in std::env::split_paths(&path_env) {
        for ext in &exts {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                let mtime = fs::metadata(&candidate)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64);
                return Some((candidate, mtime));
            }
        }
    }
    None
}

/// Looks up a cached CLI query (`npm config get cache`, `pnpm store path`, `docker --version`, etc.)
/// invalidating automatically whenever the executable's `mtime` changes on disk!
pub fn query_tool_cached(
    program: &str,
    args: &[&str],
    runner: &dyn super::process::CommandRunner,
    timeout: std::time::Duration,
) -> (bool, String) {
    let key = format!("{}|{}", program.to_ascii_lowercase(), args.join(" "));
    let resolved = resolve_executable_mtime(program);
    let (exe_path_opt, exe_mtime) = match resolved {
        Some((p, mt)) => (Some(p.to_string_lossy().into_owned()), mt),
        None => (None, 0),
    };

    let cache_lock = load_tool_cache();
    if let Ok(guard) = cache_lock.lock() {
        if let Some(hit) = guard.entries.get(&key) {
            let same_exe = hit.exe_path == exe_path_opt && hit.exe_mtime == exe_mtime;
            let not_missing_too_old =
                hit.available || (chrono::Utc::now().timestamp() - hit.cached_at) < 300;
            if same_exe && not_missing_too_old {
                return (hit.available, hit.stdout.clone());
            }
        }
    }

    // If executable isn't even on PATH, avoid spawning a failing process!
    if exe_path_opt.is_none() {
        let entry = CachedToolQuery {
            exe_path: None,
            exe_mtime: 0,
            available: false,
            stdout: String::new(),
            cached_at: chrono::Utc::now().timestamp(),
        };
        if let Ok(mut guard) = cache_lock.lock() {
            guard.entries.insert(key, entry);
            save_tool_cache(&guard);
        }
        return (false, String::new());
    }

    let res = runner.run(program, args, None, timeout);
    let available = !res.not_found && res.success;
    let stdout = res.stdout;

    let entry = CachedToolQuery {
        exe_path: exe_path_opt,
        exe_mtime,
        available,
        stdout: stdout.clone(),
        cached_at: chrono::Utc::now().timestamp(),
    };
    if let Ok(mut guard) = cache_lock.lock() {
        guard.entries.insert(key, entry);
        save_tool_cache(&guard);
    }

    (available, stdout)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoFingerprint {
    gitignore_mtime: i64,
    exclude_mtime: i64,
    head_mtime: i64,
}

fn file_mtime_secs(path: &Path) -> i64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn compute_repo_fingerprint(repo_root: &Path) -> RepoFingerprint {
    RepoFingerprint {
        gitignore_mtime: file_mtime_secs(&repo_root.join(".gitignore")),
        exclude_mtime: file_mtime_secs(&repo_root.join(".git").join("info").join("exclude")),
        head_mtime: file_mtime_secs(&repo_root.join(".git").join("HEAD")),
    }
}

struct CachedRepoIgnores {
    fingerprint: RepoFingerprint,
    results: HashMap<String, bool>,
}

static GIT_IGNORE_CACHE: OnceLock<Mutex<HashMap<String, CachedRepoIgnores>>> = OnceLock::new();

fn git_ignore_cache() -> &'static Mutex<HashMap<String, CachedRepoIgnores>> {
    GIT_IGNORE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Evaluates `git check-ignore` for ALL candidate paths in `repo_root` with a SINGLE
/// `git check-ignore --stdin` invocation per repository, cached and invalidated whenever
/// `.gitignore`, `.git/info/exclude`, or `.git/HEAD` changes.
pub fn batch_check_git_ignored(repo_root: &Path, candidate_paths: &[PathBuf]) -> HashSet<String> {
    if candidate_paths.is_empty() {
        return HashSet::new();
    }
    let repo_key = repo_root.to_string_lossy().to_ascii_lowercase();
    let fp = compute_repo_fingerprint(repo_root);

    let mut rel_to_orig: Vec<(String, String)> = Vec::with_capacity(candidate_paths.len());
    for p in candidate_paths {
        let rel = p
            .strip_prefix(repo_root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/");
        rel_to_orig.push((rel, p.to_string_lossy().to_ascii_lowercase()));
    }

    let mut missing_rels: Vec<String> = Vec::new();
    let mut ignored_set: HashSet<String> = HashSet::new();

    if let Ok(mut cache) = git_ignore_cache().lock() {
        let entry = cache
            .entry(repo_key.clone())
            .or_insert_with(|| CachedRepoIgnores {
                fingerprint: fp.clone(),
                results: HashMap::new(),
            });
        if entry.fingerprint != fp {
            entry.fingerprint = fp.clone();
            entry.results.clear();
        }
        for (rel, orig_low) in &rel_to_orig {
            if let Some(&is_ign) = entry.results.get(rel) {
                if is_ign {
                    ignored_set.insert(orig_low.clone());
                }
            } else {
                missing_rels.push(rel.clone());
            }
        }
    }

    if missing_rels.is_empty() {
        return ignored_set;
    }

    // Single batched `git check-ignore --stdin` process call for all uncached paths in this repo!
    let mut newly_ignored: HashSet<String> = HashSet::new();
    let mut cmd = Command::new("git");
    cmd.args(["check-ignore", "--stdin"])
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    if let Ok(mut child) = cmd.spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            for rel in &missing_rels {
                let _ = writeln!(stdin, "{rel}");
            }
        }
        if let Ok(out) = child.wait_with_output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim().trim_matches('"').replace('\\', "/");
                if !trimmed.is_empty() {
                    newly_ignored.insert(trimmed.to_ascii_lowercase());
                }
            }
        }
    }

    if let Ok(mut cache) = git_ignore_cache().lock() {
        let entry = cache.entry(repo_key).or_insert_with(|| CachedRepoIgnores {
            fingerprint: fp,
            results: HashMap::new(),
        });
        for rel in &missing_rels {
            let is_ign = newly_ignored.contains(&rel.to_ascii_lowercase());
            entry.results.insert(rel.clone(), is_ign);
        }
        for (rel, orig_low) in &rel_to_orig {
            if entry.results.get(rel).copied().unwrap_or(false) {
                ignored_set.insert(orig_low.clone());
            }
        }
    }

    ignored_set
}
