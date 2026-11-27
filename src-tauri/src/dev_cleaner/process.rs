use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub not_found: bool,
}

pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> CommandOutput;

    fn running_processes(&self) -> HashSet<String>;
}

#[derive(Debug, Default, Clone)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(
        &self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        timeout: Duration,
    ) -> CommandOutput {
        // On Windows, tools like npm, pnpm, yarn, composer, sdkmanager are often `.cmd` / `.bat` shims.
        // We try direct execution first; if NotFound, retry via `cmd /C <program> <args...>` when safe.
        match spawn_and_wait(program, args, cwd, timeout) {
            Some(out) => out,
            None => {
                // Retry via cmd.exe for .cmd/.bat shims in PATH
                let mut cmd_args = Vec::with_capacity(args.len() + 2);
                cmd_args.push("/C");
                cmd_args.push(program);
                cmd_args.extend_from_slice(args);
                match spawn_and_wait("cmd.exe", &cmd_args, cwd, timeout) {
                    Some(out) => {
                        let missing = !out.success
                            && (out.stderr.contains("is not recognized")
                                || out.stderr.contains("no se reconoce"));
                        CommandOutput {
                            not_found: missing,
                            ..out
                        }
                    }
                    None => CommandOutput {
                        success: false,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: format!("No se encontró el ejecutable '{program}'."),
                        timed_out: false,
                        not_found: true,
                    },
                }
            }
        }
    }

    fn running_processes(&self) -> HashSet<String> {
        list_running_process_names()
    }
}

fn spawn_and_wait(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    timeout: Duration,
) -> Option<CommandOutput> {
    const MAX_PIPE_BYTES: usize = 2 * 1024 * 1024;

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            return Some(CommandOutput {
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: e.to_string(),
                timed_out: false,
                not_found: false,
            });
        }
    };

    // Drain stdout and stderr concurrently on dedicated threads so Windows pipe buffers (~4 KB)
    // can NEVER fill and deadlock verbose CLIs like `pnpm store prune` or `npm cache clean`.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = std::thread::spawn(move || read_pipe_bounded(stdout_pipe, MAX_PIPE_BYTES));
    let stderr_handle = std::thread::spawn(move || read_pipe_bounded(stderr_pipe, MAX_PIPE_BYTES));

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout_bytes = stdout_handle.join().unwrap_or_default();
                let stderr_bytes = stderr_handle.join().unwrap_or_default();
                return Some(CommandOutput {
                    success: status.success(),
                    exit_code: status.code(),
                    stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                    timed_out: false,
                    not_found: false,
                });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    // Kill entire process tree (important when `cmd.exe /C` spawned node/pnpm)
                    let pid = child.id();
                    let _ = Command::new("taskkill")
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .creation_flags(CREATE_NO_WINDOW)
                        .status();
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    return Some(CommandOutput {
                        success: false,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: format!(
                            "Tiempo de espera agotado ({}s) ejecutando '{program}'.",
                            timeout.as_secs()
                        ),
                        timed_out: true,
                        not_found: false,
                    });
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                return Some(CommandOutput {
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    timed_out: false,
                    not_found: false,
                });
            }
        }
    }
}

fn read_pipe_bounded<R: std::io::Read>(pipe: Option<R>, max_bytes: usize) -> Vec<u8> {
    let Some(mut reader) = pipe else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.len() < max_bytes {
                    let take = n.min(max_bytes - out.len());
                    out.extend_from_slice(&buf[..take]);
                }
                // Keep draining even after cap so the child process never blocks on WriteFile!
            }
        }
    }
    out
}

pub fn list_running_process_names() -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return names;
    };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snap, &mut entry) }.is_ok() {
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = OsString::from_wide(&entry.szExeFile[..end])
                .to_string_lossy()
                .to_ascii_lowercase();
            if !name.is_empty() {
                names.insert(name);
            }
            if unsafe { Process32NextW(snap, &mut entry) }.is_err() {
                break;
            }
        }
    }
    unsafe {
        let _ = CloseHandle(snap);
    }
    names
}

pub fn matching_blocking_processes(spec: &[String], running: &HashSet<String>) -> Vec<String> {
    let mut active = Vec::new();
    for proc_name in spec {
        let low = proc_name.to_ascii_lowercase();
        if running.contains(&low) {
            active.push(proc_name.clone());
        }
    }
    active.sort();
    active.dedup();
    active
}
