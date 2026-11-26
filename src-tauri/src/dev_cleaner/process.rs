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

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().ok()?;
                return Some(CommandOutput {
                    success: output.status.success(),
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    timed_out: false,
                    not_found: false,
                });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
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
