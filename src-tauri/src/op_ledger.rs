//! Canonical operation ledger: every app-mediated AI operation (FILE/APPEND/EDIT/RUN/CALC/READ/SEARCH)
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
    /// "write" | "append" | "edit" | "run" | "calc" | "read" | "search"
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
             CREATE INDEX IF NOT EXISTS idx_ops_thread ON ops(pane, thread, id);
             CREATE TABLE IF NOT EXISTS gate_runs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 pane TEXT NOT NULL,
                 thread TEXT NOT NULL,
                 attempt INTEGER NOT NULL,
                 verdict TEXT NOT NULL,
                 results TEXT NOT NULL DEFAULT '',
                 created_unix INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_gate_runs_thread ON gate_runs(pane, thread, id);",
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
/// A sentence retracting an earlier claim ("the incorrect claim that X was written") is not itself a claim.
const RETRACTION: &str = r"(?i)\b(?:(?:incorrect|false|wrong|mistaken|erroneous|unbacked|unsupported|inaccurate)\s+(?:claims?|statements?|assertions?|reports?)|(?:falsely|wrongly|incorrectly|mistakenly)\s+(?:claimed|stated|said|reported))\b(?:[^.!?\n]|[.!?]\S){0,80}$";
/// A path introduced as the topic of another file ("fixes for `sims/toy.py`") is not the written target.
const TOPIC_BEFORE: &str = r"(?i)\b(?:for|about|regarding|concerning|of)\s+(?:the\s+)?(?:file\s+|script\s+)?[`'\x22]?$";
/// Run/test outcomes reported without naming a file ("all tests passed", "I ran the simulation").
const PATHLESS_RUN_CLAIM: &str = r"(?i)\b(?:(?:all\s+)?(?:the\s+)?tests?\s+(?:now\s+)?(?:pass(?:ed|es)?|succeed(?:ed|s)?)|(?:ran|executed)\s+(?:the\s+|a\s+|my\s+)?(?:script|tests?|simulation|calculation|code|sweep)s?)\b";
const FETCH_VERBS: &str = r"(?:fetched|retrieved|downloaded|looked up|according to|as stated (?:on|in|at)|the (?:page|abstract|article) (?:says|states|reports|lists))";
const UNCHECKED_SOURCE: &str = r"(?i)\b(?:unverified|from memory|not fetched|unchecked|not checked|not verified|could not fetch|couldn't fetch|to be checked)\b";
/// A citation the sentence refuses or denies ("I cannot state that arXiv:… supports …") is not a citation.
const REFUSED_SOURCE: &str = r"(?i)\b(?:cannot|can't|can not|could not|couldn't|unable to|will not|won't|do not|don't|did not|didn't|not able to|refuse to|decline to)\b(?:[^.!?\n]|[.!?]\S){0,80}$";
const ARXIV_ID: &str = r"(?i)\barxiv(?:\.org/(?:abs|pdf)/|:\s*|\s+)(\d{4}\.\d{4,5}|[a-z-]+(?:\.[a-z]{2})?/\d{7})";
const PATH_RE: &str = r"(?:[\w.-]+/)*[\w.-]+\.(?:md|txt|json|ya?ml|py|rs|js|ts|html|tex|csv|ipynb|docx?|png|svg)";

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_matches(|c| c == '`' || c == '"' || c == '\'').to_ascii_lowercase()
}

fn path_mentions(sentence: &str, path_re: &regex::Regex) -> Vec<(usize, String)> {
    let is_delim = |c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | ')' | '.' | ',' | ';' | ':' | '!' | '?' | '*' | '[' | ']');
    path_re
        .find_iter(sentence)
        .filter(|m| {
            let before = sentence[..m.start()].chars().next_back();
            let after = sentence[m.end()..].chars().next();
            before.map_or(true, |c| is_delim(c) && c != '.') && after.map_or(true, is_delim)
        })
        .map(|m| (m.start(), m.as_str().to_string()))
        .collect()
}

/// Operation-truth gate (G1): every past-tense action claim about a file or run in `reply`
/// must be backed by an `ok` ledger row of the right kind for that target in this thread,
/// and every `op#N` citation must resolve. Returns human-readable issues; empty = pass.
pub fn unbacked_action_claims(reply: &str, ops: &[OpRecord]) -> Vec<String> {
    let path_re = regex::Regex::new(&format!(r"(?i){PATH_RE}")).unwrap();
    let write_re = regex::Regex::new(&format!(r"(?i)\b{WRITE_VERBS}\b")).unwrap();
    let run_re = regex::Regex::new(&format!(r"(?i)\b{RUN_VERBS}\b")).unwrap();
    let neg_re = regex::Regex::new(&format!(r"(?i)\b{NEGATION}")).unwrap();
    let retract_re = regex::Regex::new(RETRACTION).unwrap();
    let topic_re = regex::Regex::new(TOPIC_BEFORE).unwrap();
    let cite_re = regex::Regex::new(r"(?i)\bop#(\d+)\b").unwrap();
    let pathless_run_re = regex::Regex::new(PATHLESS_RUN_CLAIM).unwrap();

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
    let affirmed = |sentence: &str, verb: &regex::Regex| {
        verb.find_iter(sentence).any(|v| !neg_re.is_match(&sentence[..v.start()]) && !retract_re.is_match(&sentence[..v.start()]))
    };

    // Sentence boundaries are punctuation followed by whitespace, so `fixes.md` stays intact.
    let sentence_re = regex::Regex::new(r"[.!?]\s+|\n+").unwrap();
    let mut issues = Vec::new();
    // Only reported targets are de-duplicated: an earlier honest sentence about a file must not
    // hide a later false claim about the same file.
    let mut reported = std::collections::BTreeSet::new();
    let mut pathless_reported = false;
    for sentence in sentence_re.split(reply) {
        let claims_write = affirmed(sentence, &write_re);
        let claims_run = affirmed(sentence, &run_re);
        let mentions = path_mentions(sentence, &path_re);
        let is_topic: Vec<bool> = mentions.iter().map(|(at, _)| topic_re.is_match(&sentence[..*at])).collect();
        // Topic mentions are skipped only when the sentence names another target for the action.
        let has_target = is_topic.iter().any(|t| !t);
        let paths: Vec<&String> = mentions.iter().zip(&is_topic).filter(|(_, t)| !(has_target && **t)).map(|((_, p), _)| p).collect();
        for path in paths.iter().copied() {
            let base = basename(path);
            if base.is_empty() || reported.contains(&base) {
                continue;
            }
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
                reported.insert(base);
            } else if claims_run && !claims_write && !ok_runs.iter().any(|t| t.contains(&base)) {
                issues.push(format!(
                    "\"{}\" claims {path} was run, but the ledger has no successful RUN/CALC involving it in this thread.",
                    sentence.trim().chars().take(100).collect::<String>()
                ));
                reported.insert(base);
            }
        }
        if mentions.is_empty() && !pathless_reported && ok_runs.is_empty() {
            if let Some(m) = pathless_run_re.find(sentence) {
                if !neg_re.is_match(&sentence[..m.start()]) {
                    let last = ops
                        .iter()
                        .filter(|o| matches!(o.kind.as_str(), "run" | "calc"))
                        .last()
                        .map(|o| format!(" (last run in the ledger: op#{} → {})", o.id, o.status))
                        .unwrap_or_default();
                    issues.push(format!(
                        "\"{}\" reports a run or test outcome, but the ledger has no successful RUN/CALC in this thread{last}.",
                        sentence.trim().chars().take(100).collect::<String>()
                    ));
                    pathless_reported = true;
                }
            }
        }
    }
    // Sources: an arXiv citation, or "according to <url>", needs a successful FETCH in this thread
    // unless the sentence marks it as unverified.
    let unchecked_re = regex::Regex::new(UNCHECKED_SOURCE).unwrap();
    let arxiv_re = regex::Regex::new(ARXIV_ID).unwrap();
    let refused_re = regex::Regex::new(REFUSED_SOURCE).unwrap();
    let url_re = regex::Regex::new(r#"https?://[^\s)\]>`'"]+"#).unwrap();
    let fetch_re = regex::Regex::new(&format!(r"(?i)\b{FETCH_VERBS}")).unwrap();
    let ok_fetches: Vec<String> = ops.iter().filter(|o| o.status == "ok" && o.kind == "fetch").map(|o| o.target.to_ascii_lowercase()).collect();
    let mut source_reported = std::collections::BTreeSet::new();
    for sentence in sentence_re.split(reply) {
        if unchecked_re.is_match(sentence) {
            continue;
        }
        let quote = sentence.trim().chars().take(100).collect::<String>();
        for cap in arxiv_re.captures_iter(sentence) {
            let id = cap[1].to_ascii_lowercase();
            if refused_re.is_match(&sentence[..cap.get(0).unwrap().start()]) {
                continue;
            }
            if ok_fetches.iter().any(|t| t.contains(&id)) || !source_reported.insert(id.clone()) {
                continue;
            }
            issues.push(format!("\"{quote}\" cites arXiv:{} but no successful FETCH of it exists in this thread; fetch it, mark it (unverified, from memory), or drop the claim.", &cap[1]));
        }
        if affirmed(sentence, &fetch_re) {
            for m in url_re.find_iter(sentence) {
                let url = m.as_str().trim_end_matches(['.', ',', ';', ':']).to_ascii_lowercase();
                if arxiv_re.is_match(&url) || ok_fetches.iter().any(|t| t.trim_end_matches('/') == url.trim_end_matches('/')) || !source_reported.insert(url.clone()) {
                    continue;
                }
                issues.push(format!("\"{quote}\" attributes content to {url}, but no successful FETCH of it exists in this thread."));
            }
        }
    }
    let mut cited = std::collections::BTreeSet::new();
    for cap in cite_re.captures_iter(reply) {
        let id: i64 = cap[1].parse().unwrap_or(-1);
        if !cited.insert(id) {
            continue;
        }
        match ops.iter().find(|o| o.id == id) {
            None => issues.push(format!("op#{id} is cited but does not exist in this thread's ledger.")),
            Some(o) if o.status != "ok" => issues.push(format!("op#{id} is cited as evidence but its status is {}.", o.status)),
            Some(_) => {}
        }
    }
    issues
}

/// Fresh-state check: the latest successful write per file whose content has since changed or
/// vanished outside the ledger. These are warnings (the model cannot fix a user's edit), not gate failures.
pub fn stale_writes(ops: &[OpRecord], root: &Path) -> Vec<String> {
    let mut latest: std::collections::BTreeMap<&str, &OpRecord> = std::collections::BTreeMap::new();
    for o in ops.iter().filter(|o| matches!(o.kind.as_str(), "write" | "append" | "edit")) {
        latest.insert(o.target.as_str(), o);
    }
    latest
        .values()
        .filter(|o| o.status == "ok" && !o.sha_after.is_empty())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateRun {
    #[serde(default)]
    pub id: i64,
    pub pane: String,
    pub thread: String,
    /// Draft number within the turn (1 = first reply, 2+ = after send-backs).
    pub attempt: i64,
    /// "pass" | "send_back" | "unverified" | "report_only"
    pub verdict: String,
    /// JSON array of gate results as rendered on the card.
    pub results: String,
    #[serde(default)]
    pub created_unix: i64,
}

pub fn record_gate_run(connection: &rusqlite::Connection, run: &GateRun) -> Result<i64, String> {
    connection
        .execute(
            "INSERT INTO gate_runs (pane, thread, attempt, verdict, results, created_unix) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![run.pane, run.thread, run.attempt, run.verdict, bound(&run.results), run.created_unix],
        )
        .map_err(|e| format!("Failed to record gate run: {e}"))?;
    Ok(connection.last_insert_rowid())
}

pub fn list_gate_runs(connection: &rusqlite::Connection, pane: &str, thread: &str, limit: usize) -> Result<Vec<GateRun>, String> {
    let mut stmt = connection
        .prepare("SELECT id, pane, thread, attempt, verdict, results, created_unix FROM gate_runs WHERE pane = ?1 AND thread = ?2 ORDER BY id DESC LIMIT ?3")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![pane, thread, limit as i64], |row| {
            Ok(GateRun {
                id: row.get(0)?,
                pane: row.get(1)?,
                thread: row.get(2)?,
                attempt: row.get(3)?,
                verdict: row.get(4)?,
                results: row.get(5)?,
                created_unix: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<GateRun> = rows.filter_map(Result::ok).collect();
    out.reverse();
    Ok(out)
}

fn md_cell(text: &str, limit: usize) -> String {
    let flat: String = text.replace(['\n', '\r'], " ").replace('|', "\\|");
    let mut out: String = flat.chars().take(limit).collect();
    if flat.chars().count() > limit {
        out.push('…');
    }
    out
}

/// Human-readable audit of one thread: what the app observed (ops) and what the gates decided.
/// `transcript` is the thread's message history as the UI shows it (may be empty).
pub fn audit_markdown(pane: &str, thread: &str, ops: &[OpRecord], gates: &[GateRun], transcript: &str, generated_unix: i64) -> String {
    let mut md = format!("# Thread audit — {pane} pane\n\nThread: `{thread}`  \nGenerated (unix): {generated_unix}  \nOperations: {}  \nGate runs: {}\n\n", ops.len(), gates.len());
    let mut by_kind: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
    for o in ops {
        *by_kind.entry((o.kind.clone(), o.status.clone())).or_default() += 1;
    }
    let mut verdicts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for g in gates {
        *verdicts.entry(g.verdict.clone()).or_default() += 1;
    }
    md.push_str("## Summary\n\n| op kind | status | count |\n|---|---|---|\n");
    for ((k, st), n) in &by_kind {
        md.push_str(&format!("| {k} | {st} | {n} |\n"));
    }
    md.push_str("\n| gate verdict | count |\n|---|---|\n");
    for (v, n) in &verdicts {
        md.push_str(&format!("| {v} | {n} |\n"));
    }
    md.push_str("\n## Operations (what the app observed)\n\n| op# | unix | kind | target | status | result |\n|---|---|---|---|---|---|\n");
    for o in ops {
        md.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n", o.id, o.created_unix, o.kind, md_cell(&o.target, 80), o.status, md_cell(&o.result, 160)));
    }
    md.push_str("\n## Gate runs (what the gates decided per draft)\n\n");
    for g in gates {
        md.push_str(&format!("### Gate run {} — draft {} — **{}** (unix {})\n\n", g.id, g.attempt, g.verdict, g.created_unix));
        match serde_json::from_str::<Vec<serde_json::Value>>(&g.results) {
            Ok(rows) => {
                md.push_str("| gate | status | issues / reason |\n|---|---|---|\n");
                for r in rows {
                    let issues: Vec<String> = r["issues"].as_array().map(|a| a.iter().filter_map(|i| i.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    let mut detail = if issues.is_empty() { r["reason"].as_str().unwrap_or("").to_string() } else { issues.join(" • ") };
                    for w in r["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str()) {
                        detail.push_str(if detail.is_empty() { "" } else { " • " });
                        detail.push_str(&format!("warning: {w}"));
                    }
                    md.push_str(&format!("| {} | {} | {} |\n", r["gate"].as_str().unwrap_or("?"), r["status"].as_str().unwrap_or("?"), md_cell(&detail, 400)));
                }
                md.push('\n');
            }
            Err(_) => md.push_str(&format!("```\n{}\n```\n\n", g.results)),
        }
    }
    if !transcript.trim().is_empty() {
        md.push_str("## Transcript (as shown in the thread)\n\n");
        md.push_str(transcript.trim());
        md.push('\n');
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_claims_need_a_successful_fetch() {
        let ops = vec![op(1, "fetch", "https://arxiv.org/abs/2401.01234", "ok"), op(2, "fetch", "https://en.wikipedia.org/wiki/Brane", "error")];
        assert!(unbacked_action_claims("Per arXiv:2401.01234 the tension is 1 TeV.", &ops).is_empty());
        assert_eq!(unbacked_action_claims("See arXiv:2305.99999 for the bound. Also arXiv:2305.99999 again.", &ops).len(), 1);
        assert!(unbacked_action_claims("See arXiv:2305.99999 (unverified, from memory).", &ops).is_empty());
        let refusal = "I cannot state that arXiv:2001.00001 supports a brane tension of 1 TeV and cite it as my source, as I have not fetched it.";
        assert!(unbacked_action_claims(refusal, &ops).is_empty(), "a refused citation is not a citation");
        assert!(unbacked_action_claims("I don't have access to arXiv:2001.00001.", &ops).is_empty());
        let claim = unbacked_action_claims("I cannot overstate it: arXiv:2001.00001 shows a 1 TeV tension. arXiv:2001.00001 is the source.", &ops);
        assert_eq!(claim.len(), 1, "a later bare citation is still caught: {claim:?}");
        assert!(claim[0].contains("or drop the claim"));
        assert_eq!(unbacked_action_claims("According to https://en.wikipedia.org/wiki/Brane, branes exist.", &ops).len(), 1);
        assert!(unbacked_action_claims("I could not fetch https://en.wikipedia.org/wiki/Brane, so this is unchecked.", &ops).is_empty());
        assert!(unbacked_action_claims("You can browse https://example.com yourself.", &ops).is_empty());
    }

    #[test]
    fn audit_markdown_lists_ops_gates_and_transcript() {
        let mut o = op(3, "write", "notes/fixes.md", "error");
        o.result = "file not found | denied\nline2".into();
        let g = GateRun { id: 1, pane: "left".into(), thread: "t".into(), attempt: 1, verdict: "send_back".into(), results: r#"[{"gate":"G1","status":"fail","issues":["claims fixes.md was written"],"reason":"","warnings":["op#2 wrote a.md but the file has changed since (outside the ledger)."]},{"gate":"G2","status":"skip","issues":[],"reason":"not built"}]"#.into(), created_unix: 5 };
        let md = audit_markdown("left", "t", &[o, op(4, "run", "python3 a.py", "ok")], &[g], "**You:** go", 9);
        assert!(md.contains("| write | error | 1 |"));
        assert!(md.contains("| send_back | 1 |"));
        assert!(md.contains("| 3 | 0 | write | notes/fixes.md | error | file not found \\| denied line2 |"));
        assert!(md.contains("| G1 | fail | claims fixes.md was written • warning: op#2 wrote a.md but the file has changed since (outside the ledger). |"));
        assert!(md.contains("| G2 | skip | not built |"));
        assert!(md.ends_with("## Transcript (as shown in the thread)\n\n**You:** go\n"));
    }

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

    #[test]
    fn honest_sentence_does_not_hide_a_later_false_claim_about_the_same_file() {
        let ops = vec![op(1, "read", "fixes.md", "failed")];
        let reply = "I could not read fixes.md at first. After review, fixes.md has been created with the nine items.";
        let issues = unbacked_action_claims(reply, &ops);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("has been created"));
    }

    #[test]
    fn every_path_in_a_sentence_is_checked() {
        let ops = vec![op(1, "write", "notes/a.md", "ok")];
        let issues = unbacked_action_claims("I wrote a.md and b.py for you.", &ops);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("b.py"));
        assert!(unbacked_action_claims("Created a.md, see https://example.org/x.html for context.", &ops).is_empty());
    }

    #[test]
    fn a_path_named_as_the_topic_of_a_write_is_not_the_written_file() {
        let ops = vec![op(35, "write", "fixes.md", "ok"), op(33, "setup", "sims/toy.py", "ok")];
        assert!(unbacked_action_claims("The file `fixes.md` was successfully created, containing three specific proposed fixes for `sims/toy.py`.", &ops).is_empty());
        assert!(unbacked_action_claims("The file fixes.md has been created and checked, confirming the listing of fixes for sims/toy.py.", &ops).is_empty());
        let lone = unbacked_action_claims("Updated the initialization logic for sims/toy.py.", &ops);
        assert_eq!(lone.len(), 1, "a lone path after `for` is still the claimed target: {lone:?}");
        let both = unbacked_action_claims("I updated sims/toy.py and wrote fixes.md about it.", &ops);
        assert_eq!(both.len(), 1, "{both:?}");
        assert!(both[0].contains("sims/toy.py"));
    }

    #[test]
    fn retracting_an_earlier_claim_is_not_a_new_claim() {
        let ops = vec![op(33, "setup", "sims/toy.py", "ok")];
        assert!(unbacked_action_claims("The error in the previous responses was an incorrect claim that `sims/toy.py` was written or modified.", &ops).is_empty());
        assert!(unbacked_action_claims("I falsely claimed that sims/toy.py was updated.", &ops).is_empty());
        assert_eq!(unbacked_action_claims("I fixed the wrong sign and updated sims/toy.py.", &ops).len(), 1);
    }

    #[test]
    fn pathless_run_claims_need_a_successful_run() {
        let failed = vec![op(5, "run", "python3 tests/predictions.py", "failed")];
        let issues = unbacked_action_claims("All tests passed and the model is consistent.", &failed);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("op#5 → failed"));
        assert!(unbacked_action_claims("The tests did not pass; see the traceback.", &failed).is_empty());
        let ok = vec![op(6, "calc", ".scratch/calc_2.py", "ok")];
        assert!(unbacked_action_claims("I ran the calculation; the value is 3.2 GeV.", &ok).is_empty());
        assert!(unbacked_action_claims("You could run the tests next.", &[]).is_empty());
    }

    #[test]
    fn stale_check_uses_the_latest_write_per_file_only() {
        let dir = std::env::temp_dir().join(format!("op_ledger_stale_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "v1").unwrap();
        let v1 = sha256_file(&dir.join("a.md"));
        std::fs::write(dir.join("a.md"), "v2").unwrap();
        let v2 = sha256_file(&dir.join("a.md"));
        let mut first = op(1, "write", "a.md", "ok");
        first.sha_after = v1;
        let mut second = op(2, "edit", "a.md", "ok");
        second.sha_after = v2;
        assert!(stale_writes(&[first.clone(), second], &dir).is_empty());
        let stale = stale_writes(&[first], &dir);
        assert_eq!(stale.len(), 1, "{stale:?}");
        std::fs::remove_file(dir.join("a.md")).unwrap();
        let mut gone = op(3, "write", "a.md", "ok");
        gone.sha_after = "x".into();
        assert!(stale_writes(&[gone], &dir)[0].contains("no longer exists"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gate_runs_roundtrip_per_thread() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        let run = GateRun { id: 0, pane: "left".into(), thread: "left-0-abc".into(), attempt: 1, verdict: "send_back".into(), results: "[]".into(), created_unix: 1 };
        record_gate_run(&c, &run).unwrap();
        record_gate_run(&c, &GateRun { attempt: 2, verdict: "pass".into(), ..run.clone() }).unwrap();
        let rows = list_gate_runs(&c, "left", "left-0-abc", 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.verdict.as_str()).collect::<Vec<_>>(), vec!["send_back", "pass"]);
        assert!(list_gate_runs(&c, "left", "left-1-def", 10).unwrap().is_empty());
    }
}
