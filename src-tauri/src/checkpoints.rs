//! Local git checkpoints for AI file operations.
//!
//! Each project root gets a shadow repository stored under the app's local
//! data directory (`--git-dir` outside the project, `--work-tree` = project
//! root), so checkpoints never touch the user's own `.git` history.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_LIST_LIMIT: usize = 20;
pub const MAX_LIST_LIMIT: usize = 200;
const COMMITTER_NAME: &str = "Physics IDE";
const COMMITTER_EMAIL: &str = "checkpoints@physics-ide.local";
const DEFAULT_EXCLUDES: &str = "\
# Physics IDE checkpoint excludes (the project's own .gitignore files also apply)
.git/
node_modules/
target/
__pycache__/
.venv/
venv/
*.pyc
*.sqlite3
*.sqlite3-wal
*.sqlite3-shm
*.bak
";

#[derive(Debug, Clone, Serialize)]
pub struct Checkpoint {
    pub sha: String,
    pub short_sha: String,
    pub timestamp: i64,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointOutcome {
    pub action: String,
    pub sha: Option<String>,
    pub short_sha: Option<String>,
    pub files_changed: usize,
    pub subject: String,
}

pub struct ShadowRepo {
    git_dir: PathBuf,
    work_tree: PathBuf,
}

impl ShadowRepo {
    pub fn new(git_dir: PathBuf, work_tree: PathBuf) -> Self {
        Self { git_dir, work_tree }
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(&self.git_dir)
            .arg("--work-tree")
            .arg(&self.work_tree)
            .args(["-c", &format!("user.name={COMMITTER_NAME}"), "-c", &format!("user.email={COMMITTER_EMAIL}")])
            .args(["-c", "core.autocrlf=false", "-c", "core.safecrlf=false"])
            .args(args)
            .current_dir(&self.work_tree)
            .output()
            .map_err(|e| format!("Failed to run git (is git installed?): {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git {} failed: {}", args.first().copied().unwrap_or(""), stderr));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub fn ensure(&self) -> Result<bool, String> {
        if self.git_dir.join("HEAD").exists() {
            return Ok(false);
        }
        fs::create_dir_all(&self.git_dir).map_err(|e| format!("Failed to create checkpoint directory: {e}"))?;
        let output = Command::new("git")
            .args(["init", "--quiet", "--bare", "--initial-branch=checkpoints"])
            .arg(&self.git_dir)
            .output()
            .map_err(|e| format!("Failed to run git init (is git installed?): {e}"))?;
        if !output.status.success() {
            // Older git without --initial-branch.
            let retry = Command::new("git")
                .args(["init", "--quiet", "--bare"])
                .arg(&self.git_dir)
                .output()
                .map_err(|e| format!("Failed to run git init: {e}"))?;
            if !retry.status.success() {
                return Err(format!("git init failed: {}", String::from_utf8_lossy(&retry.stderr).trim()));
            }
        }
        let info_dir = self.git_dir.join("info");
        fs::create_dir_all(&info_dir).map_err(|e| format!("Failed to create git info dir: {e}"))?;
        fs::write(info_dir.join("exclude"), DEFAULT_EXCLUDES).map_err(|e| format!("Failed to write excludes: {e}"))?;
        // A bare repo refuses work-tree operations unless this is cleared.
        let _ = self.git(&["config", "core.bare", "false"]);
        let _ = self.git(&["config", "core.logallrefupdates", "true"]);
        Ok(true)
    }

    fn has_commits(&self) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok()
    }

    fn staged_change_count(&self) -> Result<usize, String> {
        let args: &[&str] = if self.has_commits() {
            &["diff", "--cached", "--name-only", "HEAD"]
        } else {
            &["diff", "--cached", "--name-only"]
        };
        Ok(self.git(args)?.lines().filter(|l| !l.trim().is_empty()).count())
    }

    pub fn create(&self, subject: &str) -> Result<CheckpointOutcome, String> {
        self.ensure()?;
        let subject = sanitize_subject(subject);
        self.git(&["add", "--all", "--", "."])?;
        let files_changed = self.staged_change_count()?;
        if files_changed == 0 {
            return Ok(CheckpointOutcome {
                action: "unchanged".to_string(),
                sha: None,
                short_sha: None,
                files_changed: 0,
                subject,
            });
        }
        self.git(&["commit", "--quiet", "--no-verify", "-m", &subject])?;
        let sha = self.git(&["rev-parse", "HEAD"])?.trim().to_string();
        Ok(CheckpointOutcome {
            action: "created".to_string(),
            short_sha: Some(sha.chars().take(8).collect()),
            sha: Some(sha),
            files_changed,
            subject,
        })
    }

    pub fn list(&self, limit: usize) -> Result<Vec<Checkpoint>, String> {
        if !self.git_dir.join("HEAD").exists() || !self.has_commits() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, MAX_LIST_LIMIT).to_string();
        let raw = self.git(&["log", "--format=%H%x1f%ct%x1f%s", "-n", &limit])?;
        Ok(parse_log(&raw))
    }

    pub fn files_in(&self, sha: &str) -> Result<Vec<String>, String> {
        validate_sha(sha)?;
        let raw = self.git(&["show", "--format=", "--name-status", "--no-renames", sha])?;
        Ok(raw.lines().filter(|l| !l.trim().is_empty()).map(|l| l.replace('\t', " ")).collect())
    }

    /// Restore the work tree to the state recorded in `sha`, recording a safety
    /// checkpoint first and a "Reverted" checkpoint afterwards. History is never rewritten.
    pub fn revert_to(&self, sha: &str) -> Result<CheckpointOutcome, String> {
        validate_sha(sha)?;
        self.ensure()?;
        let short: String = sha.chars().take(8).collect();
        self.create(&format!("Before revert to {short}"))?;
        let now_tracked = self.git(&["ls-files"])?;
        let target_tracked = self.git(&["ls-tree", "-r", "--name-only", sha])?;
        let target_set: std::collections::BTreeSet<&str> = target_tracked.lines().collect();
        for path in now_tracked.lines().filter(|p| !p.is_empty() && !target_set.contains(p)) {
            let full = self.work_tree.join(path);
            if full.is_file() {
                fs::remove_file(&full).map_err(|e| format!("Failed to remove {path}: {e}"))?;
            }
        }
        if !target_set.is_empty() {
            self.git(&["checkout", "--quiet", sha, "--", "."])?;
        }
        self.create(&format!("Reverted to {short}"))
    }
}

pub fn sanitize_subject(subject: &str) -> String {
    let flat = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    let flat = if flat.is_empty() { "Checkpoint".to_string() } else { flat };
    flat.chars().take(160).collect()
}

fn validate_sha(sha: &str) -> Result<(), String> {
    if sha.len() < 7 || sha.len() > 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Invalid checkpoint id: {sha}"));
    }
    Ok(())
}

pub fn parse_log(raw: &str) -> Vec<Checkpoint> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            let sha = parts.next()?.trim().to_string();
            let timestamp = parts.next()?.trim().parse::<i64>().ok()?;
            let subject = parts.next().unwrap_or("").to_string();
            if sha.is_empty() {
                return None;
            }
            Some(Checkpoint { short_sha: sha.chars().take(8).collect(), sha, timestamp, subject })
        })
        .collect()
}

pub fn shadow_git_dir(app_local_data: &Path, project_root: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
    let digest = format!("{:x}", Sha256::digest(canonical.to_string_lossy().as_bytes()));
    app_local_data.join("checkpoints").join(&digest[..20]).join("shadow.git")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pair(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("physics_ide_ckpt_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let work = base.join("project");
        fs::create_dir_all(&work).unwrap();
        (base.join("data"), work)
    }

    #[test]
    fn parse_log_handles_units() {
        let rows = parse_log("abcdef1234567890\u{1f}1700000000\u{1f}AI edits: FILE a.py\nbad line\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].short_sha, "abcdef12");
        assert_eq!(rows[0].subject, "AI edits: FILE a.py");
    }

    #[test]
    fn create_list_and_revert_round_trip() {
        let (data, work) = temp_pair("roundtrip");
        let repo = ShadowRepo::new(shadow_git_dir(&data, &work), work.clone());
        fs::write(work.join("a.py"), "print(1)\n").unwrap();
        let first = repo.create("first").unwrap();
        assert_eq!(first.action, "created");
        assert_eq!(first.files_changed, 1);
        assert_eq!(repo.create("again").unwrap().action, "unchanged");

        fs::write(work.join("a.py"), "print(2)\n").unwrap();
        fs::write(work.join("b.py"), "x\n").unwrap();
        let second = repo.create("second").unwrap();
        assert_eq!(second.files_changed, 2);
        assert_eq!(repo.list(10).unwrap().len(), 2);
        assert!(repo.files_in(first.sha.as_deref().unwrap()).unwrap().iter().any(|l| l.contains("a.py")));

        fs::write(work.join("a.py"), "print(3)  # uncommitted\n").unwrap();
        let reverted = repo.revert_to(first.sha.as_deref().unwrap()).unwrap();
        assert_eq!(reverted.action, "created");
        assert_eq!(fs::read_to_string(work.join("a.py")).unwrap(), "print(1)\n");
        assert!(!work.join("b.py").exists());
        assert!(!work.join(".git").exists(), "shadow repo must not live in the project");
        let subjects: Vec<String> = repo.list(10).unwrap().into_iter().map(|c| c.subject).collect();
        assert!(subjects[0].starts_with("Reverted to"));
        assert!(subjects[1].starts_with("Before revert"));
    }

    #[test]
    fn rejects_bad_sha_and_respects_excludes() {
        let (data, work) = temp_pair("excludes");
        let repo = ShadowRepo::new(shadow_git_dir(&data, &work), work.clone());
        assert!(repo.revert_to("not-a-sha").is_err());
        fs::create_dir_all(work.join("__pycache__")).unwrap();
        fs::write(work.join("__pycache__/x.pyc"), "0").unwrap();
        fs::write(work.join("keep.md"), "k").unwrap();
        let out = repo.create("ex").unwrap();
        assert_eq!(out.files_changed, 1);
    }
}
