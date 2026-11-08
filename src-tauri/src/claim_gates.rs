//! Claim gates (G2–G5): deterministic checks that a reply's numbers and certainty have a basis in
//! this thread's ledger and inputs. They test traceability, never whether the physics is right.

use crate::op_ledger::OpRecord;

const NUM: &str = r"[-−]?\d+(?:\.\d+)?(?:[eE][-+]?\d+)?";
/// Units are case-sensitive and listed longest first, so `ms` is not read as `m`.
const UNITS: &str = r"(?:rad/s|m/s|keV|MeV|GeV|TeV|meV|eV|kHz|MHz|GHz|Hz|GPa|Pa|kpc|Mpc|pc|AMU|amu|nm|μm|um|mm|cm|km|fm|ns|μs|us|ms|kg|rad|AU|ly|sigma|m|s|g|J|K|W|N|T|V|A)\b|%|σ";
/// Counterfactual, hedged or explicitly unchecked numbers are not reported results.
const HEDGE: &str = r"(?i)\b(?:estimates?|estimated|roughly|order of|from memory|unverified|not (?:checked|verified|computed)|tentative(?:ly)?|hypothetical(?:ly)?|suppose|assum(?:e|es|ed|ing)|if|would|could|might|e\.g\.|for example|for instance|typical(?:ly)?)\b";
/// Op bodies are content (a script, a file, a calculation), not reported results.
const BLOCKS: &[&str] = &["FILE", "APPEND", "EDIT", "CALC", "PLAN", "REMEMBER"];
const SOURCE_KINDS: &[&str] = &["run", "calc", "watch_run", "read", "fetch"];

fn strip_non_prose(reply: &str) -> String {
    let mut text = reply.to_string();
    let blank = |t: &str, re: &regex::Regex| re.replace_all(t, |c: &regex::Captures| " ".repeat(c[0].len())).into_owned();
    text = blank(&text, &regex::Regex::new(r"(?s)```.*?(?:```|\z)").unwrap());
    for b in BLOCKS {
        text = blank(&text, &regex::Regex::new(&format!(r"(?s)<<<\s*{b}\b.*?(?:<<<\s*END {b}\s*>>>+|\z)")).unwrap());
    }
    blank(&text, &regex::Regex::new(r"<<<[^\n]*?>>>+").unwrap())
}

fn parse_num(token: &str) -> Option<(f64, f64)> {
    let t = token.replace('−', "-");
    let value: f64 = t.parse().ok()?;
    let (mantissa, exp) = match t.find(['e', 'E']) {
        Some(i) => (&t[..i], t[i + 1..].parse::<i32>().ok()?),
        None => (t.as_str(), 0),
    };
    let decimals = mantissa.split_once('.').map(|(_, d)| d.len() as i32).unwrap_or(0);
    Some((value, 0.5 * 10f64.powi(exp - decimals)))
}

fn source_numbers(ops: &[OpRecord], inputs: &str) -> Vec<f64> {
    let num_re = regex::Regex::new(NUM).unwrap();
    let mut texts: Vec<&str> = ops.iter().filter(|o| o.status == "ok" && SOURCE_KINDS.contains(&o.kind.as_str())).map(|o| o.result.as_str()).collect();
    texts.push(inputs);
    texts.iter().flat_map(|t| num_re.find_iter(t).filter_map(|m| m.as_str().replace('−', "-").parse::<f64>().ok())).collect()
}

/// G4: numbers the reply reports as results (`name = v`, `v ± e`, `v <unit>`) must appear, to their
/// stated precision, in a successful RUN/CALC/READ/FETCH/watch output or in the thread's inputs.
pub fn untooled_numbers(reply: &str, ops: &[OpRecord], inputs: &str) -> (Vec<String>, usize) {
    let prose = strip_non_prose(reply);
    let candidate_re = regex::Regex::new(&format!(r"(?:=|≈|≃)\s*({NUM})|({NUM})\s*(?:±|\+/-|\+-)|({NUM})\s?(?:×\s?10\^?[-+]?\d+\s?)?(?:{UNITS})")).unwrap();
    let hedge_re = regex::Regex::new(HEDGE).unwrap();
    let sentence_re = regex::Regex::new(r"[.!?]\s+|\n+").unwrap();
    let sources = source_numbers(ops, inputs);
    let mut issues = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut checked = 0;
    for sentence in sentence_re.split(&prose) {
        if hedge_re.is_match(sentence) {
            continue;
        }
        for cap in candidate_re.captures_iter(sentence) {
            let m = cap.get(1).or(cap.get(2)).or(cap.get(3)).unwrap();
            let before = sentence[..m.start()].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || "_./#:".contains(c)) {
                continue;
            }
            let token = m.as_str();
            let Some((value, tol)) = parse_num(token) else { continue };
            let year_like = !token.contains(['.', 'e', 'E']) && (1900.0..=2100.0).contains(&value) && cap.get(2).is_none();
            if year_like || !seen.insert(token.to_string()) {
                continue;
            }
            checked += 1;
            if sources.iter().any(|s| (s - value).abs() <= tol * (1.0 + 1e-9) || (s + value).abs() <= tol * (1.0 + 1e-9)) {
                continue;
            }
            let quote: String = sentence.trim().chars().take(100).collect();
            issues.push(format!("\"{quote}\" reports {token}, but no RUN/CALC/READ/FETCH output or input in this thread contains it; compute it with a CALC or RUN, or mark it as an estimate or tentative."));
        }
    }
    (issues, checked)
}

/// G2 triggers: sentences that assert a conclusion strongly enough to need a stated basis.
const CONCLUSION: &str = r"(?i)\b(?:prove[sn]?|proved|confirm(?:s|ed)?|demonstrat(?:es|ed)|establish(?:es|ed)|shows? that|showed that|we find|i find|found that|rules? out|ruled out|this means|is (?:correct|incorrect|wrong|valid|invalid))\b";
/// G5: wording that claims more than a source or a tentative status can carry.
const PROOF_WORDS: &str = r"(?i)\b(?:prove[sn]?|proved|confirm(?:s|ed)?|demonstrat(?:es|ed)|establish(?:es|ed)|conclusively|definitively|certainly|beyond doubt)\b";
const NEGATED: &str = r"(?i)\b(?:not|never|cannot|can't|no|doesn't|does not|didn't|did not|isn't|is not|unable to|without|nor)\b[^.!?\n]{0,30}$";
const DERIVED: &str = r"(?i)\b(?:assum(?:e|es|ed|ing|ption)|given|if|provided|follows from|from (?:the )?(?:equation|eq\.|formula|relation|definition)|by (?:definition|construction)|deriv(?:e|ed|ation)|substitut(?:e|ing)|therefore|thus|hence)\b";
const TENTATIVE: &str = r"(?i)\b(?:may|might|could|possibly|perhaps|likely|suggests?|appears?|seems?|plausibl[ey]|tentative(?:ly)?|hypothes[ie]s|speculat\w*)\b";
const UNRESOLVED: &str = r"(?i)\b(?:underdetermined|unresolved|undetermined|cannot (?:be )?(?:determined|distinguished|told)|not enough|insufficient|unknown|not checked|unchecked|unverified|from memory|unclear)\b";
const TOOL_REF: &str = r"(?i)\b(?:run|output|calculation|calc|simulation|results?|watch|script)\b";
const SOURCE_REF: &str = r"(?i)\b(?:paper|abstract|article|page|source|according to)\b|arxiv";

/// A claim's status from the vocabulary in AI-RIGOR-STRATEGY.md, as far as the sentence shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    VerifiedOperation,
    SourceBacked,
    DerivedConditionally,
    Tentative,
    Unresolved,
    NoBasis,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Claim {
    pub quote: String,
    pub status: ClaimStatus,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ClaimReport {
    pub claims: Vec<Claim>,
    pub g2: Vec<String>,
    pub g5: Vec<String>,
}

fn affirmed_match(sentence: &str, re: &regex::Regex, neg: &regex::Regex) -> Option<String> {
    re.find_iter(sentence).find(|m| !neg.is_match(&sentence[..m.start()])).map(|m| m.as_str().to_string())
}

/// G2 (claim extraction) and G5 (status consistency). G2: a conclusion-shaped sentence needs a
/// basis (a successful op, a fetched source, a derivation) or an honest tentative/unresolved label.
/// G5: a source reports rather than proves, and a tentative claim cannot be called proven.
pub fn claim_status(reply: &str, ops: &[OpRecord]) -> ClaimReport {
    let prose = strip_non_prose(reply);
    let re = |p: &str| regex::Regex::new(p).unwrap();
    let (conclusion, proof, neg) = (re(CONCLUSION), re(PROOF_WORDS), re(NEGATED));
    let (derived, tentative, unresolved) = (re(DERIVED), re(TENTATIVE), re(UNRESOLVED));
    let (tool_ref, source_ref, cite) = (re(TOOL_REF), re(SOURCE_REF), re(r"(?i)\bop#(\d+)\b"));
    let arxiv = re(r"(?i)\barxiv(?:\.org/(?:abs|pdf)/|:\s*|\s+)(\d{4}\.\d{4,5})");
    let ok = |kinds: &[&str]| ops.iter().any(|o| o.status == "ok" && kinds.contains(&o.kind.as_str()));
    let fetched: Vec<String> = ops.iter().filter(|o| o.status == "ok" && o.kind == "fetch").map(|o| o.target.to_ascii_lowercase()).collect();
    let mut report = ClaimReport::default();
    let sentence_re = re(r"[.!?]\s+|\n+");
    for sentence in sentence_re.split(&prose) {
        let end = sentence.as_ptr() as usize - prose.as_ptr() as usize + sentence.len();
        if sentence.trim_end().ends_with('?') || prose[end..].starts_with('?') || sentence.trim().is_empty() {
            continue;
        }
        let Some(trigger) = affirmed_match(sentence, &conclusion, &neg) else { continue };
        let quote: String = sentence.trim().chars().take(100).collect();
        let cited: Vec<&OpRecord> = cite.captures_iter(sentence).filter_map(|c| c[1].parse::<i64>().ok()).filter_map(|id| ops.iter().find(|o| o.id == id && o.status == "ok")).collect();
        let cited_tool = cited.iter().any(|o| o.kind != "fetch") || (tool_ref.is_match(sentence) && ok(&["run", "calc", "watch_run"]));
        let cited_source = cited.iter().any(|o| o.kind == "fetch")
            || arxiv.captures_iter(sentence).any(|c| fetched.iter().any(|t| t.contains(&c[1])))
            || (source_ref.is_match(sentence) && !fetched.is_empty() && !arxiv.is_match(sentence));
        let status = if unresolved.is_match(sentence) {
            ClaimStatus::Unresolved
        } else if tentative.is_match(sentence) {
            ClaimStatus::Tentative
        } else if cited_tool {
            ClaimStatus::VerifiedOperation
        } else if cited_source {
            ClaimStatus::SourceBacked
        } else if derived.is_match(sentence) {
            ClaimStatus::DerivedConditionally
        } else {
            ClaimStatus::NoBasis
        };
        let strong = affirmed_match(sentence, &proof, &neg);
        match (status, strong) {
            (ClaimStatus::NoBasis, _) => report.g2.push(format!("\"{quote}\" asserts a conclusion (\"{trigger}\") with no basis: cite the op#, the fetched source, or the assumptions it follows from, or mark it tentative / not checked.")),
            (ClaimStatus::SourceBacked, Some(w)) => report.g5.push(format!("\"{quote}\" says a source \"{w}\" it; a source-backed claim reports what the source says (\"reports\", \"states\", \"finds\"), not that it is proven.")),
            (ClaimStatus::Tentative | ClaimStatus::Unresolved, Some(w)) => report.g5.push(format!("\"{quote}\" is hedged or unchecked but says \"{w}\"; keep the wording as tentative as the status.")),
            _ => {}
        }
        report.claims.push(Claim { quote, status });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: i64, kind: &str, target: &str, result: &str) -> OpRecord {
        OpRecord { id, pane: "left".into(), thread: "t".into(), kind: kind.into(), target: target.into(), status: "ok".into(), result: result.into(), sha_before: String::new(), sha_after: String::new(), created_unix: 0 }
    }

    fn run94() -> Vec<OpRecord> {
        vec![op(94, "run", "python3 sims/toy.py", r#"{"exit_code":0,"stdout":"RESULT: omega=2.0000±0.01 rad/s\nRESULT: period=3.1416±0.02 s\n"}"#)]
    }

    #[test]
    fn numbers_from_a_run_pass_at_their_stated_precision() {
        let ops = run94();
        let (issues, checked) = untooled_numbers("The run (op#94) gives omega = 2.0000 ± 0.01 rad/s and a period of 3.14 s.", &ops, "");
        assert!(issues.is_empty(), "{issues:?}");
        assert!(checked >= 2);
        assert!(untooled_numbers("The period changed by −3.1416 s.", &ops, "").0.is_empty(), "sign-flipped deltas are the same number");
    }

    #[test]
    fn invented_numbers_are_flagged() {
        let ops = run94();
        let (issues, _) = untooled_numbers("sims/toy.py gives omega = 2.3817 rad/s and period = 2.6381 s.", &ops, "");
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(issues[0].contains("2.3817"));
        let (padded, _) = untooled_numbers("The two-loop result is 0.731 ± 0.000.", &ops, "");
        assert_eq!(padded.len(), 1, "{padded:?}");
    }

    #[test]
    fn inputs_identifiers_hedges_and_op_bodies_are_not_results() {
        let ops = run94();
        assert!(untooled_numbers("With k = 9.662 from params/toy.json the watch moved.", &ops, r#"set {"k": 9.662}"#).0.is_empty());
        assert!(untooled_numbers("See op#94, arXiv:1905.09311, sims/toy2.py and v9.0.2 from 2019.", &ops, "").0.is_empty());
        assert!(untooled_numbers("If k were doubled, omega would be 2.828 rad/s.", &ops, "").0.is_empty());
        assert!(untooled_numbers("A rough estimate is 5 GeV.", &ops, "").0.is_empty());
        let body = "<<<FILE path=\"sims/x.py\">>>\nk = 7.25\nprint(k * 3.3)\n<<<END FILE>>>\n```\nx = 8.8 m\n```\nDone.";
        assert!(untooled_numbers(body, &ops, "").0.is_empty(), "{:?}", untooled_numbers(body, &ops, ""));
    }

    #[test]
    fn units_do_not_match_inside_words() {
        let ops = run94();
        assert!(untooled_numbers("I made 3 small edits and 2 sims.", &ops, "").0.is_empty());
        assert_eq!(untooled_numbers("It took 12 ms.", &ops, "").0.len(), 1);
    }

    fn fetch85() -> OpRecord {
        op(85, "fetch", "https://arxiv.org/abs/1905.09311", "text")
    }

    #[test]
    fn conclusions_need_a_basis_or_an_honest_label() {
        let ops = run94();
        let r = claim_status("This proves the brane tension is 1 TeV.", &ops);
        assert_eq!(r.g2.len(), 1, "{r:?}");
        assert_eq!(r.claims[0].status, ClaimStatus::NoBasis);
        for ok in [
            "The run (op#94) confirms omega = 2.0000 rad/s.",
            "The simulation output shows that omega doubles when k quadruples.",
            "Assuming a harmonic potential, this shows that omega scales as sqrt(k).",
            "This might mean the tension is 1 TeV.",
            "Whether this rules out the model is underdetermined.",
            "This does not prove anything about the tension.",
            "Does this prove the tension is 1 TeV?",
            "The script printed two RESULT lines.",
        ] {
            let r = claim_status(ok, &ops);
            assert!(r.g2.is_empty() && r.g5.is_empty(), "{ok}: {r:?}");
        }
        assert!(claim_status("The simulation output shows that omega doubles.", &[]).g2.len() == 1, "a tool reference needs a real op");
    }

    #[test]
    fn sources_report_and_tentative_claims_stay_tentative() {
        let ops = vec![fetch85()];
        let r = claim_status("arXiv:1905.09311 proves that repulsion grows as energy falls.", &ops);
        assert_eq!((r.g2.len(), r.g5.len()), (0, 1), "{r:?}");
        assert_eq!(r.claims[0].status, ClaimStatus::SourceBacked);
        assert!(claim_status("arXiv:1905.09311 finds that repulsion grows; the paper shows that it drops below 11.5 GeV.", &ops).g5.is_empty());
        let t = claim_status("It seems likely, and this conclusively proves, that k sets omega.", &run94());
        assert_eq!(t.g5.len(), 1, "{t:?}");
        let unfetched = claim_status("arXiv:2001.00001 proves the tension is 1 TeV.", &ops);
        assert_eq!(unfetched.g2.len(), 1, "an unfetched citation is no basis: {unfetched:?}");
    }
}
