//! AI file-operation protocol helpers: SEARCH/REPLACE edits, interpreter
//! allowlisting, and bounded script execution inside the project root.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;

/// Set by the UI Stop button; checked by the run loop so a running script can be killed.
pub static RUN_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_run_cancel() {
    RUN_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

pub const DEFAULT_RUN_ALLOWLIST: &str = "python3,python,bash,sh,node";
pub const DEFAULT_RUN_TIMEOUT_SECONDS: u64 = 60;
pub const MAX_RUN_TIMEOUT_SECONDS: u64 = 600;
/// Per-stream cap on captured output returned to the model.
pub const MAX_OUTPUT_BYTES: usize = 12_000;
pub const MAX_RUN_MEMORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_RUN_FILE_BYTES: u64 = 256 * 1024 * 1024;
const TMPFS_LIMIT_128_MIB: &str = "134217728";
const TMPFS_LIMIT_16_MIB: &str = "16777216";
/// Scratch directory (relative to the project root) used by CALC blocks.
pub const SCRATCH_DIR: &str = ".scratch";
pub const MAX_CALC_TIMEOUT_SECONDS: u64 = 120;
pub const MAX_CALC_ARTIFACTS: usize = 6;
/// Per-artifact cap on image bytes returned inline to the UI.
pub const MAX_CALC_ARTIFACT_BYTES: u64 = 2_000_000;
/// Cap on file contents returned by READ.
pub const MAX_READ_BYTES: usize = 48_000;

#[derive(Debug, Clone, PartialEq)]
pub struct EditHunk {
    pub search: String,
    pub replace: String,
}

/// Parses an EDIT body made of one or more hunks:
/// ```text
/// <<<<< SEARCH
/// old text
/// =====
/// new text
/// >>>>> REPLACE
/// ```
pub fn parse_edit_hunks(body: &str) -> Result<Vec<EditHunk>, String> {
    let mut hunks = Vec::new();
    let mut state = 0u8; // 0 outside, 1 in search, 2 in replace
    let mut search = String::new();
    let mut replace = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        match (state, trimmed) {
            (0, t) if t.starts_with("<<<<<") && t.contains("SEARCH") => {
                state = 1;
                search.clear();
                replace.clear();
            }
            (0, "") => {}
            (0, other) => return Err(format!("Unexpected text outside a SEARCH/REPLACE hunk: '{other}'")),
            (1, t) if t.starts_with("=====") => state = 2,
            (1, _) => {
                search.push_str(line);
                search.push('\n');
            }
            (2, t) if t.starts_with(">>>>>") && t.contains("REPLACE") => {
                hunks.push(EditHunk {
                    search: strip_trailing_newline(&search),
                    replace: strip_trailing_newline(&replace),
                });
                state = 0;
            }
            (2, _) => {
                replace.push_str(line);
                replace.push('\n');
            }
            _ => unreachable!(),
        }
    }
    if state != 0 {
        return Err("Unterminated SEARCH/REPLACE hunk (missing '>>>>> REPLACE').".to_string());
    }
    if hunks.is_empty() {
        return Err("EDIT block contains no SEARCH/REPLACE hunks.".to_string());
    }
    if hunks.iter().any(|h| h.search.trim().is_empty()) {
        return Err("A SEARCH section is empty; use APPEND or FILE to add content to an empty file.".to_string());
    }
    Ok(hunks)
}

fn strip_trailing_newline(text: &str) -> String {
    text.strip_suffix('\n').unwrap_or(text).to_string()
}

/// Applies hunks in order; every SEARCH text must occur exactly once.
pub fn apply_edit_hunks(content: &str, hunks: &[EditHunk]) -> Result<String, String> {
    let mut result = content.to_string();
    for (i, hunk) in hunks.iter().enumerate() {
        let count = result.matches(hunk.search.as_str()).count();
        match count {
            1 => result = result.replacen(hunk.search.as_str(), hunk.replace.as_str(), 1),
            0 => {
                return Err(format!(
                    "Hunk {} SEARCH text not found in the file (it must match the current contents exactly, including indentation). Use READ to fetch the current file and retry.",
                    i + 1
                ))
            }
            n => {
                return Err(format!(
                    "Hunk {} SEARCH text matches {n} locations; include more surrounding lines so it is unique.",
                    i + 1
                ))
            }
        }
    }
    Ok(result)
}

pub fn parse_allowlist(raw: &str) -> Vec<String> {
    let list: Vec<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if list.is_empty() {
        parse_allowlist(DEFAULT_RUN_ALLOWLIST)
    } else {
        list
    }
}

/// Splits a command line on whitespace honouring single/double quotes.
pub fn split_command(cmd: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for ch in cmd.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                has_token = true;
            }
            None if ch.is_whitespace() => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            None if matches!(ch, '|' | '&' | ';' | '>' | '<' | '`' | '$') => {
                return Err(format!("Shell operator '{ch}' is not allowed in RUN commands; run one program directly."));
            }
            None => {
                current.push(ch);
                has_token = true;
            }
        }
    }
    if quote.is_some() {
        return Err("Unbalanced quote in RUN command.".to_string());
    }
    if has_token {
        args.push(current);
    }
    if args.is_empty() {
        return Err("Empty RUN command.".to_string());
    }
    Ok(args)
}

/// Validates that the program is on the allowlist and no argument escapes the
/// project root (absolute paths outside it, or `..` components).
/// Interpreter flags that execute code given on the command line; they would let any
/// allowlisted interpreter act as an arbitrary shell (`python3 -c "shutil.rmtree(...)"`).
const INLINE_CODE_FLAGS: &[&str] = &["-c", "-e", "--eval", "-p", "--print", "-r", "--require", "--import"];

pub fn validate_run_command(cmd: &str, allowlist: &[String], project_root: &Path) -> Result<Vec<String>, String> {
    let argv = split_command(cmd)?;
    let program = Path::new(&argv[0])
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !allowlist.iter().any(|a| a.eq_ignore_ascii_case(&program)) {
        return Err(format!(
            "Program '{program}' is not on the AI run allowlist ({}). Adjust the allowlist in Settings if this is intended.",
            allowlist.join(", ")
        ));
    }
    for arg in &argv[1..] {
        if INLINE_CODE_FLAGS.iter().any(|f| f.eq_ignore_ascii_case(arg)) {
            return Err(format!(
                "Flag '{arg}' runs inline code and is refused; put the code in a project file (or a CALC block) and run that instead."
            ));
        }
        let p = Path::new(arg);
        if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("Argument '{arg}' uses '..' which may leave the project root."));
        }
        if p.is_absolute() && !p.starts_with(project_root) {
            return Err(format!("Argument '{arg}' points outside the project root."));
        }
    }
    Ok(argv)
}

#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    pub cancelled: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub elapsed_seconds: f64,
}

fn capture_output<R: Read>(mut reader: R) -> (String, bool) {
    let half = MAX_OUTPUT_BYTES / 2;
    let mut head = Vec::with_capacity(half);
    let mut tail = Vec::with_capacity(half);
    let mut total = 0usize;
    let mut chunk = [0u8; 8192];

    loop {
        let read = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        total = total.saturating_add(read);
        let mut remaining = &chunk[..read];
        if head.len() < half {
            let copied = (half - head.len()).min(remaining.len());
            head.extend_from_slice(&remaining[..copied]);
            remaining = &remaining[copied..];
        }
        if !remaining.is_empty() {
            tail.extend_from_slice(remaining);
            if tail.len() > half {
                let excess = tail.len() - half;
                tail.drain(..excess);
            }
        }
    }

    if total <= MAX_OUTPUT_BYTES {
        let mut captured = head;
        captured.extend_from_slice(&tail);
        return (String::from_utf8_lossy(&captured).into_owned(), false);
    }

    let omitted = total.saturating_sub(head.len()).saturating_sub(tail.len());
    let head = String::from_utf8_lossy(&head);
    let tail = String::from_utf8_lossy(&tail);
    (format!("{head}\n… [{omitted} bytes omitted] …\n{tail}"), true)
}

#[cfg(target_os = "linux")]
fn sandbox_command(argv: &[String], project_root: &Path, timeout: Duration) -> Result<Command, String> {
    let project_root = project_root
        .canonicalize()
        .map_err(|e| format!("AI execution sandbox requires a canonical project root: {e}"))?;
    if !project_root.is_dir() {
        return Err("AI execution sandbox requires the project root to be a directory.".to_string());
    }
    if ["/", "/home", "/root", "/run/user", "/media", "/mnt", "/srv", "/tmp", "/var/tmp", "/var/lib", "/sys"]
        .iter()
        .any(|path| project_root == Path::new(path))
    {
        return Err("AI execution blocked: choose a project subdirectory, not a protected system or temporary directory.".to_string());
    }

    let bwrap = ["/usr/bin/bwrap", "/bin/bwrap"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .ok_or_else(|| "AI execution blocked: Bubblewrap is not installed; install the bubblewrap package and retry.".to_string())?;

    let mut command = Command::new(bwrap);
    command
        .env_clear()
        .args([
            "--die-with-parent",
            "--new-session",
            "--unshare-user",
            "--unshare-pid",
            "--unshare-net",
            "--unshare-ipc",
            "--unshare-uts",
            "--disable-userns",
            "--cap-drop",
            "ALL",
            "--ro-bind",
            "/",
            "/",
            "--size",
            TMPFS_LIMIT_16_MIB,
            "--tmpfs",
            "/home",
            "--size",
            TMPFS_LIMIT_16_MIB,
            "--tmpfs",
            "/root",
            "--size",
            TMPFS_LIMIT_128_MIB,
            "--tmpfs",
            "/tmp",
            "--size",
            TMPFS_LIMIT_128_MIB,
            "--tmpfs",
            "/var/tmp",
        ]);

    for path in ["/run", "/media", "/mnt", "/srv", "/var/lib"] {
        if Path::new(path).exists() {
            command.args(["--size", TMPFS_LIMIT_16_MIB, "--tmpfs", path]);
        }
    }
    command
        .args(["--size", TMPFS_LIMIT_16_MIB, "--tmpfs", "/sys", "--proc", "/proc", "--dev", "/dev"])
        .args(["--size", TMPFS_LIMIT_128_MIB, "--tmpfs", "/dev/shm"]);

    for masked_root in ["/home", "/root", "/run", "/media", "/mnt", "/srv", "/tmp", "/var/tmp", "/var/lib"] {
        let masked_root = Path::new(masked_root);
        if !project_root.starts_with(masked_root) {
            continue;
        }
        let Ok(relative) = project_root.strip_prefix(masked_root) else {
            continue;
        };
        let mut visible_path = masked_root.to_path_buf();
        for component in relative.components() {
            visible_path.push(component);
            command.arg("--dir").arg(&visible_path);
        }
        break;
    }

    let project_path = project_root.to_string_lossy().into_owned();
    let timeout_secs = timeout.as_secs().max(1);
    command
        .arg("--bind")
        .arg(&project_root)
        .arg(&project_path)
        .args(["--chdir", &project_path, "--clearenv"])
        .args(["--setenv", "HOME", "/tmp"])
        .args(["--setenv", "TMPDIR", "/tmp"])
        .args(["--setenv", "TMP", "/tmp"])
        .args(["--setenv", "TEMP", "/tmp"])
        .args(["--setenv", "PATH", "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"])
        .args(["--setenv", "LANG", "C.UTF-8"])
        .args(["--setenv", "LC_ALL", "C.UTF-8"])
        .arg("--")
        .arg(&argv[0])
        .args(&argv[1..]);

    let memory_limit = MAX_RUN_MEMORY_BYTES as libc::rlim_t;
    let file_limit = MAX_RUN_FILE_BYTES as libc::rlim_t;
    let cpu_limit = timeout_secs.saturating_add(1) as libc::rlim_t;
    unsafe {
        command.pre_exec(move || {
            let limits = [
                (libc::RLIMIT_AS, memory_limit),
                (libc::RLIMIT_FSIZE, file_limit),
                (libc::RLIMIT_CPU, cpu_limit),
                (libc::RLIMIT_NOFILE, 256),
                (libc::RLIMIT_CORE, 0),
            ];
            for (resource, limit) in limits {
                let value = libc::rlimit {
                    rlim_cur: limit,
                    rlim_max: limit,
                };
                if libc::setrlimit(resource, &value) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    Ok(command)
}

#[cfg(not(target_os = "linux"))]
fn sandbox_command(_argv: &[String], _project_root: &Path, _timeout: Duration) -> Result<Command, String> {
    Err("AI execution is unavailable on this platform because the required Bubblewrap sandbox is Linux-only.".to_string())
}

/// Runs `argv` inside an isolated Linux namespace with a read-only host filesystem,
/// workspace-only writes, no network, bounded resources, and a wall-clock timeout.
pub fn run_in_project(argv: &[String], project_root: &Path, timeout: Duration) -> Result<RunOutcome, String> {
    let started = Instant::now();
    RUN_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let mut child = sandbox_command(argv, project_root, timeout)?
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start AI execution sandbox for '{}': {e}", argv[0]))?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut result = (String::new(), false);
        if let Some(mut pipe) = stdout_pipe.take() {
            result = capture_output(&mut pipe);
        }
        result
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut result = (String::new(), false);
        if let Some(mut pipe) = stderr_pipe.take() {
            result = capture_output(&mut pipe);
        }
        result
    });

    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if RUN_CANCEL_REQUESTED.swap(false, Ordering::SeqCst) {
                    let _ = child.kill();
                    let _ = child.wait();
                    cancelled = true;
                    break None;
                }
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Failed while waiting for '{}': {e}", argv[0])),
        }
    };

    let (stdout, stdout_truncated) = stdout_thread.join().unwrap_or_default();
    let (stderr, stderr_truncated) = stderr_thread.join().unwrap_or_default();

    Ok(RunOutcome {
        command: argv.join(" "),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        cancelled,
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        elapsed_seconds: (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcArtifact {
    pub name: String,
    pub relative_path: String,
    pub bytes: u64,
    pub mime: String,
    pub data_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcOutcome {
    pub script_relative_path: String,
    pub run: RunOutcome,
    pub artifacts: Vec<CalcArtifact>,
    pub skipped_artifacts: usize,
}

fn image_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn snapshot_images(dir: &Path) -> std::collections::HashMap<PathBuf, std::time::SystemTime> {
    let mut out = std::collections::HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if image_mime(&path).is_none() {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                out.insert(path, meta.modified().unwrap_or(std::time::UNIX_EPOCH));
            }
        }
    }
    out
}

/// Pick the interpreter for CALC snippets: the first python on the allowlist.
pub fn calc_interpreter(allowlist: &[String]) -> Result<String, String> {
    ["python3", "python"]
        .iter()
        .find(|p| allowlist.iter().any(|a| a.eq_ignore_ascii_case(p)))
        .map(|p| p.to_string())
        .ok_or_else(|| "CALC blocks need python3 (or python) on the AI run allowlist.".to_string())
}

/// Write `code` into `<root>/.scratch/<stem>.py`, run it there, and collect image files
/// created or modified by the run (base64-encoded via `encode` so this module stays dependency-free).
pub fn run_calc(
    project_root: &Path,
    code: &str,
    interpreter: &str,
    stem: &str,
    timeout: Duration,
    encode: &dyn Fn(&[u8]) -> String,
) -> Result<CalcOutcome, String> {
    if code.trim().is_empty() {
        return Err("CALC block is empty.".to_string());
    }
    let scratch = project_root.join(SCRATCH_DIR);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("Failed to create {}: {e}", scratch.display()))?;
    let safe_stem: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    let safe_stem = if safe_stem.is_empty() { "calc".to_string() } else { safe_stem };
    let script_name = format!("{safe_stem}.py");
    let script_path = scratch.join(&script_name);
    let mut body = code.replace("\r\n", "\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
    std::fs::write(&script_path, body).map_err(|e| format!("Failed to write {}: {e}", script_path.display()))?;

    let before = snapshot_images(&scratch);
    let argv = vec![interpreter.to_string(), script_name.clone()];
    let run = run_in_project(&argv, &scratch, timeout)?;

    let mut changed: Vec<(PathBuf, std::time::SystemTime)> = snapshot_images(&scratch)
        .into_iter()
        .filter(|(path, modified)| before.get(path).map(|old| old < modified).unwrap_or(true))
        .collect();
    changed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let skipped_artifacts = changed.len().saturating_sub(MAX_CALC_ARTIFACTS);
    let artifacts = changed
        .into_iter()
        .take(MAX_CALC_ARTIFACTS)
        .map(|(path, _)| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let mime = image_mime(&path).unwrap_or("application/octet-stream").to_string();
            let data_base64 = if bytes <= MAX_CALC_ARTIFACT_BYTES {
                std::fs::read(&path).ok().map(|raw| encode(&raw))
            } else {
                None
            };
            CalcArtifact { relative_path: format!("{SCRATCH_DIR}/{name}"), name, bytes, mime, data_base64 }
        })
        .collect();
    Ok(CalcOutcome {
        script_relative_path: format!("{SCRATCH_DIR}/{script_name}"),
        run,
        artifacts,
        skipped_artifacts,
    })
}

#[cfg(test)]
mod tests {
    /// The cancel flag is process-global, so tests that spawn child processes must not overlap.
    static PROCESS_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn calc_runs_in_scratch_and_collects_new_images() {
        let _guard = PROCESS_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("ai_calc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let code = "import os\nprint('cwd', os.path.basename(os.getcwd()))\nopen('plot.png','wb').write(b'\\x89PNG fake')\nopen('old.txt','w').write('x')\n";
        let enc = |b: &[u8]| format!("len{}", b.len());
        let outcome = super::run_calc(&root, code, "python3", "calc_1", std::time::Duration::from_secs(20), &enc).unwrap();
        assert_eq!(outcome.run.exit_code, Some(0));
        assert!(outcome.run.stdout.contains("cwd .scratch"), "{}", outcome.run.stdout);
        assert_eq!(outcome.script_relative_path, ".scratch/calc_1.py");
        assert_eq!(outcome.artifacts.len(), 1);
        assert_eq!(outcome.artifacts[0].relative_path, ".scratch/plot.png");
        assert_eq!(outcome.artifacts[0].mime, "image/png");
        assert_eq!(outcome.artifacts[0].data_base64.as_deref(), Some("len9"));
        // a second run that touches nothing reports no artifacts
        let again = super::run_calc(&root, "print(1)", "python3", "calc_2", std::time::Duration::from_secs(20), &enc).unwrap();
        assert!(again.artifacts.is_empty());
        assert!(super::run_calc(&root, "  ", "python3", "x", std::time::Duration::from_secs(5), &enc).is_err());
        let al = vec!["python".to_string()];
        assert_eq!(super::calc_interpreter(&al).unwrap(), "python");
        assert!(super::calc_interpreter(&["node".to_string()]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_request_kills_running_command() {
        let _guard = PROCESS_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("ai_run_cancel_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let argv = vec!["sleep".to_string(), "30".to_string()];
        let root = dir.clone();
        let handle = std::thread::spawn(move || super::run_in_project(&argv, &root, std::time::Duration::from_secs(20)));
        std::thread::sleep(std::time::Duration::from_millis(300));
        super::request_run_cancel();
        let outcome = handle.join().unwrap().unwrap();
        assert!(outcome.cancelled);
        assert!(!outcome.timed_out);
        assert!(outcome.elapsed_seconds < 10.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn parses_multiple_hunks() {
        let body = "<<<<< SEARCH\nfoo = 1\n=====\nfoo = 2\n>>>>> REPLACE\n\n<<<<< SEARCH\nbar()\n=====\nbar(x)\nbaz()\n>>>>> REPLACE\n";
        let hunks = parse_edit_hunks(body).unwrap();
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].search, "foo = 1");
        assert_eq!(hunks[1].replace, "bar(x)\nbaz()");
    }

    #[test]
    fn rejects_unterminated_or_empty_hunks() {
        assert!(parse_edit_hunks("<<<<< SEARCH\nfoo\n=====\nbar\n").is_err());
        assert!(parse_edit_hunks("nothing here").is_err());
        assert!(parse_edit_hunks("<<<<< SEARCH\n=====\nbar\n>>>>> REPLACE\n").is_err());
    }

    #[test]
    fn applies_unique_match_only() {
        let hunks = vec![EditHunk { search: "x = 1".into(), replace: "x = 2".into() }];
        assert_eq!(apply_edit_hunks("a\nx = 1\nb", &hunks).unwrap(), "a\nx = 2\nb");
        assert!(apply_edit_hunks("a\nb", &hunks).unwrap_err().contains("not found"));
        assert!(apply_edit_hunks("x = 1\nx = 1", &hunks).unwrap_err().contains("matches 2"));
    }

    #[test]
    fn allowlist_and_shell_operator_checks() {
        let allow = parse_allowlist("python3, node");
        let root = Path::new("/proj");
        assert_eq!(validate_run_command("python3 sim.py --n 10", &allow, root).unwrap().len(), 4);
        assert!(validate_run_command("rm -rf .", &allow, root).is_err());
        assert!(validate_run_command("python3 a.py | tee x", &allow, root).is_err());
        assert!(validate_run_command("python3 ../escape.py", &allow, root).is_err());
        assert!(validate_run_command("python3 /etc/passwd", &allow, root).is_err());
        assert!(validate_run_command("/usr/bin/python3 /proj/a.py", &allow, root).is_ok());
        assert!(validate_run_command("python3 -c \"import shutil; shutil.rmtree('tests')\"", &allow, root).is_err());
        assert!(validate_run_command("node -e \"require('fs').rmSync('tests',{recursive:true})\"", &allow, root).is_err());
        assert!(validate_run_command("python3 -m pytest tests", &allow, root).is_ok());
        assert_eq!(split_command("python3 -c 'print(\"hi there\")'").unwrap()[2], "print(\"hi there\")");
    }

    #[test]
    fn runs_and_times_out() {
        let _guard = PROCESS_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("ai_run_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let argv: Vec<String> = ["sh", "-c", "echo out; echo err 1>&2; exit 3"].iter().map(|s| s.to_string()).collect();
        let out = run_in_project(&argv, &dir, Duration::from_secs(10)).unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stdout.trim(), "out");
        assert_eq!(out.stderr.trim(), "err");
        assert!(!out.timed_out);

        let argv: Vec<String> = ["sh", "-c", "sleep 5"].iter().map(|s| s.to_string()).collect();
        let out = run_in_project(&argv, &dir, Duration::from_millis(300)).unwrap();
        assert!(out.timed_out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncates_long_output() {
        let big = vec![b'a'; MAX_OUTPUT_BYTES * 3];
        let (text, truncated) = capture_output(big.as_slice());
        assert!(truncated);
        assert!(text.contains("bytes omitted"));
        assert!(text.len() < MAX_OUTPUT_BYTES + 100);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn run_sandbox_limits_writes_to_project_and_disables_network() {
        let _guard = PROCESS_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("ai_run_sandbox_{}", std::process::id()));
        let project = base.join("workspace");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(base.join("outside.txt"), "host-only").unwrap();

        let code = "from pathlib import Path\nimport socket\nPath('inside.txt').write_text('ok')\nprint('inside', Path('inside.txt').read_text())\nprint('outside_visible', Path('/tmp/ai_run_sandbox_{}' + '/outside.txt').exists())\ntry:\n socket.create_connection(('1.1.1.1', 53), timeout=1)\n print('network', 'available')\nexcept OSError:\n print('network', 'blocked')\n"
            .replace("{}", &std::process::id().to_string());
        let argv = vec!["python3".to_string(), "-c".to_string(), code];
        let outcome = run_in_project(&argv, &project, Duration::from_secs(5)).unwrap();
        assert_eq!(outcome.exit_code, Some(0), "{}", outcome.stderr);
        assert!(outcome.stdout.contains("inside ok"), "{}", outcome.stdout);
        assert!(outcome.stdout.contains("outside_visible False"), "{}", outcome.stdout);
        assert!(outcome.stdout.contains("network blocked"), "{}", outcome.stdout);
        assert_eq!(std::fs::read_to_string(project.join("inside.txt")).unwrap(), "ok");
        assert_eq!(std::fs::read_to_string(base.join("outside.txt")).unwrap(), "host-only");
        let _ = std::fs::remove_dir_all(&base);
    }
}
