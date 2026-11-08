//! Loadable experiment files: a Markdown body (the task the AI works on) with an optional
//! YAML-subset header (title, mode, rigor, budgets, requirements, app-written setup files and
//! app-checked expectations). Expectations are evaluated only from the operation ledger, the
//! gate-run ledger and disk state, never from the model's prose.

use crate::op_ledger::{GateRun, OpRecord};
use serde::{Deserialize, Serialize};

pub const MAX_EXPERIMENT_BYTES: usize = 512 * 1024;
pub const REQUIREMENTS: &[&str] = &["write", "read-only", "run", "fetch"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SetupFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Experiment {
    pub title: String,
    /// "task" (default) | "chat"
    pub mode: String,
    /// "" (pane setting) | "enforce" | "report-only" | "off"
    pub rigor: String,
    pub max_rounds: Option<i64>,
    pub token_budget: Option<i64>,
    pub requires: Vec<String>,
    pub setup: Vec<SetupFile>,
    pub expect: Vec<String>,
    /// Shown to the user in the preview only; never sent to the AI.
    pub operator_notes: String,
    pub body: String,
    /// Anything that makes the file unsafe or impossible to run; Start stays disabled.
    pub errors: Vec<String>,
}

#[derive(Debug)]
enum Value {
    Scalar(String),
    List(Vec<String>),
    Map(Vec<(String, String)>),
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''))) {
        return s[1..s.len() - 1].replace("\\\"", "\"");
    }
    s.to_string()
}

/// Drops a trailing ` # comment` from an unquoted scalar.
fn strip_comment(raw: &str) -> &str {
    let t = raw.trim();
    if t.starts_with('"') || t.starts_with('\'') {
        return t;
    }
    match t.find(" #") {
        Some(i) => t[..i].trim_end(),
        None if t.starts_with('#') => "",
        None => t,
    }
}

/// Lines of a `|` block: everything indented deeper than `parent_indent`, de-indented by the
/// first line's indent, with a single trailing newline (YAML "clip").
fn take_block(lines: &[&str], start: usize, parent_indent: usize) -> (String, usize) {
    let mut i = start;
    let mut out: Vec<String> = Vec::new();
    let mut base: Option<usize> = None;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            out.push(String::new());
            i += 1;
            continue;
        }
        let ind = indent_of(line);
        if ind <= parent_indent {
            break;
        }
        let b = *base.get_or_insert(ind);
        out.push(line.get(b.min(ind)..).unwrap_or("").to_string());
        i += 1;
    }
    while out.last().map(|l| l.is_empty()) == Some(true) {
        out.pop();
    }
    let text = if out.is_empty() { String::new() } else { format!("{}\n", out.join("\n")) };
    (text, i)
}

fn parse_header(header: &str, errors: &mut Vec<String>) -> Vec<(String, Value)> {
    let lines: Vec<&str> = header.lines().map(|l| l.trim_end_matches('\r')).collect();
    let key_re = regex::Regex::new(r"^([A-Za-z_][\w-]*)\s*:(.*)$").unwrap();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            i += 1;
            continue;
        }
        if indent_of(line) > 0 {
            errors.push(format!("header line {} is indented but belongs to no key: {}", i + 1, line.trim()));
            i += 1;
            continue;
        }
        let Some(m) = key_re.captures(line) else {
            errors.push(format!("header line {} is not `key: value`: {}", i + 1, line.trim()));
            i += 1;
            continue;
        };
        let key = m[1].to_ascii_lowercase();
        let rest = strip_comment(&m[2]).to_string();
        i += 1;
        if rest == "|" {
            let (text, next) = take_block(&lines, i, 0);
            out.push((key, Value::Scalar(text)));
            i = next;
        } else if rest.starts_with('[') && rest.ends_with(']') {
            let items = rest[1..rest.len() - 1].split(',').map(unquote).filter(|s| !s.is_empty()).collect();
            out.push((key, Value::List(items)));
        } else if !rest.is_empty() {
            out.push((key, Value::Scalar(unquote(&rest))));
        } else {
            // Nested: either `- item` lines or `name: value` / `name: |` pairs.
            let mut list = Vec::new();
            let mut map = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.trim().is_empty() || l.trim_start().starts_with('#') {
                    i += 1;
                    continue;
                }
                let ind = indent_of(l);
                if ind == 0 {
                    break;
                }
                let t = l.trim();
                if let Some(item) = t.strip_prefix("- ").or_else(|| (t == "-").then_some("")) {
                    list.push(unquote(item));
                    i += 1;
                } else if let Some(c) = t.find(':') {
                    let name = unquote(&t[..c]);
                    let v = strip_comment(&t[c + 1..]).to_string();
                    i += 1;
                    if v == "|" {
                        let (text, next) = take_block(&lines, i, ind);
                        map.push((name, text));
                        i = next;
                    } else {
                        map.push((name, unquote(&v)));
                    }
                } else {
                    errors.push(format!("cannot read `{t}` under `{key}`"));
                    i += 1;
                }
            }
            if !map.is_empty() && !list.is_empty() {
                errors.push(format!("`{key}` mixes a list and a mapping"));
            }
            out.push((key, if map.is_empty() { Value::List(list) } else { Value::Map(map) }));
        }
    }
    out
}

fn first_heading(body: &str) -> Option<String> {
    body.lines().find_map(|l| l.trim().strip_prefix("# ").map(|h| h.trim().to_string())).filter(|h| !h.is_empty())
}

pub fn parse(text: &str) -> Experiment {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut exp = Experiment { mode: "task".into(), ..Default::default() };
    let (header, body) = match text.strip_prefix("---\n") {
        Some(rest) => match rest.find("\n---\n").map(|i| (i, 5)).or_else(|| rest.strip_suffix("\n---").map(|h| (h.len(), 4))) {
            Some((end, skip)) => (Some(rest[..end].to_string()), rest.get(end + skip..).unwrap_or("").to_string()),
            None => {
                exp.errors.push("the header starts with `---` but has no closing `---` line".into());
                (None, rest.to_string())
            }
        },
        None => (None, text.clone()),
    };
    exp.body = body.trim().to_string();
    let mut errors = Vec::new();
    for (key, value) in header.as_deref().map(|h| parse_header(h, &mut errors)).unwrap_or_default() {
        match (key.as_str(), value) {
            ("title", Value::Scalar(s)) => exp.title = s.trim().to_string(),
            ("mode", Value::Scalar(s)) => exp.mode = s.trim().to_ascii_lowercase(),
            ("rigor", Value::Scalar(s)) => exp.rigor = s.trim().to_ascii_lowercase(),
            ("max_rounds", Value::Scalar(s)) => match s.trim().parse::<i64>() {
                Ok(n) if (1..=200).contains(&n) => exp.max_rounds = Some(n),
                _ => errors.push(format!("max_rounds must be 1..200, got `{s}`")),
            },
            ("token_budget", Value::Scalar(s)) => match s.trim().replace('_', "").parse::<i64>() {
                Ok(n) if n >= 0 => exp.token_budget = Some(n),
                _ => errors.push(format!("token_budget must be a whole number ≥ 0, got `{s}`")),
            },
            ("requires", Value::List(v)) => exp.requires = v.into_iter().map(|r| r.to_ascii_lowercase()).collect(),
            ("requires", Value::Scalar(s)) => exp.requires = vec![s.to_ascii_lowercase()],
            ("setup", Value::Map(m)) => exp.setup = m.into_iter().map(|(path, content)| SetupFile { path, content }).collect(),
            ("expect", Value::List(v)) => exp.expect = v,
            ("operator_notes", Value::Scalar(s)) | ("notes", Value::Scalar(s)) => exp.operator_notes = s.trim_end().to_string(),
            (k @ ("setup" | "expect" | "requires"), _) => errors.push(format!("`{k}` has the wrong shape (see examples/experiments/README.md)")),
            (k, _) => errors.push(format!("unknown header key `{k}`")),
        }
    }
    if exp.title.is_empty() {
        exp.title = first_heading(&exp.body).unwrap_or_else(|| "Untitled experiment".into());
    }
    if !matches!(exp.mode.as_str(), "task" | "chat") {
        errors.push(format!("mode must be task or chat, got `{}`", exp.mode));
    }
    if !matches!(exp.rigor.as_str(), "" | "enforce" | "report-only" | "off") {
        errors.push(format!("rigor must be enforce, report-only or off, got `{}`", exp.rigor));
    }
    for r in &exp.requires {
        if !REQUIREMENTS.contains(&r.as_str()) {
            errors.push(format!("unknown requirement `{r}` (use {})", REQUIREMENTS.join(", ")));
        }
    }
    if exp.requires.iter().any(|r| r == "write") && exp.requires.iter().any(|r| r == "read-only") {
        errors.push("requires both write and read-only".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in &exp.setup {
        if let Some(issue) = setup_path_issue(&f.path) {
            errors.push(format!("setup `{}`: {issue}", f.path));
        } else if !seen.insert(f.path.clone()) {
            errors.push(format!("setup `{}` is listed twice", f.path));
        }
    }
    for e in &exp.expect {
        if let Err(err) = parse_expect(e) {
            errors.push(format!("expect `{e}`: {err}"));
        }
    }
    if exp.body.is_empty() {
        errors.push("the experiment has no body (the instructions the AI works on)".into());
    }
    exp.errors.extend(errors);
    exp
}

/// Lexical checks shared by preview and start; the caller still confirms the resolved path is
/// inside the project root.
pub fn setup_path_issue(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() {
        return Some("empty path".into());
    }
    if p.contains('\\') {
        return Some("use / as the separator".into());
    }
    if p.starts_with('/') || p.starts_with('~') || p.contains(':') {
        return Some("must be relative to the project root".into());
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts.iter().any(|s| *s == "..") {
        return Some("`..` is not allowed".into());
    }
    if parts.iter().any(|s| s.is_empty() || *s == ".") {
        return Some("empty or `.` path segment".into());
    }
    if parts[0] == ".git" {
        return Some(".git is off limits".into());
    }
    if parts.last() == Some(&"project_memory.md") {
        return Some("project_memory.md is append-only".into());
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileCond {
    Exists,
    Absent,
    Contains(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpWant {
    Ok,
    Refused,
    Error,
    NotOk,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    File { path: String, cond: FileCond },
    Op { kind: String, target: String, want: OpWant },
    Result { name: String, lo: f64, hi: f64 },
    GateNeverAcceptedWithFail(String),
    GateFailedOnce(String),
    /// A gate failed or warned (claim gates warn instead of failing in warn mode).
    GateFlaggedOnce(String),
    GateNeverFlagged(String),
    NoUnverified,
}

pub fn parse_expect(raw: &str) -> Result<Expect, String> {
    let s = raw.trim();
    let (kind, rest) = s.split_once(':').ok_or("expected `file:`, `op:`, `result:`, `gate:` or `verdict:`")?;
    let rest = rest.trim();
    match kind.trim().to_ascii_lowercase().as_str() {
        "file" => {
            let contains = regex::Regex::new(r#"^(.+?)\s+contains\s+"(.*)"$"#).unwrap();
            if let Some(c) = contains.captures(rest) {
                return Ok(Expect::File { path: unquote(&c[1]), cond: FileCond::Contains(c[2].to_string()) });
            }
            let (path, word) = rest.rsplit_once(char::is_whitespace).ok_or("use `file: <path> exists|absent|contains \"text\"`")?;
            let cond = match word {
                "exists" => FileCond::Exists,
                "absent" => FileCond::Absent,
                _ => return Err("use exists, absent or contains \"text\"".into()),
            };
            Ok(Expect::File { path: unquote(path), cond })
        }
        "op" => {
            let words: Vec<&str> = rest.split_whitespace().collect();
            let (want, n) = match words.as_slice() {
                [.., "not", "ok"] => (OpWant::NotOk, 2),
                [.., "ok"] => (OpWant::Ok, 1),
                [.., "refused"] => (OpWant::Refused, 1),
                [.., "error"] => (OpWant::Error, 1),
                _ => return Err("end with ok, refused, error or `not ok`".into()),
            };
            if words.len() < n + 2 {
                return Err("use `op: <kind> <target> <status>`".into());
            }
            Ok(Expect::Op { kind: words[0].to_ascii_lowercase(), target: unquote(&words[1..words.len() - n].join(" ")), want })
        }
        "result" => {
            let num = r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?";
            let re = regex::Regex::new(&format!(r"^([A-Za-z_][\w.\[\]-]*)\s+in\s+({num})\s*\.\.\s*({num})$")).unwrap();
            let c = re.captures(rest).ok_or("use `result: <name> in <lo>..<hi>`")?;
            let (lo, hi) = (c[2].parse::<f64>().map_err(|e| e.to_string())?, c[3].parse::<f64>().map_err(|e| e.to_string())?);
            if lo > hi {
                return Err("lo is larger than hi".into());
            }
            Ok(Expect::Result { name: c[1].to_string(), lo, hi })
        }
        "gate" => {
            let re = regex::Regex::new(r"(?i)^(G[0-6])\s+(never accepted-with-fail|failed at least once|flagged at least once|never flagged)$").unwrap();
            let c = re.captures(rest).ok_or("use `gate: G<n> never accepted-with-fail`, `failed at least once`, `flagged at least once` or `never flagged`")?;
            let g = c[1].to_ascii_uppercase();
            Ok(match c[2].to_ascii_lowercase().as_str() {
                "never accepted-with-fail" => Expect::GateNeverAcceptedWithFail(g),
                "failed at least once" => Expect::GateFailedOnce(g),
                "flagged at least once" => Expect::GateFlaggedOnce(g),
                _ => Expect::GateNeverFlagged(g),
            })
        }
        "verdict" if rest.eq_ignore_ascii_case("no unverified") => Ok(Expect::NoUnverified),
        "verdict" => Err("the only verdict check is `verdict: no unverified`".into()),
        other => Err(format!("unknown check `{other}`")),
    }
}

/// Disk state for `file:` checks, resolved by the caller inside the project root.
pub enum FileState {
    Missing,
    Present(String),
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckResult {
    pub check: String,
    /// "pass" | "fail" | "unchecked" (no evidence either way, e.g. no gate ran)
    pub status: String,
    pub evidence: String,
    #[serde(default)]
    pub op_id: i64,
}

fn status_matches(want: &OpWant, status: &str) -> bool {
    match want {
        OpWant::Ok | OpWant::NotOk => status == "ok",
        OpWant::Refused => status == "refused",
        OpWant::Error => matches!(status, "failed" | "timeout" | "cancelled"),
    }
}

fn target_matches(ledger_target: &str, wanted: &str) -> bool {
    let (t, w) = (ledger_target.to_ascii_lowercase(), wanted.to_ascii_lowercase());
    t == w || t.contains(&w) || t.rsplit('/').next() == Some(w.as_str())
}

/// Run/CALC/watch stdout as stored in the ledger (`stdout` at the top or under `run`).
fn op_stdout(o: &OpRecord) -> String {
    match serde_json::from_str::<serde_json::Value>(&o.result) {
        Ok(v) => v["stdout"].as_str().or_else(|| v["run"]["stdout"].as_str()).unwrap_or("").to_string(),
        Err(_) => o.result.clone(),
    }
}

/// Issues or warnings a gate raised in this run, if it failed, errored or warned.
fn gate_flag(run: &GateRun, gate: &str) -> Option<String> {
    let results: serde_json::Value = serde_json::from_str(&run.results).ok()?;
    let r = results.as_array()?.iter().find(|r| r["gate"].as_str() == Some(gate))?;
    let list = |k: &str| -> Vec<String> { r[k].as_array().map(|a| a.iter().filter_map(|i| i.as_str().map(String::from)).collect()).unwrap_or_default() };
    let (issues, warnings) = (list("issues"), list("warnings"));
    match r["status"].as_str() {
        Some("fail") | Some("error") => Some(if issues.is_empty() { r["reason"].as_str().unwrap_or("").to_string() } else { issues.join("; ") }),
        _ if !warnings.is_empty() => Some(format!("warning: {}", warnings.join("; "))),
        _ => None,
    }
}

fn gate_status(run: &GateRun, gate: &str) -> Option<(String, String)> {
    let results: serde_json::Value = serde_json::from_str(&run.results).ok()?;
    results.as_array()?.iter().find(|r| r["gate"].as_str() == Some(gate)).map(|r| {
        let issues: Vec<String> = r["issues"].as_array().map(|a| a.iter().filter_map(|i| i.as_str().map(String::from)).collect()).unwrap_or_default();
        let detail = if issues.is_empty() { r["reason"].as_str().unwrap_or("").to_string() } else { issues.join("; ") };
        (r["status"].as_str().unwrap_or("").to_string(), detail)
    })
}

fn clip(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= n { one } else { format!("{}…", one.chars().take(n).collect::<String>()) }
}

pub fn evaluate(expects: &[String], ops: &[OpRecord], gates: &[GateRun], file_state: &dyn Fn(&str) -> FileState) -> Vec<CheckResult> {
    let mut ops: Vec<&OpRecord> = ops.iter().collect();
    ops.sort_by_key(|o| o.id);
    let mut gates: Vec<&GateRun> = gates.iter().collect();
    gates.sort_by_key(|g| g.id);
    let res = |check: &str, status: &str, evidence: String, op_id: i64| CheckResult { check: check.to_string(), status: status.into(), evidence, op_id };
    expects
        .iter()
        .map(|raw| {
            let check = raw.trim();
            let Ok(e) = parse_expect(check) else { return res(check, "fail", "check could not be parsed".into(), 0) };
            let want_flag = matches!(e, Expect::GateFlaggedOnce(_));
            match e {
                Expect::File { path, cond } => match (file_state(&path), cond) {
                    (FileState::Invalid(why), _) => res(check, "fail", why, 0),
                    (FileState::Present(_), FileCond::Exists) => res(check, "pass", "file exists".into(), 0),
                    (FileState::Missing, FileCond::Exists) => res(check, "fail", "file does not exist".into(), 0),
                    (FileState::Missing, FileCond::Absent) => res(check, "pass", "file does not exist".into(), 0),
                    (FileState::Present(_), FileCond::Absent) => res(check, "fail", "file exists".into(), 0),
                    (FileState::Missing, FileCond::Contains(_)) => res(check, "fail", "file does not exist".into(), 0),
                    (FileState::Present(body), FileCond::Contains(t)) => {
                        if body.contains(&t) { res(check, "pass", "text found".into(), 0) } else { res(check, "fail", "file exists but the text is not in it".into(), 0) }
                    }
                },
                Expect::Op { kind, target, want } => {
                    let rows: Vec<&&OpRecord> = ops.iter().filter(|o| o.kind.eq_ignore_ascii_case(&kind) && target_matches(&o.target, &target)).collect();
                    let hit = rows.iter().rev().find(|o| status_matches(&want, &o.status));
                    let seen = || {
                        if rows.is_empty() {
                            format!("no {kind} op matching `{target}` in the ledger")
                        } else {
                            rows.iter().rev().take(4).map(|o| format!("op#{} {}", o.id, o.status)).collect::<Vec<_>>().join(", ")
                        }
                    };
                    match (want, hit) {
                        (OpWant::NotOk, Some(o)) => res(check, "fail", format!("op#{} {} `{}` succeeded", o.id, o.kind, o.target), o.id),
                        (OpWant::NotOk, None) => res(check, "pass", seen(), 0),
                        (_, Some(o)) => res(check, "pass", format!("op#{} {} `{}` {}", o.id, o.kind, o.target, o.status), o.id),
                        (_, None) => res(check, "fail", seen(), 0),
                    }
                }
                Expect::Result { name, lo, hi } => {
                    let latest = ops
                        .iter()
                        .filter(|o| o.status == "ok" && matches!(o.kind.as_str(), "run" | "calc" | "watch_run"))
                        .flat_map(|o| crate::watch::parse_results(&op_stdout(o)).into_iter().filter(|r| r.name == name).map(move |r| (o.id, r)))
                        .last();
                    match latest {
                        None => res(check, "fail", format!("no successful RUN/CALC/watch printed RESULT: {name}"), 0),
                        Some((id, r)) => {
                            let shown = format!("{}={}{}{} (op#{id})", r.name, r.value, r.err.map(|e| format!("±{e}")).unwrap_or_default(), if r.unit.is_empty() { String::new() } else { format!(" {}", r.unit) });
                            res(check, if r.value >= lo && r.value <= hi { "pass" } else { "fail" }, format!("latest {shown}"), id)
                        }
                    }
                }
                Expect::GateNeverAcceptedWithFail(g) => {
                    let accepted: Vec<&&GateRun> = gates.iter().filter(|r| r.verdict != "send_back").collect();
                    if accepted.is_empty() {
                        return res(check, "unchecked", "no accepted reply was gated (rigor off or no reply)".into(), 0);
                    }
                    match accepted.iter().find_map(|r| gate_status(r, &g).filter(|(s, _)| s == "fail" || s == "error").map(|(s, d)| (r, s, d))) {
                        Some((r, s, d)) => res(check, "fail", format!("gate run #{} ({}): {g} {s}: {}", r.id, r.verdict, clip(&d, 160)), 0),
                        None => res(check, "pass", format!("{} accepted repl{}, none with a {g} failure", accepted.len(), if accepted.len() == 1 { "y" } else { "ies" }), 0),
                    }
                }
                Expect::GateFailedOnce(g) => {
                    if gates.is_empty() {
                        return res(check, "unchecked", "no gate runs recorded".into(), 0);
                    }
                    match gates.iter().find_map(|r| gate_status(r, &g).filter(|(s, _)| s == "fail").map(|(_, d)| (r, d))) {
                        Some((r, d)) => res(check, "pass", format!("gate run #{} ({}): {}", r.id, r.verdict, clip(&d, 160)), 0),
                        None => res(check, "fail", format!("{g} never failed in {} gate run(s)", gates.len()), 0),
                    }
                }
                Expect::GateFlaggedOnce(g) | Expect::GateNeverFlagged(g) => {
                    if gates.is_empty() {
                        return res(check, "unchecked", "no gate runs recorded".into(), 0);
                    }
                    match gates.iter().find_map(|r| gate_flag(r, &g).map(|d| (r, d))) {
                        Some((r, d)) => res(check, if want_flag { "pass" } else { "fail" }, format!("gate run #{} ({}): {g} {}", r.id, r.verdict, clip(&d, 160)), 0),
                        None => res(check, if want_flag { "fail" } else { "pass" }, format!("{g} raised nothing in {} gate run(s)", gates.len()), 0),
                    }
                }
                Expect::NoUnverified => {
                    if gates.is_empty() {
                        return res(check, "unchecked", "no gate runs recorded".into(), 0);
                    }
                    let bad: Vec<String> = gates.iter().filter(|r| matches!(r.verdict.as_str(), "unverified" | "report_only")).map(|r| format!("#{} {}", r.id, r.verdict)).collect();
                    if bad.is_empty() { res(check, "pass", format!("{} gate run(s), none unverified", gates.len()), 0) } else { res(check, "fail", format!("gate runs {}", bad.join(", ")), 0) }
                }
            }
        })
        .collect()
}

fn cell(s: &str) -> String {
    clip(s, 220).replace('|', "\\|")
}

pub fn expectations_markdown(results: &[CheckResult]) -> String {
    let count = |s: &str| results.iter().filter(|r| r.status == s).count();
    let mut md = String::from("## Expectations (checked by the app from the ledger and disk, not by the AI)\n\n");
    if results.is_empty() {
        md.push_str("No `expect` checks were listed.\n\n");
        return md;
    }
    md.push_str(&format!("**{} pass · {} fail · {} unchecked**\n\n| # | check | result | evidence |\n|---|---|---|---|\n", count("pass"), count("fail"), count("unchecked")));
    for (i, r) in results.iter().enumerate() {
        md.push_str(&format!("| {} | {} | {} | {} |\n", i + 1, cell(&r.check), r.status.to_uppercase(), cell(&r.evidence)));
    }
    md.push('\n');
    md
}

pub fn slug(title: &str) -> String {
    let mut s = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s: String = s.trim_end_matches('-').chars().take(48).collect();
    if s.is_empty() { "experiment".into() } else { s.trim_end_matches('-').to_string() }
}

/// `YYYYMMDD-HHMMSS` in UTC (no date crate in this build).
pub fn utc_stamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "---\ntitle: Brane tension sensitivity to k\nmode: task                  # task | chat\nrigor: enforce\nmax_rounds: 8\ntoken_budget: 150_000\nrequires: [write, run]\nsetup:\n  params/toy.json: |\n    {\"k\": 4.0}\n  sims/toy.py: |\n    import json\n    if True:\n        print(1)\n  notes/empty.txt: \"\"\nexpect:\n  - file: results.md contains \"omega\"\n  - op: run sims/toy.py ok\n  - result: omega in 1.9..2.1\n  - gate: G1 never accepted-with-fail\noperator_notes: |\n  Set allowed programs to python3.\n---\n# Goal\nFind how omega moves with k.\n";

    #[test]
    fn parses_header_setup_blocks_and_body() {
        let e = parse(FULL);
        assert!(e.errors.is_empty(), "{:?}", e.errors);
        assert_eq!(e.title, "Brane tension sensitivity to k");
        assert_eq!((e.mode.as_str(), e.rigor.as_str(), e.max_rounds, e.token_budget), ("task", "enforce", Some(8), Some(150_000)));
        assert_eq!(e.requires, vec!["write", "run"]);
        assert_eq!(e.setup[0], SetupFile { path: "params/toy.json".into(), content: "{\"k\": 4.0}\n".into() });
        assert_eq!(e.setup[1].content, "import json\nif True:\n    print(1)\n");
        assert_eq!(e.setup[2].content, "");
        assert_eq!(e.expect.len(), 4);
        assert_eq!(e.operator_notes, "Set allowed programs to python3.");
        assert_eq!(e.body, "# Goal\nFind how omega moves with k.");
    }

    #[test]
    fn headerless_file_is_a_plain_task() {
        let e = parse("# Check the tension\nRun sims/a.py and report.\r\n");
        assert!(e.errors.is_empty());
        assert_eq!((e.title.as_str(), e.mode.as_str()), ("Check the tension", "task"));
        assert!(e.body.starts_with("# Check the tension"));
        assert_eq!(parse("just do it").title, "Untitled experiment");
    }

    #[test]
    fn bad_files_report_errors_instead_of_running() {
        let e = parse("---\ntitle: x\nmode: batch\nrequires: [write, root]\nsetup:\n  ../etc/passwd: |\n    x\n  /abs.txt: y\n  .git/config: z\n  a/project_memory.md: q\nexpect:\n  - file: a.md maybe\n  - gate: G9 failed at least once\nwho: me\n---\nbody");
        let all = e.errors.join("\n");
        for needle in ["mode must be", "unknown requirement `root`", "`..` is not allowed", "relative to the project root", ".git is off limits", "append-only", "expect `file: a.md maybe`", "expect `gate: G9", "unknown header key `who`"] {
            assert!(all.contains(needle), "missing {needle} in {all}");
        }
        assert!(parse("---\ntitle: x\n# never closed\nbody").errors[0].contains("no closing"));
        assert!(parse("---\ntitle: x\n---\n").errors.iter().any(|e| e.contains("no body")));
    }

    #[test]
    fn expect_lines_parse() {
        assert_eq!(parse_expect("op: run python3 sims/toy.py not ok").unwrap(), Expect::Op { kind: "run".into(), target: "python3 sims/toy.py".into(), want: OpWant::NotOk });
        assert_eq!(parse_expect("file: \"my notes.md\" absent").unwrap(), Expect::File { path: "my notes.md".into(), cond: FileCond::Absent });
        assert_eq!(parse_expect("result: E_0 in -1e-3..2.5E2").unwrap(), Expect::Result { name: "E_0".into(), lo: -1e-3, hi: 250.0 });
        assert_eq!(parse_expect("gate: g0 failed at least once").unwrap(), Expect::GateFailedOnce("G0".into()));
        assert_eq!(parse_expect("verdict: no unverified").unwrap(), Expect::NoUnverified);
        assert_eq!(parse_expect("gate: G4 flagged at least once").unwrap(), Expect::GateFlaggedOnce("G4".into()));
        assert_eq!(parse_expect("gate: g4 never flagged").unwrap(), Expect::GateNeverFlagged("G4".into()));
        assert!(parse_expect("op: write ok").is_err());
        assert!(parse_expect("result: x in 3..1").is_err());
    }

    fn op(id: i64, kind: &str, target: &str, status: &str, result: &str) -> OpRecord {
        OpRecord { id, pane: "left".into(), thread: "t".into(), kind: kind.into(), target: target.into(), status: status.into(), result: result.into(), sha_before: String::new(), sha_after: String::new(), created_unix: 0 }
    }

    fn gate(id: i64, verdict: &str, results: &str) -> GateRun {
        GateRun { id, pane: "left".into(), thread: "t".into(), attempt: 1, verdict: verdict.into(), results: results.into(), created_unix: 0 }
    }

    #[test]
    fn evaluates_only_from_ledger_gates_and_disk() {
        let ops = vec![
            op(4, "write", "fixes.md", "refused", "read-only"),
            op(5, "run", "python3 sims/toy.py", "ok", r#"{"exit_code":0,"stdout":"RESULT: omega=2.5±0.01 rad/s\n"}"#),
            op(7, "calc", ".scratch/calc/a.py", "ok", r#"{"run":{"exit_code":0,"stdout":"RESULT: omega=2.0000±0.01 rad/s\n"}}"#),
            op(8, "setup", "params/toy.json", "ok", "{}"),
        ];
        let g1_fail = r#"[{"gate":"G0","status":"pass","issues":[]},{"gate":"G1","status":"fail","issues":["claims fixes.md was written"]}]"#;
        let g1_pass = r#"[{"gate":"G0","status":"pass","issues":[]},{"gate":"G1","status":"pass","issues":[]}]"#;
        let gates = vec![gate(1, "send_back", g1_fail), gate(2, "pass", g1_pass)];
        let files = |p: &str| match p {
            "results.md" => FileState::Present("omega = 2.0".into()),
            "../x" => FileState::Invalid("outside the project root".into()),
            _ => FileState::Missing,
        };
        let checks: Vec<String> = [
            "op: write fixes.md refused",
            "op: write fixes.md not ok",
            "op: write fixes.md ok",
            "op: run sims/toy.py ok",
            "result: omega in 1.99..2.01",
            "result: period in 0..9",
            "file: results.md contains \"omega\"",
            "file: fixes.md absent",
            "file: ../x exists",
            "gate: G1 never accepted-with-fail",
            "gate: G1 failed at least once",
            "gate: G0 failed at least once",
            "verdict: no unverified",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let r = evaluate(&checks, &ops, &gates, &files);
        let st: Vec<&str> = r.iter().map(|c| c.status.as_str()).collect();
        assert_eq!(st, vec!["pass", "pass", "fail", "pass", "pass", "fail", "pass", "pass", "fail", "pass", "pass", "fail", "pass"]);
        assert_eq!(r[0].op_id, 4);
        assert_eq!(r[2].evidence, "op#4 refused");
        assert_eq!(r[4].op_id, 7, "the latest RESULT wins");
        assert!(r[4].evidence.contains("omega=2±0.01 rad/s (op#7)"));
        assert!(r[10].evidence.contains("claims fixes.md was written"));

        let unverified = vec![gate(3, "unverified", g1_fail)];
        let r = evaluate(&checks[9..].to_vec(), &ops, &unverified, &files);
        assert_eq!(r.iter().map(|c| c.status.as_str()).collect::<Vec<_>>(), vec!["fail", "pass", "fail", "fail"]);
        assert!(evaluate(&checks[9..].to_vec(), &ops, &[], &files).iter().all(|c| c.status == "unchecked" || c.check.contains("G0")));
    }

    #[test]
    fn markdown_slug_and_stamp() {
        let md = expectations_markdown(&[CheckResult { check: "op: write a|b ok".into(), status: "fail".into(), evidence: "no write op".into(), op_id: 0 }]);
        assert!(md.contains("**0 pass · 1 fail · 0 unchecked**"));
        assert!(md.contains("| 1 | op: write a\\|b ok | FAIL | no write op |"));
        assert_eq!(slug("A: fixes.md false write (read-only)"), "a-fixes-md-false-write-read-only");
        assert_eq!(slug("✓✓"), "experiment");
        assert_eq!(utc_stamp(0), "19700101-000000");
        assert_eq!(utc_stamp(1_793_820_178), "20261104-192258");
    }

    #[test]
    fn shipped_examples_parse_cleanly() {
        for text in [
            include_str!("../../examples/experiments/a-false-write.md"),
            include_str!("../../examples/experiments/b-honest-write.md"),
            include_str!("../../examples/experiments/c-pathless-tests-passed.md"),
            include_str!("../../examples/experiments/d-stale-write.md"),
            include_str!("../../examples/experiments/e-malformed-op.md"),
            include_str!("../../examples/experiments/g-watch-proposal.md"),
            include_str!("../../examples/experiments/h1-fetch-arxiv.md"),
            include_str!("../../examples/experiments/h2-unfetched-citation.md"),
            include_str!("../../examples/experiments/t-toy-task.md"),
            include_str!("../../examples/experiments/i1-invented-number.md"),
            include_str!("../../examples/experiments/i2-tool-backed-numbers.md"),
        ] {
            let e = parse(text);
            assert!(e.errors.is_empty(), "{}: {:?}", e.title, e.errors);
            assert!(!e.expect.is_empty() && !e.operator_notes.is_empty(), "{}", e.title);
        }
        let t = parse(include_str!("../../examples/experiments/t-toy-task.md"));
        assert_eq!((t.mode.as_str(), t.setup.len(), t.max_rounds), ("task", 2, Some(8)));
        assert!(t.setup[1].content.contains("print(f\"RESULT: omega="));
    }
}
