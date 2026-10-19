//! Canonical operation ledger: every app-mediated AI operation (FILE/APPEND/EDIT/RUN/CALC)
//! is recorded here with its observed outcome. Gates read these rows instead of trusting
//! the model's narration (AI-RIGOR-STRATEGY.md, Tier 1).

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_RESULT_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpRecord {
    #[serde(default)]
    pub id: i64,
    pub pane: String,
    pub thread: String,
    /// "write" | "append" | "edit" | "run" | "calc"
    pub kind: String,
    /// Relative path for file ops; the command line for run; the scratch script for calc.
    pub target: String,
    /// "ok" | "failed" | "refused" | "timeout" | "cancelled"
    pub status: String,
    /// Bounded JSON/text outcome as returned to the model.
    pub result: String,
    #[serde(default)]
    pub sha_before: String,
    #[serde(default)]
    pub sha_after: String,
    #[serde(default)]
    pub created_unix: i64,
}

pub fn sha256_file(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    match std::fs::read(path) {
        Ok(bytes) => format!("{:x}", Sha256::digest(bytes)),
        Err(_) => String::new(),
    }
}

pub fn bound(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_RESULT_CHARS - 20).collect();
    format!("{head}\n…[truncated]")
}

pub fn init(connection: &rusqlite::Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS ops (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 pane TEXT NOT NULL,
                 thread TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 target TEXT NOT NULL,
                 status TEXT NOT NULL,
                 result TEXT NOT NULL DEFAULT '',
                 sha_before TEXT NOT NULL DEFAULT '',
                 sha_after TEXT NOT NULL DEFAULT '',
                 created_unix INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_ops_thread ON ops(pane, thread, id);",
        )
        .map_err(|e| format!("Failed to initialize operation ledger: {e}"))
}

pub fn record(connection: &rusqlite::Connection, rec: &OpRecord) -> Result<i64, String> {
    connection
        .execute(
            "INSERT INTO ops (pane, thread, kind, target, status, result, sha_before, sha_after, created_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                rec.pane,
                rec.thread,
                rec.kind,
                rec.target,
                rec.status,
                bound(&rec.result),
                rec.sha_before,
                rec.sha_after,
                rec.created_unix
            ],
        )
        .map_err(|e| format!("Failed to record operation: {e}"))?;
    Ok(connection.last_insert_rowid())
}

pub fn list(connection: &rusqlite::Connection, pane: &str, thread: &str, limit: usize) -> Result<Vec<OpRecord>, String> {
    let mut stmt = connection
        .prepare(
            "SELECT id, pane, thread, kind, target, status, result, sha_before, sha_after, created_unix
             FROM ops WHERE pane = ?1 AND thread = ?2 ORDER BY id DESC LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![pane, thread, limit as i64], |row| {
            Ok(OpRecord {
                id: row.get(0)?,
                pane: row.get(1)?,
                thread: row.get(2)?,
                kind: row.get(3)?,
                target: row.get(4)?,
                status: row.get(5)?,
                result: row.get(6)?,
                sha_before: row.get(7)?,
                sha_after: row.get(8)?,
                created_unix: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<OpRecord> = rows.filter_map(Result::ok).collect();
    out.reverse();
    Ok(out)
}

const WRITE_VERBS: &str = r"(?:created|wrote|written|saved|placed|generated|updated|edited|appended|patched|produced)";
const RUN_VERBS: &str = r"(?:ran|executed|tested|verified by running|run and)";
const NEGATION: &str = r"(?:not|never|unable|cannot|can't|couldn't|didn't|failed to|no)\b[^.!?\n]{0,40}$";
const PATH_RE: &str = r"(?:[\w.-]+/)*[\w.-]+\.(?:md|txt|json|ya?ml|py|rs|js|ts|html|tex|csv|ipynb|docx?|png|svg)";

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_matches(|c| c == '`' || c == '"' || c == '\'').to_ascii_lowercase()
}

/// Operation-truth gate (G1): every past-tense action claim about a file or run in `reply`
/// must be backed by an `ok` ledger row of the right kind for that target in this thread,
/// and every `op#N` citation must resolve. Returns human-readable issues; empty = pass.
pub fn unbacked_action_claims(reply: &str, ops: &[OpRecord]) -> Vec<String> {
    let path_re = regex::Regex::new(&format!(r"(?i)(?:^|[\s\x60\x22\x27(])({PATH_RE})(?:$|[\s\x60\x22\x27).,;:!?])")).unwrap();
    let write_re = regex::Regex::new(&format!(r"(?i)\b{WRITE_VERBS}\b")).unwrap();
    let run_re = regex::Regex::new(&format!(r"(?i)\b{RUN_VERBS}\b")).unwrap();
    let neg_re = regex::Regex::new(&format!(r"(?i)\b{NEGATION}")).unwrap();
    let cite_re = regex::Regex::new(r"(?i)\bop#(\d+)\b").unwrap();

    let ok_writes: Vec<String> = ops
        .iter()
        .filter(|o| o.status == "ok" && matches!(o.kind.as_str(), "write" | "append" | "edit"))
        .map(|o| basename(&o.target))
        .collect();
    let ok_runs: Vec<String> = ops
        .iter()
        .filter(|o| o.status == "ok" && matches!(o.kind.as_str(), "run" | "calc"))
        .map(|o| o.target.to_ascii_lowercase())
        .collect();

    // Sentence boundaries are punctuation followed by whitespace, so `fixes.md` stays intact.
    let sentence_re = regex::Regex::new(r"[.!?]\s+|\n+").unwrap();
    let mut issues = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for sentence in sentence_re.split(reply) {
        let Some(m) = path_re.captures(sentence) else { continue };
        let path = m.get(1).map(|p| p.as_str()).unwrap_or("");
        let base = basename(path);
        if base.is_empty() || !seen.insert(base.clone()) {
            continue;
        }
        let claims_write = write_re.find(sentence).is_some_and(|v| !neg_re.is_match(&sentence[..v.start()]));
        let claims_run = run_re.find(sentence).is_some_and(|v| !neg_re.is_match(&sentence[..v.start()]));
        if claims_write && !ok_writes.iter().any(|t| t == &base) {
            let detail = ops
                .iter()
                .filter(|o| basename(&o.target) == base)
                .last()
                .map(|o| format!(" (last ledger entry for it: op#{} {} → {})", o.id, o.kind, o.status))
                .unwrap_or_default();
            issues.push(format!(
                "\"{}\" claims {path} was written, but the ledger has no successful FILE/APPEND/EDIT operation for it in this thread{detail}.",
                sentence.trim().chars().take(100).collect::<String>()
            ));
        } else if claims_run && !ok_runs.iter().any(|t| t.contains(&base)) {
            issues.push(format!(
                "\"{}\" claims {path} was run, but the ledger has no successful RUN/CALC involving it in this thread.",
                sentence.trim().chars().take(100).collect::<String>()
            ));
        }
    }
    for cap in cite_re.captures_iter(reply) {
        let id: i64 = cap[1].parse().unwrap_or(-1);
        match ops.iter().find(|o| o.id == id) {
            None => issues.push(format!("op#{id} is cited but does not exist in this thread's ledger.")),
            Some(o) if o.status != "ok" => issues.push(format!("op#{id} is cited as evidence but its status is {}.", o.status)),
            Some(_) => {}
        }
    }
    issues
}

/// Fresh-state check: successful write ops whose file has since changed or vanished.
pub fn stale_writes(ops: &[OpRecord], root: &Path) -> Vec<String> {
    ops.iter()
        .filter(|o| o.status == "ok" && matches!(o.kind.as_str(), "write" | "append" | "edit") && !o.sha_after.is_empty())
        .filter_map(|o| {
            let now = sha256_file(&root.join(&o.target));
            if now.is_empty() {
                Some(format!("op#{} wrote {} but the file no longer exists.", o.id, o.target))
            } else if now != o.sha_after {
                Some(format!("op#{} wrote {} but the file has changed since (outside the ledger).", o.id, o.target))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: i64, kind: &str, target: &str, status: &str) -> OpRecord {
        OpRecord { id, pane: "left".into(), thread: "0".into(), kind: kind.into(), target: target.into(), status: status.into(), result: String::new(), sha_before: String::new(), sha_after: String::new(), created_unix: 0 }
    }

    #[test]
    fn ledger_roundtrip_in_memory() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        let id = record(&c, &op(0, "write", "manuscript/md/fixes.md", "ok")).unwrap();
        assert_eq!(id, 1);
        let rows = list(&c, "left", "0", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target, "manuscript/md/fixes.md");
        assert!(list(&c, "right", "0", 10).unwrap().is_empty());
    }

    #[test]
    fn fixes_md_lie_is_caught_and_honest_report_passes() {
        let ops = vec![op(1, "read", "fixes.md", "failed")];
        let lie = "The file `fixes.md` has been created and placed into the `manuscript/md/` directory.";
        let issues = unbacked_action_claims(lie, &ops);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("op#1 read → failed"));

        let honest = "fixes.md was not created: the read returned file not found.";
        assert!(unbacked_action_claims(honest, &ops).is_empty());

        let backed = vec![op(2, "write", "manuscript/md/fixes.md", "ok")];
        assert!(unbacked_action_claims(lie, &backed).is_empty());
    }

    #[test]
    fn run_claims_and_citations_resolve_against_ledger() {
        let ops = vec![op(3, "calc", ".scratch/calc_1.py", "ok"), op(4, "run", "python3 tests/predictions.py", "timeout")];
        assert!(unbacked_action_claims("I ran tests/predictions.py and it passed.", &ops).iter().any(|i| i.contains("was run")));
        assert!(unbacked_action_claims("Result from op#3: 1.616e-35 m.", &ops).is_empty());
        let bad = unbacked_action_claims("See op#4 and op#9.", &ops);
        assert_eq!(bad.len(), 2, "{bad:?}");
    }

    #[test]
    fn bound_truncates_long_results() {
        let long = "x".repeat(MAX_RESULT_CHARS + 500);
        let b = bound(&long);
        assert!(b.ends_with("[truncated]") && b.chars().count() <= MAX_RESULT_CHARS);
    }
}
