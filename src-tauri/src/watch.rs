//! Watch-mode experiment loops: a user-armed watch reruns a project script when its inputs
//! change (or on a schedule), compares its `RESULT: name=value±err unit` lines with a saved
//! baseline, and reports only when a prediction moves. No fs watcher: the UI ticks while a
//! watch is armed and inputs are re-hashed only when their mtime/size changed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// A value "moved" when |new - old| exceeds this many combined standard errors.
pub const MOVED_SIGMA: f64 = 2.0;
pub const MIN_EVERY_SECONDS: i64 = 30;
pub const MAX_EVERY_SECONDS: i64 = 7 * 24 * 3600;
pub const MAX_INPUT_FILES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultLine {
    pub name: String,
    pub value: f64,
    pub err: Option<f64>,
    pub unit: String,
}

/// Parses `RESULT: name=value±err unit` (also `+/-`, or no error). The last line per name wins.
pub fn parse_results(stdout: &str) -> Vec<ResultLine> {
    let num = r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?";
    let re = regex::Regex::new(&format!(
        r"(?m)^[ \t]*RESULT:[ \t]*`?([A-Za-z_][\w.\[\]-]*)`?[ \t]*=[ \t]*({num})[ \t]*(?:(?:±|\+/-|\+-)[ \t]*({num}))?[ \t]*([^\r\n]*?)[ \t]*\r?$"
    ))
    .expect("valid RESULT regex");
    let mut by_name: BTreeMap<String, ResultLine> = BTreeMap::new();
    for c in re.captures_iter(stdout) {
        let Ok(value) = c[2].parse::<f64>() else { continue };
        let err = c.get(3).and_then(|m| m.as_str().parse::<f64>().ok()).map(f64::abs);
        let name = c[1].to_string();
        by_name.insert(name.clone(), ResultLine { name, value, err, unit: c[4].trim().to_string() });
    }
    by_name.into_values().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub name: String,
    /// "steady" | "moved" | "missing" | "new" | "unit_changed"
    pub kind: String,
    pub old: Option<ResultLine>,
    pub new: Option<ResultLine>,
    pub z: Option<f64>,
    #[serde(default)]
    pub note: String,
}

pub fn compare(baseline: &[ResultLine], current: &[ResultLine], sigma: f64) -> Vec<Change> {
    let mut out = Vec::new();
    for old in baseline {
        let Some(new) = current.iter().find(|r| r.name == old.name) else {
            out.push(Change { name: old.name.clone(), kind: "missing".into(), old: Some(old.clone()), new: None, z: None, note: "no longer printed".into() });
            continue;
        };
        if new.unit != old.unit {
            out.push(Change { name: old.name.clone(), kind: "unit_changed".into(), old: Some(old.clone()), new: Some(new.clone()), z: None, note: format!("unit '{}' -> '{}'", old.unit, new.unit) });
            continue;
        }
        let delta = (new.value - old.value).abs();
        let combined = (old.err.unwrap_or(0.0).powi(2) + new.err.unwrap_or(0.0).powi(2)).sqrt();
        let (kind, z, note) = if combined > 0.0 {
            let z = delta / combined;
            (if z > sigma { "moved" } else { "steady" }, Some(z), String::new())
        } else {
            let tol = 1e-12 * old.value.abs().max(new.value.abs()).max(f64::MIN_POSITIVE);
            (if delta > tol { "moved" } else { "steady" }, None, "no error stated; any change counts".to_string())
        };
        out.push(Change { name: old.name.clone(), kind: kind.into(), old: Some(old.clone()), new: Some(new.clone()), z, note });
    }
    for new in current.iter().filter(|r| !baseline.iter().any(|b| b.name == r.name)) {
        out.push(Change { name: new.name.clone(), kind: "new".into(), old: None, new: Some(new.clone()), z: None, note: "not in the baseline".into() });
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct FileStamp {
    pub mtime: i64,
    pub size: u64,
    pub sha: String,
}

pub type Fingerprint = BTreeMap<String, FileStamp>;

fn safe_relative(pattern: &str) -> Result<(), String> {
    let p = Path::new(pattern);
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return Err(format!("watch input '{pattern}' must be a project-relative path without '..'"));
    }
    Ok(())
}

fn wildcard_match(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut pi, mut ni, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Comma-separated project-relative paths; `*`/`?` wildcards in the last component; a directory
/// means the files directly inside it. Literal paths are kept even if missing so their creation counts.
pub fn expand_inputs(root: &Path, spec: &str) -> Result<Vec<String>, String> {
    let mut out = std::collections::BTreeSet::new();
    for raw in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let pattern = raw.replace('\\', "/");
        safe_relative(&pattern)?;
        let (dir, leaf) = match pattern.rsplit_once('/') {
            Some((d, l)) => (d.to_string(), l.to_string()),
            None => (String::new(), pattern.clone()),
        };
        if dir.contains('*') || dir.contains('?') {
            return Err(format!("watch input '{raw}': wildcards are only allowed in the file name"));
        }
        let join = |name: &str| if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
        let base = root.join(&dir);
        if leaf.contains('*') || leaf.contains('?') {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if e.path().is_file() && wildcard_match(&leaf, &name) {
                        out.insert(join(&name));
                    }
                }
            }
        } else if root.join(&pattern).is_dir() {
            if let Ok(entries) = std::fs::read_dir(root.join(&pattern)) {
                for e in entries.flatten() {
                    if e.path().is_file() {
                        out.insert(format!("{}/{}", pattern.trim_end_matches('/'), e.file_name().to_string_lossy()));
                    }
                }
            }
        } else {
            out.insert(pattern.clone());
        }
        if out.len() > MAX_INPUT_FILES {
            return Err(format!("watch inputs match more than {MAX_INPUT_FILES} files; narrow the pattern"));
        }
    }
    Ok(out.into_iter().collect())
}

/// Re-stamps `files`, hashing only those whose mtime or size differs from `prev`. A file modified
/// within the last 2 s is always re-hashed: coarse mtimes can hide a same-size rewrite.
pub fn refresh_fingerprint(root: &Path, files: &[String], prev: &Fingerprint) -> Fingerprint {
    const RACY_NANOS: i64 = 2_000_000_000;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0);
    let mut next = Fingerprint::new();
    for rel in files {
        let path = root.join(rel);
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let size = meta.len();
        let sha = match prev.get(rel) {
            Some(p) if p.mtime == mtime && p.size == size && now - mtime >= RACY_NANOS => p.sha.clone(),
            _ => crate::op_ledger::sha256_file(&path),
        };
        next.insert(rel.clone(), FileStamp { mtime, size, sha });
    }
    next
}

/// Inputs whose content changed, appeared, or disappeared (a touch with the same content is not a change).
pub fn changed_inputs(prev: &Fingerprint, next: &Fingerprint) -> Vec<String> {
    let mut out: Vec<String> = next.iter().filter(|(k, v)| prev.get(*k).map(|p| p.sha != v.sha).unwrap_or(true)).map(|(k, _)| k.clone()).collect();
    out.extend(prev.keys().filter(|k| !next.contains_key(*k)).cloned());
    out.sort();
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Watch {
    #[serde(default)]
    pub id: i64,
    pub pane: String,
    pub thread: String,
    pub cmd: String,
    pub inputs: String,
    /// 0 = rerun on input change only.
    pub every_seconds: i64,
    /// "proposed" | "armed" | "paused"
    pub status: String,
    pub baseline: Vec<ResultLine>,
    pub fingerprint: Fingerprint,
    pub dirty: bool,
    pub changed: Vec<String>,
    pub last_run_unix: i64,
    pub last_op_id: i64,
    pub created_unix: i64,
}

pub fn clamp_every(every: i64) -> i64 {
    if every <= 0 {
        0
    } else {
        every.clamp(MIN_EVERY_SECONDS, MAX_EVERY_SECONDS)
    }
}

/// Armed and (inputs changed, never run since arming/rebaseline, or the schedule elapsed).
pub fn is_due(w: &Watch, now: i64) -> bool {
    w.status == "armed" && (w.dirty || w.last_run_unix == 0 || (w.every_seconds > 0 && now - w.last_run_unix >= w.every_seconds))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunReport {
    pub watch_id: i64,
    /// "baseline" | "steady" | "moved" | "failed"
    pub status: String,
    pub reportable: bool,
    pub changes: Vec<Change>,
    pub changed_inputs: Vec<String>,
    pub op_id: i64,
    pub sigma: f64,
    pub note: String,
}

/// Folds one run into the watch: sets the baseline on the first good run, otherwise compares.
/// A failed run never touches the baseline and is always reported.
pub fn apply_run(w: &mut Watch, succeeded: bool, stdout: &str, failure: &str, now: i64, op_id: i64) -> RunReport {
    let changed_inputs = std::mem::take(&mut w.changed);
    w.dirty = false;
    w.last_run_unix = now;
    w.last_op_id = op_id;
    let mut report = RunReport { watch_id: w.id, status: String::new(), reportable: false, changes: Vec::new(), changed_inputs, op_id, sigma: MOVED_SIGMA, note: String::new() };
    if !succeeded {
        report.status = "failed".into();
        report.reportable = true;
        report.note = failure.chars().take(400).collect();
        return report;
    }
    let results = parse_results(stdout);
    if w.baseline.is_empty() {
        report.status = "baseline".into();
        if results.is_empty() {
            report.reportable = true;
            report.note = "the script printed no RESULT: name=value±err unit lines, so there is nothing to watch".into();
        } else {
            report.note = format!("baseline set from {} RESULT line(s)", results.len());
            w.baseline = results;
        }
        return report;
    }
    report.changes = compare(&w.baseline, &results, MOVED_SIGMA);
    report.reportable = report.changes.iter().any(|c| c.kind != "steady");
    report.status = if report.reportable { "moved" } else { "steady" }.into();
    report
}

pub fn init(c: &rusqlite::Connection) -> Result<(), String> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS watches (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             body TEXT NOT NULL
         );",
    )
    .map_err(|e| format!("watch table init failed: {e}"))
}

pub fn insert(c: &rusqlite::Connection, w: &Watch) -> Result<i64, String> {
    c.execute("INSERT INTO watches (body) VALUES ('{}')", []).map_err(|e| e.to_string())?;
    let id = c.last_insert_rowid();
    let mut stored = w.clone();
    stored.id = id;
    update(c, &stored)?;
    Ok(id)
}

pub fn update(c: &rusqlite::Connection, w: &Watch) -> Result<(), String> {
    let body = serde_json::to_string(w).map_err(|e| e.to_string())?;
    c.execute("UPDATE watches SET body = ?1 WHERE id = ?2", rusqlite::params![body, w.id]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get(c: &rusqlite::Connection, id: i64) -> Result<Watch, String> {
    let body: String = c
        .query_row("SELECT body FROM watches WHERE id = ?1", [id], |r| r.get(0))
        .map_err(|_| format!("watch #{id} not found"))?;
    serde_json::from_str(&body).map_err(|e| format!("watch #{id} unreadable: {e}"))
}

pub fn list(c: &rusqlite::Connection) -> Result<Vec<Watch>, String> {
    let mut stmt = c.prepare("SELECT body FROM watches ORDER BY id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
    Ok(rows.flatten().filter_map(|b| serde_json::from_str(&b).ok()).collect())
}

pub fn delete(c: &rusqlite::Connection, id: i64) -> Result<(), String> {
    c.execute("DELETE FROM watches WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// App start: nothing reruns unattended until the user re-arms it.
pub fn pause_all_armed(c: &rusqlite::Connection) -> Result<usize, String> {
    let mut n = 0;
    for mut w in list(c)? {
        if w.status == "armed" {
            w.status = "paused".into();
            update(c, &w)?;
            n += 1;
        }
    }
    Ok(n)
}

/// One UI tick: re-stamps inputs of armed watches, marks content changes dirty, returns due ids.
pub fn tick(c: &rusqlite::Connection, root: &Path, now: i64) -> Result<Vec<i64>, String> {
    let mut due = Vec::new();
    for mut w in list(c)? {
        if w.status != "armed" {
            continue;
        }
        if let Ok(files) = expand_inputs(root, &w.inputs) {
            let next = refresh_fingerprint(root, &files, &w.fingerprint);
            let changed = changed_inputs(&w.fingerprint, &next);
            if !changed.is_empty() || next != w.fingerprint {
                if !changed.is_empty() {
                    w.dirty = true;
                    for f in changed {
                        if !w.changed.contains(&f) {
                            w.changed.push(f);
                        }
                    }
                }
                w.fingerprint = next;
                update(c, &w)?;
            }
        }
        if is_due(&w, now) {
            due.push(w.id);
        }
    }
    Ok(due)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, value: f64, err: Option<f64>, unit: &str) -> ResultLine {
        ResultLine { name: name.into(), value, err, unit: unit.into() }
    }

    #[test]
    fn parses_result_lines_with_and_without_errors() {
        let out = "warmup\nRESULT: sigma_T=1.23±0.05 GeV\nRESULT: `w0` = -0.98 +/- 2e-2\nRESULT: n_s=0.965\nRESULT: sigma_T=1.25±0.05 GeV\nnot RESULT: x=1\n";
        let r = parse_results(out);
        assert_eq!(r, vec![line("n_s", 0.965, None, ""), line("sigma_T", 1.25, Some(0.05), "GeV"), line("w0", -0.98, Some(0.02), "")]);
    }

    #[test]
    fn two_sigma_rule_classifies_moves() {
        let base = vec![line("a", 1.0, Some(0.1), "m"), line("b", 1.0, Some(0.1), "m"), line("c", 2.0, None, ""), line("gone", 1.0, None, "")];
        let now = vec![line("a", 1.2, Some(0.1), "m"), line("b", 1.4, Some(0.1), "m"), line("c", 2.0, None, ""), line("fresh", 3.0, None, "")];
        let ch = compare(&base, &now, MOVED_SIGMA);
        let kinds: Vec<(&str, &str)> = ch.iter().map(|c| (c.name.as_str(), c.kind.as_str())).collect();
        assert_eq!(kinds, vec![("a", "steady"), ("b", "moved"), ("c", "steady"), ("gone", "missing"), ("fresh", "new")]);
        assert!((ch[0].z.unwrap() - 0.2 / 0.02f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn errorless_values_move_on_any_change_and_units_are_checked() {
        let ch = compare(&[line("x", 1.0, None, "s")], &[line("x", 1.0000001, None, "s")], MOVED_SIGMA);
        assert_eq!(ch[0].kind, "moved");
        assert!(ch[0].note.contains("no error"));
        let ch = compare(&[line("x", 1.0, Some(0.1), "s")], &[line("x", 1.0, Some(0.1), "ms")], MOVED_SIGMA);
        assert_eq!(ch[0].kind, "unit_changed");
    }

    #[test]
    fn first_good_run_sets_baseline_failures_never_do() {
        let mut w = Watch { id: 1, status: "armed".into(), changed: vec!["p.yaml".into()], dirty: true, ..Default::default() };
        let r = apply_run(&mut w, false, "", "exit 1", 100, 7);
        assert_eq!((r.status.as_str(), r.reportable), ("failed", true));
        assert!(w.baseline.is_empty());
        assert_eq!(r.changed_inputs, vec!["p.yaml".to_string()]);
        assert!(!w.dirty && w.changed.is_empty());
        let r = apply_run(&mut w, true, "RESULT: a=1±0.1\n", "", 200, 8);
        assert_eq!((r.status.as_str(), r.reportable), ("baseline", false));
        assert_eq!(w.baseline.len(), 1);
        let r = apply_run(&mut w, true, "RESULT: a=1.05±0.1\n", "", 300, 9);
        assert_eq!((r.status.as_str(), r.reportable), ("steady", false));
        let r = apply_run(&mut w, true, "RESULT: a=2±0.1\n", "", 400, 10);
        assert_eq!((r.status.as_str(), r.reportable, r.op_id), ("moved", true, 10));
        let mut empty = Watch { id: 2, status: "armed".into(), ..Default::default() };
        assert!(apply_run(&mut empty, true, "no results\n", "", 1, 1).reportable);
    }

    #[test]
    fn due_rules() {
        let mut w = Watch { status: "proposed".into(), ..Default::default() };
        assert!(!is_due(&w, 10), "a proposed watch never runs");
        w.status = "armed".into();
        assert!(is_due(&w, 10), "first run after arming builds the baseline");
        w.last_run_unix = 10;
        assert!(!is_due(&w, 20));
        w.every_seconds = 60;
        assert!(is_due(&w, 70));
        w.every_seconds = 0;
        w.dirty = true;
        assert!(is_due(&w, 11));
        w.status = "paused".into();
        assert!(!is_due(&w, 11));
        assert_eq!((clamp_every(0), clamp_every(5), clamp_every(600)), (0, MIN_EVERY_SECONDS, 600));
    }

    #[test]
    fn inputs_expand_safely_and_only_content_changes_count() {
        let root = std::env::temp_dir().join(format!("watch_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::write(root.join("params.yaml"), "a: 1\n").unwrap();
        std::fs::write(root.join("data/x.csv"), "1\n").unwrap();
        std::fs::write(root.join("data/y.csv"), "2\n").unwrap();
        std::fs::write(root.join("data/notes.txt"), "n\n").unwrap();
        assert!(expand_inputs(&root, "../etc/passwd").is_err());
        assert!(expand_inputs(&root, "/etc/passwd").is_err());
        assert!(expand_inputs(&root, "*/x.csv").is_err());
        let files = expand_inputs(&root, "params.yaml, data/*.csv, later.txt").unwrap();
        assert_eq!(files, vec!["data/x.csv", "data/y.csv", "later.txt", "params.yaml"]);
        let fp1 = refresh_fingerprint(&root, &files, &Fingerprint::new());
        assert_eq!(fp1.len(), 3);
        std::fs::write(root.join("params.yaml"), "a: 1\n").unwrap();
        let fp2 = refresh_fingerprint(&root, &files, &fp1);
        assert!(changed_inputs(&fp1, &fp2).is_empty(), "rewriting identical content is not a change");
        std::fs::write(root.join("data/y.csv"), "3\n").unwrap();
        std::fs::write(root.join("later.txt"), "new\n").unwrap();
        let fp3 = refresh_fingerprint(&root, &files, &fp2);
        assert_eq!(changed_inputs(&fp2, &fp3), vec!["data/y.csv", "later.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn store_roundtrip_tick_and_pause_on_start() {
        let root = std::env::temp_dir().join(format!("watch_store_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("p.txt"), "1").unwrap();
        let c = rusqlite::Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        let mut w = Watch { pane: "left".into(), thread: "t".into(), cmd: "python3 s.py".into(), inputs: "p.txt".into(), status: "proposed".into(), ..Default::default() };
        w.id = insert(&c, &w).unwrap();
        assert!(tick(&c, &root, 5).unwrap().is_empty(), "proposed watches are not ticked");
        w.status = "armed".into();
        w.fingerprint = refresh_fingerprint(&root, &expand_inputs(&root, &w.inputs).unwrap(), &Fingerprint::new());
        w.last_run_unix = 5;
        update(&c, &w).unwrap();
        assert!(tick(&c, &root, 6).unwrap().is_empty());
        std::fs::write(root.join("p.txt"), "2").unwrap();
        assert_eq!(tick(&c, &root, 7).unwrap(), vec![w.id]);
        assert_eq!(get(&c, w.id).unwrap().changed, vec!["p.txt".to_string()]);
        assert_eq!(pause_all_armed(&c).unwrap(), 1);
        assert_eq!(get(&c, w.id).unwrap().status, "paused");
        delete(&c, w.id).unwrap();
        assert!(list(&c).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
