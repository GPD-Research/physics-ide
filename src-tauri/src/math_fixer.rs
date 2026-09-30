//! Deterministic scanner/rewriter for malformed math in markdown theory documents.
//!
//! The scanner segments each file into fenced/inline code (ignored), math spans
//! (`$..$`, `$$..$$`, `\(..\)`, `\[..\]`) and prose, then applies rule families:
//!
//! * `unicode_math`      – Unicode symbols inside math → LaTeX commands (auto)
//! * `unicode_prose`     – math symbols in prose → wrapped `$…$` (auto)
//! * `greek_name`        – spelled-out Greek letters → `\lambda` / `\Lambda` (choice)
//! * `shorthand`         – Python / typed notation → LaTeX (`x**2`, `sqrt(x)`, `3x3`, `1e-5`, `>=`)
//! * `bare_identifier`   – `M_eff`, `T_ij` in prose → `$M_{\text{eff}}$` (choice)
//! * `word_equation`     – typed equation in prose (`rho_c = 3*H**2/(8*pi*G)`) → `$…$` (review)
//! * `malformed`         – unbalanced braces / delimiters, empty math (manual)
//! * `unsupported_symbol`– emoji or symbols with no LaTeX equivalent (manual)
//!
//! Every finding carries the exact original span plus candidate replacements, so the
//! frontend can let the user accept, pick between options, or dismiss before anything
//! is written. `apply_math_fixes` verifies the original text is still present at the
//! recorded offset before rewriting and keeps a `.bak` copy of each touched file.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MathFinding {
    pub id: usize,
    pub file: String,
    pub relative_path: String,
    pub line: usize,
    pub column: usize,
    pub offset: usize,
    pub rule: String,
    /// `auto` (safe default), `review` (pick an option), `manual` (no automatic fix).
    pub severity: String,
    pub message: String,
    pub original: String,
    pub options: Vec<String>,
    pub context: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MathScanFileSummary {
    pub file: String,
    pub relative_path: String,
    pub findings: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MathScanReport {
    pub root: String,
    pub files_scanned: usize,
    pub files: Vec<MathScanFileSummary>,
    pub findings: Vec<MathFinding>,
    pub rule_counts: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MathFixSelection {
    pub file: String,
    pub offset: usize,
    pub original: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MathFixApplyReport {
    pub files_changed: usize,
    pub fixes_applied: usize,
    pub skipped: Vec<String>,
    pub backups: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    Code,
    Math,
    Prose,
}

#[derive(Debug, Clone)]
struct Segment {
    kind: SegmentKind,
    /// Byte range of the whole segment (delimiters included).
    start: usize,
    end: usize,
    /// Byte range of the content between delimiters.
    inner_start: usize,
    inner_end: usize,
}

const GREEK_NAMES: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta", "vartheta",
    "iota", "kappa", "lambda", "mu", "nu", "xi", "pi", "varpi", "rho", "varrho", "sigma", "varsigma",
    "tau", "upsilon", "phi", "varphi", "chi", "psi", "omega",
];

/// Names that have a distinct uppercase LaTeX form.
const GREEK_UPPER: &[&str] = &[
    "gamma", "delta", "theta", "lambda", "xi", "pi", "sigma", "upsilon", "phi", "psi", "omega",
];

const FUNCTION_NAMES: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "coth", "exp", "ln", "log", "lim", "min", "max", "det", "dim", "ker", "arg", "deg", "gcd", "sup", "inf",
];

fn unicode_math_replacement(ch: char) -> Option<&'static str> {
    Some(match ch {
        '−' | '‐' | '‑' | '‒' | '–' | '—' | '―' => "-",
        '×' => r"\times ",
        '·' | '⋅' | '∙' => r"\cdot ",
        '÷' => r"\div ",
        '±' => r"\pm ",
        '∓' => r"\mp ",
        '≈' => r"\approx ",
        '≃' => r"\simeq ",
        '≅' => r"\cong ",
        '≡' => r"\equiv ",
        '≠' => r"\neq ",
        '≤' | '⩽' => r"\leq ",
        '≥' | '⩾' => r"\geq ",
        '≪' => r"\ll ",
        '≫' => r"\gg ",
        '≲' => r"\lesssim ",
        '≳' => r"\gtrsim ",
        '∝' => r"\propto ",
        '∼' | '～' => r"\sim ",
        '→' => r"\to ",
        '←' => r"\leftarrow ",
        '↔' => r"\leftrightarrow ",
        '⇒' => r"\Rightarrow ",
        '⇐' => r"\Leftarrow ",
        '⇔' => r"\Leftrightarrow ",
        '↦' => r"\mapsto ",
        '∞' => r"\infty ",
        '∂' => r"\partial ",
        '∇' => r"\nabla ",
        '∫' => r"\int ",
        '∮' => r"\oint ",
        '∑' => r"\sum ",
        '∏' => r"\prod ",
        '√' => r"\sqrt ",
        '∈' => r"\in ",
        '∉' => r"\notin ",
        '∋' => r"\ni ",
        '⊂' => r"\subset ",
        '⊃' => r"\supset ",
        '⊆' => r"\subseteq ",
        '⊇' => r"\supseteq ",
        '∪' => r"\cup ",
        '∩' => r"\cap ",
        '∅' => r"\emptyset ",
        '∀' => r"\forall ",
        '∃' => r"\exists ",
        '¬' => r"\neg ",
        '∧' => r"\wedge ",
        '∨' => r"\vee ",
        '⊗' => r"\otimes ",
        '⊕' => r"\oplus ",
        '∘' => r"\circ ",
        '°' => r"^{\circ}",
        '′' => "'",
        '″' => "''",
        '‖' => r"\|",
        '〈' | '⟨' => r"\langle ",
        '〉' | '⟩' => r"\rangle ",
        'ℏ' => r"\hbar ",
        'ℓ' => r"\ell ",
        'ℜ' => r"\Re ",
        'ℑ' => r"\Im ",
        'ℵ' => r"\aleph ",
        '∴' => r"\therefore ",
        '∵' => r"\because ",
        '…' | '⋯' => r"\cdots ",
        '\u{00A0}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{2002}' | '\u{2003}' => " ",
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' => "",
        '\u{2018}' | '\u{2019}' => "'",
        '\u{201C}' | '\u{201D}' => "\"",
        'α' => r"\alpha ",
        'β' => r"\beta ",
        'γ' => r"\gamma ",
        'δ' => r"\delta ",
        'ε' => r"\varepsilon ",
        'ϵ' => r"\epsilon ",
        'ζ' => r"\zeta ",
        'η' => r"\eta ",
        'θ' => r"\theta ",
        'ϑ' => r"\vartheta ",
        'ι' => r"\iota ",
        'κ' => r"\kappa ",
        'λ' => r"\lambda ",
        'μ' | 'µ' => r"\mu ",
        'ν' => r"\nu ",
        'ξ' => r"\xi ",
        'π' => r"\pi ",
        'ρ' => r"\rho ",
        'ϱ' => r"\varrho ",
        'σ' => r"\sigma ",
        'ς' => r"\varsigma ",
        'τ' => r"\tau ",
        'υ' => r"\upsilon ",
        'φ' => r"\varphi ",
        'ϕ' => r"\phi ",
        'χ' => r"\chi ",
        'ψ' => r"\psi ",
        'ω' => r"\omega ",
        'Γ' => r"\Gamma ",
        'Δ' => r"\Delta ",
        'Θ' => r"\Theta ",
        'Λ' => r"\Lambda ",
        'Ξ' => r"\Xi ",
        'Π' => r"\Pi ",
        'Σ' => r"\Sigma ",
        'Υ' => r"\Upsilon ",
        'Φ' => r"\Phi ",
        'Ψ' => r"\Psi ",
        'Ω' => r"\Omega ",
        _ => return None,
    })
}

fn superscript_digit(ch: char) -> Option<char> {
    Some(match ch {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁺' => '+',
        '⁻' => '-',
        'ⁱ' => 'i',
        'ⁿ' => 'n',
        _ => return None,
    })
}

fn subscript_digit(ch: char) -> Option<char> {
    Some(match ch {
        '₀' => '0',
        '₁' => '1',
        '₂' => '2',
        '₃' => '3',
        '₄' => '4',
        '₅' => '5',
        '₆' => '6',
        '₇' => '7',
        '₈' => '8',
        '₉' => '9',
        '₊' => '+',
        '₋' => '-',
        _ => return None,
    })
}

/// True for symbols that must live inside math when the document is compiled with LaTeX.
fn is_math_symbol_for_prose(ch: char) -> bool {
    if superscript_digit(ch).is_some() || subscript_digit(ch).is_some() {
        return true;
    }
    match ch {
        // Typographic characters pandoc/LaTeX already handle in prose.
        '‐' | '‑' | '‒' | '–' | '—' | '―' | '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}'
        | '\u{00A0}' | '\u{2009}' | '\u{200A}' | '\u{202F}' | '\u{2002}' | '\u{2003}' | '\u{200B}'
        | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '…' | '°' | '′' | '″' => false,
        _ => unicode_math_replacement(ch).is_some(),
    }
}

// ---------------------------------------------------------------------------
// Segmentation
// ---------------------------------------------------------------------------

fn segment_markdown(text: &str) -> Vec<Segment> {
    let bytes = text.as_bytes();
    let mut segments = Vec::new();
    let mut prose_start = 0usize;
    let mut i = 0usize;
    let len = bytes.len();

    let flush_prose = |segments: &mut Vec<Segment>, from: usize, to: usize| {
        if to > from {
            segments.push(Segment { kind: SegmentKind::Prose, start: from, end: to, inner_start: from, inner_end: to });
        }
    };

    while i < len {
        let at_line_start = i == 0 || bytes[i - 1] == b'\n';

        // Fenced code block.
        if at_line_start && (text[i..].starts_with("```") || text[i..].starts_with("~~~")) {
            let fence = &text[i..i + 3];
            let line_end = text[i..].find('\n').map(|p| i + p + 1).unwrap_or(len);
            let mut close = len;
            let mut cursor = line_end;
            while cursor < len {
                let next_line_end = text[cursor..].find('\n').map(|p| cursor + p + 1).unwrap_or(len);
                if text[cursor..next_line_end].trim_start().starts_with(fence) {
                    close = next_line_end;
                    break;
                }
                cursor = next_line_end;
            }
            flush_prose(&mut segments, prose_start, i);
            segments.push(Segment { kind: SegmentKind::Code, start: i, end: close, inner_start: line_end, inner_end: close });
            i = close;
            prose_start = i;
            continue;
        }

        // HTML comment.
        if text[i..].starts_with("<!--") {
            let close = text[i + 4..].find("-->").map(|p| i + 4 + p + 3).unwrap_or(len);
            flush_prose(&mut segments, prose_start, i);
            segments.push(Segment { kind: SegmentKind::Code, start: i, end: close, inner_start: i + 4, inner_end: close });
            i = close;
            prose_start = i;
            continue;
        }

        // Inline code.
        if bytes[i] == b'`' {
            let mut ticks = 0;
            while i + ticks < len && bytes[i + ticks] == b'`' {
                ticks += 1;
            }
            let opener = &text[i..i + ticks];
            if let Some(rel) = text[i + ticks..].find(opener) {
                let close = i + ticks + rel + ticks;
                flush_prose(&mut segments, prose_start, i);
                segments.push(Segment { kind: SegmentKind::Code, start: i, end: close, inner_start: i + ticks, inner_end: close - ticks });
                i = close;
                prose_start = i;
                continue;
            }
            i += ticks;
            continue;
        }

        // Display math $$ ... $$
        if text[i..].starts_with("$$") {
            if let Some(rel) = text[i + 2..].find("$$") {
                let close = i + 2 + rel + 2;
                flush_prose(&mut segments, prose_start, i);
                segments.push(Segment { kind: SegmentKind::Math, start: i, end: close, inner_start: i + 2, inner_end: close - 2 });
                i = close;
                prose_start = i;
                continue;
            }
            i += 2;
            continue;
        }

        // \[ ... \] and \( ... \)
        if text[i..].starts_with("\\[") || text[i..].starts_with("\\(") {
            let closer = if bytes[i + 1] == b'[' { "\\]" } else { "\\)" };
            if let Some(rel) = text[i + 2..].find(closer) {
                let close = i + 2 + rel + 2;
                flush_prose(&mut segments, prose_start, i);
                segments.push(Segment { kind: SegmentKind::Math, start: i, end: close, inner_start: i + 2, inner_end: close - 2 });
                i = close;
                prose_start = i;
                continue;
            }
            i += 2;
            continue;
        }

        // Inline math $ ... $ (single line; `$` followed by whitespace or a digit-price is not math).
        if bytes[i] == b'$' {
            let line_end = text[i..].find('\n').map(|p| i + p).unwrap_or(len);
            let after = &text[i + 1..line_end];
            let opens_math = after
                .chars()
                .next()
                .map(|c| !c.is_whitespace())
                .unwrap_or(false);
            if opens_math {
                if let Some(rel) = after.find('$') {
                    let close = i + 1 + rel + 1;
                    let inner = &text[i + 1..close - 1];
                    let inner_is_currency = inner.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',' || c.is_whitespace() || c.is_alphabetic())
                        && inner.chars().any(|c| c.is_ascii_digit())
                        && inner.contains(' ');
                    if !inner.trim().is_empty() && !inner_is_currency && !inner.ends_with(' ') {
                        flush_prose(&mut segments, prose_start, i);
                        segments.push(Segment { kind: SegmentKind::Math, start: i, end: close, inner_start: i + 1, inner_end: close - 1 });
                        i = close;
                        prose_start = i;
                        continue;
                    }
                }
            }
            i += 1;
            continue;
        }

        // Advance one UTF-8 character.
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    flush_prose(&mut segments, prose_start, len);
    segments
}

// ---------------------------------------------------------------------------
// Shorthand → LaTeX converter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(String),
    Ident(String),
    Cmd(String),
    Op(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Space,
}

fn tokenize_shorthand(input: &str) -> Vec<Tok> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            if !matches!(tokens.last(), Some(Tok::Space)) {
                tokens.push(Tok::Space);
            }
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // scientific notation 1e-5 / 3E8
            if i < chars.len()
                && (chars[i] == 'e' || chars[i] == 'E')
                && i + 1 < chars.len()
                && (chars[i + 1].is_ascii_digit() || ((chars[i + 1] == '-' || chars[i + 1] == '+') && i + 2 < chars.len() && chars[i + 2].is_ascii_digit()))
            {
                i += 1;
                if chars[i] == '-' || chars[i] == '+' {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            tokens.push(Tok::Num(chars[start..i].iter().collect()));
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            tokens.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c == '\\' {
            let start = i;
            i += 1;
            if i < chars.len() && chars[i].is_alphabetic() {
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
            } else if i < chars.len() {
                i += 1;
            }
            tokens.push(Tok::Cmd(chars[start..i].iter().collect()));
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let op = match two.as_str() {
                "**" | ">=" | "<=" | "!=" | "->" | "=>" | "<<" | ">>" | "~=" | "==" | "=~" | "+-" => Some(two.clone()),
                _ => None,
            };
            if let Some(op) = op {
                tokens.push(Tok::Op(op));
                i += 2;
                continue;
            }
            match c {
                '(' | '[' => tokens.push(Tok::LParen),
                ')' | ']' => tokens.push(Tok::RParen),
                '{' => tokens.push(Tok::LBrace),
                '}' => tokens.push(Tok::RBrace),
                ',' => tokens.push(Tok::Comma),
                _ => tokens.push(Tok::Op(c.to_string())),
            }
            i += 1;
        }
    }
    tokens
}

fn greek_command(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    if !GREEK_NAMES.contains(&lower.as_str()) {
        return None;
    }
    let first_upper = name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
    if first_upper && GREEK_UPPER.contains(&lower.as_str()) {
        let mut s = lower.clone();
        if let Some(f) = s.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        Some(format!("\\{s}"))
    } else {
        Some(format!("\\{lower}"))
    }
}

fn render_ident(name: &str) -> String {
    if let Some(cmd) = greek_command(name) {
        return cmd;
    }
    if FUNCTION_NAMES.contains(&name) {
        return format!("\\{name}");
    }
    match name {
        "inf" | "infinity" | "Infinity" => return "\\infty".to_string(),
        "hbar" => return "\\hbar".to_string(),
        "nabla" | "grad" => return "\\nabla".to_string(),
        "partial" => return "\\partial".to_string(),
        "sum" => return "\\sum".to_string(),
        "prod" => return "\\prod".to_string(),
        "int" => return "\\int".to_string(),
        _ => {}
    }
    if name.chars().count() > 1 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        // `dt`, `dx` are differentials; `GM`, `kT` read as products of single-letter
        // symbols; longer names (`eff`, `Pl`, `scale`) are upright words.
        let mut chars = name.chars();
        let first = chars.next().unwrap_or(' ');
        if name.len() == 2 {
            if first == 'd' {
                return format!("d{}", chars.as_str());
            }
            if name.chars().all(|c| c.is_ascii_uppercase()) {
                return name.to_string();
            }
        }
        return format!("\\mathrm{{{name}}}");
    }
    // H0, T2 → H_{0}, T_{2}
    let letters: String = name.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let digits = &name[letters.len()..];
    if letters.chars().count() == 1 && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return format!("{letters}_{{{digits}}}");
    }
    name.to_string()
}

fn render_number(num: &str) -> String {
    let lower = num.to_ascii_lowercase();
    if let Some(pos) = lower.find('e') {
        let mantissa = &num[..pos];
        let exponent = num[pos + 1..].trim_start_matches('+');
        let exponent = exponent.strip_prefix('-').map(|e| format!("-{}", e.trim_start_matches('0').trim_start_matches('0')))
            .unwrap_or_else(|| exponent.to_string());
        let exponent = if exponent == "-" { "-0".to_string() } else { exponent };
        if mantissa.is_empty() || mantissa == "1" {
            return format!("10^{{{exponent}}}");
        }
        return format!("{mantissa} \\times 10^{{{exponent}}}");
    }
    num.to_string()
}

/// Parse a balanced group starting at `i` (which must be `LParen`). Returns the
/// rendered inner text and the index after the closing paren.
fn render_group(tokens: &[Tok], i: usize) -> Option<(String, usize)> {
    if tokens.get(i) != Some(&Tok::LParen) {
        return None;
    }
    let mut depth = 0;
    let mut j = i;
    while j < tokens.len() {
        match tokens[j] {
            Tok::LParen => depth += 1,
            Tok::RParen => {
                depth -= 1;
                if depth == 0 {
                    let inner = render_tokens(&tokens[i + 1..j]);
                    return Some((inner, j + 1));
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Render a single operand (number, identifier, command, or parenthesised group) for
/// use in a `^{}` / `_{}` / `\frac{}{}` slot. Returns rendered text and next index.
fn render_atom(tokens: &[Tok], i: usize) -> Option<(String, usize, bool)> {
    match tokens.get(i)? {
        Tok::Num(n) => Some((render_number(n), i + 1, false)),
        Tok::Ident(s) => {
            if let Some((call, next)) = render_call(tokens, i) {
                return Some((call, next, false));
            }
            Some((render_ident(s), i + 1, false))
        }
        Tok::Cmd(c) => Some((c.clone(), i + 1, false)),
        Tok::LParen => render_group(tokens, i).map(|(s, next)| (s, next, true)),
        Tok::Op(op) if op == "-" || op == "+" => {
            let (inner, next, grouped) = render_atom(tokens, i + 1)?;
            Some((format!("{op}{inner}"), next, grouped))
        }
        _ => None,
    }
}

/// `sqrt(x)`, `sin(x)`, `abs(x)` at `i` → rendered call and next index.
fn render_call(tokens: &[Tok], i: usize) -> Option<(String, usize)> {
    let Tok::Ident(name) = tokens.get(i)? else { return None };
    if !(FUNCTION_NAMES.contains(&name.as_str()) || name == "sqrt" || name == "abs") {
        return None;
    }
    let (inner, next) = render_group(tokens, i + 1)?;
    let rendered = match name.as_str() {
        "sqrt" => format!("\\sqrt{{{inner}}}"),
        "abs" => format!("\\left|{inner}\\right|"),
        _ => format!("{}\\left({inner}\\right)", render_ident(name)),
    };
    Some((rendered, next))
}

/// `(a+b)` → true; `(a)(b)` / `a(b)` → false.
fn is_single_paren_group(s: &str) -> bool {
    if !s.starts_with('(') || !s.ends_with(')') {
        return false;
    }
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i == s.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

fn render_tokens(tokens: &[Tok]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Tok::Space => {
                if let Some(last) = out.last() {
                    if !last.ends_with(' ') {
                        out.push(" ".to_string());
                    }
                }
                i += 1;
            }
            Tok::Num(n) => {
                out.push(render_number(n));
                i += 1;
            }
            Tok::Ident(name) => {
                if let Some((call, next)) = render_call(tokens, i) {
                    out.push(call);
                    i = next;
                    continue;
                }
                out.push(render_ident(name));
                i += 1;
            }
            Tok::Cmd(cmd) => {
                out.push(cmd.clone());
                i += 1;
            }
            Tok::LParen => {
                if let Some((inner, next)) = render_group(tokens, i) {
                    out.push(format!("({inner})"));
                    i = next;
                } else {
                    out.push("(".to_string());
                    i += 1;
                }
            }
            Tok::RParen => {
                out.push(")".to_string());
                i += 1;
            }
            Tok::LBrace => {
                out.push("{".to_string());
                i += 1;
            }
            Tok::RBrace => {
                out.push("}".to_string());
                i += 1;
            }
            Tok::Comma => {
                out.push(", ".to_string());
                i += 1;
            }
            Tok::Op(op) => {
                let op = op.as_str();
                match op {
                    "**" | "^" => {
                        if let Some((inner, mut next, _)) = render_atom(tokens, i + 1) {
                            // `pi^-8/3` → `\pi^{-8/3}`: a numeric exponent followed by `/number`
                            // is a fractional exponent, not a fraction of the power.
                            let mut exponent = inner;
                            let numeric = exponent.trim_start_matches(['-', '+']).chars().all(|c| c.is_ascii_digit() || c == '.');
                            if numeric && tokens.get(next) == Some(&Tok::Op("/".to_string())) {
                                if let Some(Tok::Num(den)) = tokens.get(next + 1) {
                                    exponent = format!("{exponent}/{den}");
                                    next += 2;
                                }
                            }
                            out.push(format!("^{{{exponent}}}"));
                            i = next;
                            continue;
                        }
                        out.push("^".to_string());
                        i += 1;
                    }
                    "_" => {
                        if let Some((inner, next, grouped)) = render_atom(tokens, i + 1) {
                            out.push(format!("_{{{}}}", subscript_text(inner, grouped)));
                            i = next;
                            continue;
                        }
                        out.push("_".to_string());
                        i += 1;
                    }
                    "/" => {
                        // term/b → \frac{term}{b}; the numerator is the preceding product term
                        // (atoms, scripts and juxtaposition spaces up to the last spaced operator).
                        let mut term_start = out.len();
                        while term_start > 0 {
                            let piece = out[term_start - 1].as_str();
                            let spaced_operator = piece != PRODUCT_BOUNDARY && piece.len() > 1 && piece.starts_with(' ') && piece.ends_with(' ');
                            if spaced_operator || matches!(piece, "(" | "{" | "," | ", ") {
                                break;
                            }
                            term_start -= 1;
                        }
                        // `a/b * c/d` reads as a product of fractions: an explicit `*` after an
                        // existing fraction starts a new factor.
                        if let Some(last_boundary) = out[term_start..].iter().rposition(|p| p == PRODUCT_BOUNDARY) {
                            if out[term_start..term_start + last_boundary].iter().any(|p| p.contains("\\frac")) {
                                term_start += last_boundary + 1;
                            }
                        }
                        // A leading unary sign stays outside the fraction.
                        if term_start < out.len() && matches!(out[term_start].as_str(), "-" | "+") {
                            term_start += 1;
                        }
                        let numerator: Option<String> = if term_start < out.len() {
                            Some(out[term_start..].concat().trim().to_string()).filter(|s| !s.is_empty())
                        } else {
                            None
                        };
                        let mut den_index = i + 1;
                        while tokens.get(den_index) == Some(&Tok::Space) {
                            den_index += 1;
                        }
                        if let (Some(num), Some((den, next, _))) = (numerator, render_atom(tokens, den_index)) {
                            let num_is_group = is_single_paren_group(&num);
                            if !num.ends_with('=') && !matches!(num.as_str(), "+" | "-" | "=" | "(") {
                                // Absorb any subscript/superscript attached to denominator.
                                let mut den = den;
                                let mut next = next;
                                while let Some(Tok::Op(o)) = tokens.get(next) {
                                    if (o == "^" || o == "**" || o == "_") && render_atom(tokens, next + 1).is_some() {
                                        let (inner, after, grouped) = render_atom(tokens, next + 1).unwrap();
                                        let inner = if o == "_" { subscript_text(inner, grouped) } else { inner };
                                        let mark = if o == "_" { "_" } else { "^" };
                                        den.push_str(&format!("{mark}{{{inner}}}"));
                                        next = after;
                                    } else {
                                        break;
                                    }
                                }
                                out.truncate(term_start);
                                let num = if num_is_group { num[1..num.len() - 1].to_string() } else { num };
                                out.push(format!("\\frac{{{num}}}{{{den}}}"));
                                i = next;
                                continue;
                            }
                        }
                        out.push("/".to_string());
                        i += 1;
                    }
                    "*" => {
                        let prev_num = matches!(out.iter().rev().find(|s| !s.trim().is_empty()).map(|s| s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)), Some(true));
                        let next_num = matches!(tokens.get(i + 1), Some(Tok::Num(_))) || matches!((tokens.get(i + 1), tokens.get(i + 2)), (Some(Tok::Space), Some(Tok::Num(_))));
                        if prev_num && next_num {
                            out.push(" \\cdot ".to_string());
                        } else {
                            out.push(PRODUCT_BOUNDARY.to_string());
                        }
                        i += 1;
                    }
                    ">=" => { out.push(" \\geq ".to_string()); i += 1; }
                    "<=" => { out.push(" \\leq ".to_string()); i += 1; }
                    "!=" => { out.push(" \\neq ".to_string()); i += 1; }
                    "->" => { out.push(" \\to ".to_string()); i += 1; }
                    "=>" => { out.push(" \\Rightarrow ".to_string()); i += 1; }
                    "<<" => { out.push(" \\ll ".to_string()); i += 1; }
                    ">>" => { out.push(" \\gg ".to_string()); i += 1; }
                    "~=" | "=~" => { out.push(" \\approx ".to_string()); i += 1; }
                    "==" => { out.push(" = ".to_string()); i += 1; }
                    "+-" => { out.push(" \\pm ".to_string()); i += 1; }
                    "~" => { out.push(" \\sim ".to_string()); i += 1; }
                    "=" | "+" | "<" | ">" => { out.push(format!(" {op} ")); i += 1; }
                    "-" => {
                        let unary = out.iter().rev().find(|s| !s.trim().is_empty()).map(|s| s.ends_with('=') || s.ends_with('(') || s.ends_with('{') || s.trim().ends_with("\\times")).unwrap_or(true);
                        out.push(if unary { "-".to_string() } else { " - ".to_string() });
                        i += 1;
                    }
                    "%" => { out.push("\\%".to_string()); i += 1; }
                    "&" => { out.push("\\&".to_string()); i += 1; }
                    _ => { out.push(op.to_string()); i += 1; }
                }
            }
        }
    }
    let joined: String = out.concat().replace(PRODUCT_BOUNDARY, " ");
    collapse_spaces(joined.trim())
}

/// Placeholder emitted for explicit `*` so `a/b * c/d` keeps its factors apart when
/// building fractions; rendered as a plain space at the end.
const PRODUCT_BOUNDARY: &str = " \u{1}* ";

/// Word-like subscripts (`eff`, `scale`) are labels, not products: use `\text{}`.
fn subscript_text(inner: String, grouped: bool) -> String {
    if !grouped && inner.starts_with("\\mathrm{") {
        inner.replacen("\\mathrm{", "\\text{", 1)
    } else {
        inner
    }
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                out.push(c);
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

/// Convert typed / Python-style shorthand into LaTeX. Public for tests and the UI preview.
pub fn shorthand_to_latex(input: &str) -> String {
    // NxN / 3x3 matrix shorthand before tokenizing so the `x` is not read as a variable.
    let pre = replace_dimension_x(input);
    render_tokens(&tokenize_shorthand(&pre))
}

fn replace_dimension_x(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == 'x' || chars[i] == 'X')
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
            && (i < 2 || !chars[i - 2].is_alphabetic())
        {
            out.push_str(" \\times ");
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Rule application
// ---------------------------------------------------------------------------

struct FindingBuilder<'a> {
    text: &'a str,
    file: String,
    relative_path: String,
    findings: Vec<MathFinding>,
}

impl<'a> FindingBuilder<'a> {
    fn push(&mut self, offset: usize, original: &str, rule: &str, severity: &str, message: String, options: Vec<String>) {
        let (line, column) = line_column(self.text, offset);
        let context = line_text(self.text, offset);
        self.findings.push(MathFinding {
            id: 0,
            file: self.file.clone(),
            relative_path: self.relative_path.clone(),
            line,
            column,
            offset,
            rule: rule.to_string(),
            severity: severity.to_string(),
            message,
            original: original.to_string(),
            options,
            context,
        });
    }
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rfind('\n').map(|p| before[p + 1..].chars().count()).unwrap_or_else(|| before.chars().count()) + 1;
    (line, column)
}

fn line_text(text: &str, offset: usize) -> String {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let end = text[offset..].find('\n').map(|p| offset + p).unwrap_or(text.len());
    let line = text[start..end].trim_end();
    if line.chars().count() > 240 {
        let truncated: String = line.chars().take(240).collect();
        format!("{truncated}…")
    } else {
        line.to_string()
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Finds standalone spelled-out Greek names in a span. Returns (offset, name).
fn find_greek_words(span: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    for (idx, word) in split_words(span) {
        let lower = word.to_ascii_lowercase();
        if !GREEK_NAMES.contains(&lower.as_str()) {
            continue;
        }
        let before = span[..idx].chars().next_back();
        if before == Some('\\') {
            continue;
        }
        // Inside \text{...} or \mathrm{...} the word is prose.
        if inside_text_command(span, idx) {
            continue;
        }
        found.push((idx, word));
    }
    found
}

fn split_words(span: &str) -> Vec<(usize, &str)> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in span.char_indices() {
        if c.is_ascii_alphabetic() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            if !is_word_char(c) {
                words.push((s, &span[s..i]));
            } else {
                // alnum/underscore continues an identifier → not a standalone word
                let mut j = i;
                while j < span.len() && span[j..].chars().next().map(is_word_char).unwrap_or(false) {
                    j += span[j..].chars().next().unwrap().len_utf8();
                }
                let _ = j;
            }
        }
    }
    if let Some(s) = start {
        words.push((s, &span[s..]));
    }
    // Filter words preceded by an identifier char (e.g. `x_mu` handled elsewhere).
    words
        .into_iter()
        .filter(|(i, _)| span[..*i].chars().next_back().map(|c| !is_word_char(c)).unwrap_or(true))
        .collect()
}

fn inside_text_command(span: &str, idx: usize) -> bool {
    let before = &span[..idx];
    for cmd in ["\\text{", "\\mathrm{", "\\textrm{", "\\mathit{", "\\operatorname{", "\\mbox{"] {
        if let Some(pos) = before.rfind(cmd) {
            let after_cmd = &before[pos + cmd.len()..];
            let opens = after_cmd.matches('{').count();
            let closes = after_cmd.matches('}').count();
            if closes <= opens {
                return true;
            }
        }
    }
    false
}

fn greek_options(name: &str) -> Vec<String> {
    let lower = name.to_ascii_lowercase();
    let mut options = Vec::new();
    let first_upper = name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
    let upper = if GREEK_UPPER.contains(&lower.as_str()) {
        let mut s = lower.clone();
        if let Some(f) = s.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        Some(format!("\\{s}"))
    } else {
        None
    };
    let lower_cmd = format!("\\{lower}");
    if first_upper {
        if let Some(u) = &upper {
            options.push(u.clone());
        }
        options.push(lower_cmd);
    } else {
        options.push(lower_cmd);
        if let Some(u) = upper {
            options.push(u);
        }
    }
    options
}

fn brace_balance(span: &str) -> i64 {
    let mut balance = 0i64;
    let mut chars = span.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => balance += 1,
            '}' => balance -= 1,
            _ => {}
        }
    }
    balance
}

fn scan_math_span(b: &mut FindingBuilder, inner: &str, inner_start: usize, whole: &str, whole_start: usize) {
    if inner.trim().is_empty() {
        b.push(whole_start, whole, "malformed", "manual", "Empty math span.".to_string(), vec![String::new()]);
        return;
    }
    let balance = brace_balance(inner);
    if balance != 0 {
        b.push(
            whole_start,
            whole,
            "malformed",
            "manual",
            format!("Unbalanced braces in math span ({} unmatched).", if balance > 0 { format!("{balance} open") } else { format!("{} close", -balance) }),
            vec![],
        );
    }
    let paren_balance = inner.chars().fold(0i64, |acc, c| match c {
        '(' => acc + 1,
        ')' => acc - 1,
        _ => acc,
    });
    if paren_balance != 0 && !inner.contains("\\left") && !inner.contains("\\right") {
        b.push(whole_start, whole, "malformed", "manual", "Unbalanced parentheses in math span.".to_string(), vec![]);
    }

    // Unicode symbols inside math.
    let mut iter = inner.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c.is_ascii() {
            continue;
        }
        if superscript_digit(c).is_some() || subscript_digit(c).is_some() {
            let is_super = superscript_digit(c).is_some();
            let mut end = i + c.len_utf8();
            let mut digits = String::new();
            digits.push(if is_super { superscript_digit(c).unwrap() } else { subscript_digit(c).unwrap() });
            while let Some(&(j, n)) = iter.peek() {
                let mapped = if is_super { superscript_digit(n) } else { subscript_digit(n) };
                match mapped {
                    Some(d) => {
                        digits.push(d);
                        end = j + n.len_utf8();
                        iter.next();
                    }
                    None => break,
                }
            }
            let mark = if is_super { "^" } else { "_" };
            b.push(
                inner_start + i,
                &inner[i..end],
                "unicode_math",
                "auto",
                format!("Unicode {} digits → LaTeX {}{{}}.", if is_super { "superscript" } else { "subscript" }, mark),
                vec![format!("{mark}{{{digits}}}")],
            );
            continue;
        }
        if let Some(rep) = unicode_math_replacement(c) {
            let original = &inner[i..i + c.len_utf8()];
            let mut replacement = rep.to_string();
            // Avoid doubling spaces when the next char is already a space / brace.
            if replacement.ends_with(' ') {
                let next = inner[i + c.len_utf8()..].chars().next();
                if matches!(next, Some(' ') | Some('}') | Some('$') | Some('_') | Some('^') | Some(')') | None) {
                    replacement = replacement.trim_end().to_string();
                }
            }
            b.push(
                inner_start + i,
                original,
                "unicode_math",
                "auto",
                format!("Unicode `{original}` in math → `{}`.", replacement.trim()),
                vec![replacement],
            );
            continue;
        }
        if c.is_alphabetic() && c.len_utf8() == 2 && (c as u32) < 0x250 {
            // Latin letters with diacritics render under xelatex; leave alone.
            continue;
        }
        b.push(
            inner_start + i,
            &inner[i..i + c.len_utf8()],
            "unsupported_symbol",
            "manual",
            format!("Character `{c}` (U+{:04X}) has no LaTeX equivalent; replace or remove by hand.", c as u32),
            vec![String::new()],
        );
    }

    // Spelled-out Greek names.
    for (idx, word) in find_greek_words(inner) {
        let options = greek_options(word);
        b.push(
            inner_start + idx,
            word,
            "greek_name",
            "review",
            format!("Spelled-out Greek letter `{word}` in math; choose `{}`.", options.join("` or `")),
            options,
        );
    }

    // Shorthand operators.
    scan_shorthand_in_math(b, inner, inner_start);
}

fn scan_shorthand_in_math(b: &mut FindingBuilder, inner: &str, inner_start: usize) {
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ascii_two = i + 2 <= bytes.len() && bytes[i].is_ascii() && bytes[i + 1].is_ascii();
        let two = if ascii_two { &inner[i..i + 2] } else { "" };
        // Python power.
        if ascii_two && two == "**" {
            let (end, rendered) = shorthand_tail(inner, i, "^");
            b.push(inner_start + i, &inner[i..end], "shorthand", "auto", "Python power `**` → `^{}`.".to_string(), vec![rendered]);
            i = end;
            continue;
        }
        if ascii_two && matches!(two, ">=" | "<=" | "!=" | "->" | "=>" | "<<" | ">>" | "~=" | "+-") {
            let prev = inner[..i].chars().next_back();
            let next = inner[i + 2..].chars().next();
            if prev != Some('\\') && prev != Some('-') && prev != Some('<') && prev != Some('>') && next != Some('-') && next != Some('>') && next != Some('=') {
                let rendered = render_tokens(&tokenize_shorthand(two));
                b.push(inner_start + i, two, "shorthand", "auto", format!("Typed operator `{two}` → `{}`.", rendered.trim()), vec![rendered]);
                i += 2;
                continue;
            }
        }
        // Unbraced multi-character sub/superscript: x_10, x_eff, x^-1, x^10 (single char is fine).
        if (bytes[i] == b'_' || bytes[i] == b'^') && i + 1 < bytes.len() && bytes[i + 1] != b'{' && bytes[i + 1] != b'\\' && bytes[i + 1] != b' ' {
            let start = i + 1;
            let mut end = start;
            let leading_sign = bytes[start] == b'-' || bytes[start] == b'+';
            if leading_sign {
                end += 1;
            }
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric()) {
                end += 1;
            }
            let body = &inner[start..end];
            let core = if leading_sign { &body[1..] } else { body };
            // `x_10`, `x^-1`, `x^10`, `M_eff` need braces; `d^3x`, `e^+e^-`, `\ell_zT_R`
            // are legitimate single-character scripts followed by more math.
            let is_alpha = core.chars().all(|c| c.is_ascii_alphabetic()) && core.len() >= 3
                && (core.chars().all(|c| c.is_ascii_lowercase()) || core.chars().all(|c| c.is_ascii_uppercase()));
            let is_numeric = !core.is_empty() && core.chars().all(|c| c.is_ascii_digit()) && (core.len() >= 2 || leading_sign);
            if is_alpha || is_numeric {
                let mark = &inner[i..i + 1];
                let mut options = Vec::new();
                if is_alpha && mark == "_" {
                    options.push(format!("{mark}{{\\text{{{body}}}}}"));
                    options.push(format!("{mark}{{{body}}}"));
                } else {
                    options.push(format!("{mark}{{{body}}}"));
                }
                let severity = if is_alpha { "review" } else { "auto" };
                b.push(
                    inner_start + i,
                    &inner[i..end],
                    "shorthand",
                    severity,
                    format!("Multi-character script `{}` needs braces.", &inner[i..end]),
                    options,
                );
                i = end;
                continue;
            }
        }
        // Function names without backslash: sin(, exp(, sqrt(
        if bytes[i].is_ascii_alphabetic() && (i == 0 || !is_word_char(inner[..i].chars().next_back().unwrap_or(' ')) ) && inner[..i].chars().next_back() != Some('\\') {
            let mut end = i;
            while end < bytes.len() && bytes[end].is_ascii_alphabetic() {
                end += 1;
            }
            let word = &inner[i..end];
            let followed_by_paren = inner[end..].trim_start().starts_with('(');
            if (FUNCTION_NAMES.contains(&word) || word == "sqrt") && followed_by_paren && !inside_text_command(inner, i) {
                if let Some((group, group_end)) = find_paren_group(inner, end) {
                    let rendered = shorthand_to_latex(&inner[i..group_end]);
                    let _ = group;
                    b.push(inner_start + i, &inner[i..group_end], "shorthand", "auto", format!("Function `{word}` written without backslash."), vec![rendered]);
                    i = group_end;
                    continue;
                }
            }
            i = end;
            continue;
        }
        // 3x3 dimension shorthand.
        if (bytes[i] == b'x' || bytes[i] == b'X') && i > 0 && bytes[i - 1].is_ascii_digit() && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            b.push(inner_start + i, &inner[i..i + 1], "shorthand", "auto", "Dimension `NxN` → `N \\times N`.".to_string(), vec![" \\times ".to_string()]);
            i += 1;
            continue;
        }
        // Scientific notation 1e-5.
        if bytes[i].is_ascii_digit() && (i == 0 || !is_word_char(inner[..i].chars().next_back().unwrap_or(' '))) {
            let mut end = i;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
                end += 1;
            }
            if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
                let mut e_end = end + 1;
                if e_end < bytes.len() && (bytes[e_end] == b'-' || bytes[e_end] == b'+') {
                    e_end += 1;
                }
                let digits_start = e_end;
                while e_end < bytes.len() && bytes[e_end].is_ascii_digit() {
                    e_end += 1;
                }
                if e_end > digits_start && (e_end == bytes.len() || !bytes[e_end].is_ascii_alphabetic()) {
                    let original = &inner[i..e_end];
                    b.push(inner_start + i, original, "shorthand", "auto", format!("Scientific notation `{original}` → powers of ten."), vec![render_number(original)]);
                    i = e_end;
                    continue;
                }
            }
            i = end.max(i + 1);
            continue;
        }
        i += inner[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
}

fn find_paren_group(s: &str, from: usize) -> Option<(&str, usize)> {
    let open = from + s[from..].find('(')?;
    if !s[from..open].trim().is_empty() {
        return None;
    }
    let mut depth = 0;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let end = open + i + 1;
                    return Some((&s[open..end], end));
                }
            }
            _ => {}
        }
    }
    None
}

/// For `**` at `i`, consume the exponent operand and render `^{...}`.
fn shorthand_tail(inner: &str, i: usize, mark: &str) -> (usize, String) {
    let after = &inner[i + 2..];
    let bytes = after.as_bytes();
    let mut end = 0;
    if !bytes.is_empty() && bytes[0] == b'(' {
        if let Some((_, group_end)) = find_paren_group(after, 0) {
            let rendered = shorthand_to_latex(&after[1..group_end - 1]);
            return (i + 2 + group_end, format!("{mark}{{{rendered}}}"));
        }
    }
    if !bytes.is_empty() && bytes[0] == b'{' {
        return (i + 2, mark.to_string());
    }
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        end += 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'.') {
        end += 1;
    }
    if end == 0 {
        return (i + 2, mark.to_string());
    }
    (i + 2 + end, format!("{mark}{{{}}}", &after[..end]))
}

fn scan_prose_span(b: &mut FindingBuilder, span: &str, span_start: usize) {
    // Unicode math symbols in prose → wrap in $...$, attaching adjacent numbers.
    let chars: Vec<(usize, char)> = span.char_indices().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let (i, c) = chars[idx];
        if c.is_ascii() {
            idx += 1;
            continue;
        }
        if is_math_symbol_for_prose(c) {
            // Extend over a contiguous run of math symbols and adjacent numeric text.
            let mut run_start_idx = idx;
            let mut run_end_idx = idx + 1;
            // Include number immediately before (e.g. 400×, 5.557σ, 10⁻¹).
            while run_start_idx > 0 {
                let p = chars[run_start_idx - 1].1;
                if p.is_ascii_digit() || p == '.' || p == ',' && run_start_idx > 1 && chars[run_start_idx - 2].1.is_ascii_digit() {
                    run_start_idx -= 1;
                } else {
                    break;
                }
            }
            while run_start_idx < idx && !chars[run_start_idx].1.is_ascii_digit() {
                run_start_idx += 1;
            }
            // `m²`, `T₂`, `Hz⁻¹`: a script digit belongs to the short symbol before it.
            if run_start_idx == idx && (superscript_digit(c).is_some() || subscript_digit(c).is_some() || c == '⁻') {
                let mut word_start = idx;
                while word_start > 0 && chars[word_start - 1].1.is_ascii_alphabetic() {
                    word_start -= 1;
                }
                let word_len = idx - word_start;
                let word_boundary = word_start == 0 || !is_word_char(chars[word_start - 1].1);
                if (1..=2).contains(&word_len) && word_boundary {
                    run_start_idx = word_start;
                }
            }
            // Extend forwards over more symbols, digits, and sub/superscripts.
            while run_end_idx < chars.len() {
                let n = chars[run_end_idx].1;
                if is_math_symbol_for_prose(n) || (n.is_ascii_digit() && run_end_idx > idx && (is_math_symbol_for_prose(chars[run_end_idx - 1].1) || chars[run_end_idx - 1].1.is_ascii_digit() || chars[run_end_idx - 1].1 == '.')) || (n == '.' && run_end_idx + 1 < chars.len() && chars[run_end_idx + 1].1.is_ascii_digit()) {
                    run_end_idx += 1;
                } else {
                    break;
                }
            }
            let start_byte = chars[run_start_idx].0;
            let end_byte = if run_end_idx < chars.len() { chars[run_end_idx].0 } else { span.len() };
            let original = &span[start_byte..end_byte];
            let rendered = latexify_symbol_run(original);
            b.push(
                span_start + start_byte,
                original,
                "unicode_prose",
                "auto",
                format!("Math symbol `{original}` in prose must be inside math: `${}$`.", rendered),
                vec![format!("${rendered}$")],
            );
            idx = run_end_idx;
            continue;
        }
        if unicode_math_replacement(c).is_some() || c.is_alphabetic() || c.is_ascii_punctuation() {
            idx += 1;
            continue;
        }
        if is_probably_emoji(c) {
            b.push(
                span_start + i,
                &span[i..i + c.len_utf8()],
                "unsupported_symbol",
                "manual",
                format!("Symbol `{c}` (U+{:04X}) will not compile under pdflatex; remove or replace.", c as u32),
                vec![String::new()],
            );
            // Skip variation selectors / joiners following the emoji.
            idx += 1;
            while idx < chars.len() && matches!(chars[idx].1 as u32, 0xFE0F | 0x200D | 0x20E3) {
                idx += 1;
            }
            continue;
        }
        idx += 1;
    }

    // Bare identifiers with subscripts: M_eff, T_ij, G_w.
    for (start, end) in find_bare_identifiers(span) {
        let ident = &span[start..end];
        let (base, sub) = ident.split_once('_').unwrap();
        let base_rendered = greek_command(base).unwrap_or_else(|| base.to_string());
        let mut options = Vec::new();
        if sub.chars().count() == 1 {
            options.push(format!("${base_rendered}_{sub}$"));
        } else if sub.chars().all(|c| c.is_ascii_alphabetic()) {
            options.push(format!("${base_rendered}_{{\\text{{{sub}}}}}$"));
            options.push(format!("${base_rendered}_{{{sub}}}$"));
        } else {
            options.push(format!("${base_rendered}_{{{sub}}}$"));
        }
        b.push(
            span_start + start,
            ident,
            "bare_identifier",
            "review",
            format!("Identifier `{ident}` outside math will be typeset as italics/underscore; wrap as `{}`.", options[0]),
            options,
        );
    }

    // Spelled-out Greek words in prose.
    for (idx, word) in find_greek_words(span) {
        let lower = word.to_ascii_lowercase();
        // Common English collisions: only flag when the word sits in a math-like context.
        let english_collision = matches!(lower.as_str(), "alpha" | "beta" | "delta" | "gamma" | "iota" | "omega" | "theta" | "kappa" | "eta" | "chi" | "phi" | "psi" | "xi" | "tau" | "rho" | "zeta" | "sigma" | "lambda" | "epsilon" | "upsilon");
        if english_collision && !greek_word_in_math_context(span, idx, word.len()) {
            continue;
        }
        let options: Vec<String> = greek_options(word).into_iter().map(|o| format!("${o}$")).collect();
        b.push(
            span_start + idx,
            word,
            "greek_name",
            "review",
            format!("Spelled-out Greek letter `{word}`; choose `{}`.", options.join("` or `")),
            options,
        );
    }

    // Typed equations in prose.
    for (start, end) in find_word_equations(span) {
        let original = &span[start..end];
        let rendered = shorthand_to_latex(original);
        if rendered.trim().is_empty() || rendered == original {
            continue;
        }
        b.push(
            span_start + start,
            original,
            "word_equation",
            "review",
            format!("Typed equation `{original}` → `${rendered}$`."),
            vec![format!("${rendered}$")],
        );
    }
}

fn is_probably_emoji(c: char) -> bool {
    let cp = c as u32;
    (0x1F000..=0x1FAFF).contains(&cp)
        || (0x2600..=0x27BF).contains(&cp)
        || (0x2B00..=0x2BFF).contains(&cp)
        || (0x2300..=0x23FF).contains(&cp)
        || cp == 0x2705 || cp == 0x274C || cp == 0x2714 || cp == 0x2716
}

fn latexify_symbol_run(run: &str) -> String {
    let mut out = String::new();
    let mut chars = run.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(d) = superscript_digit(c) {
            let mut digits = String::from(d);
            while let Some(&n) = chars.peek() {
                match superscript_digit(n) {
                    Some(d) => {
                        digits.push(d);
                        chars.next();
                    }
                    None => break,
                }
            }
            let trimmed = out.trim_end().len();
            out.truncate(trimmed);
            out.push_str(&format!("^{{{digits}}}"));
        } else if let Some(d) = subscript_digit(c) {
            let mut digits = String::from(d);
            while let Some(&n) = chars.peek() {
                match subscript_digit(n) {
                    Some(d) => {
                        digits.push(d);
                        chars.next();
                    }
                    None => break,
                }
            }
            let trimmed = out.trim_end().len();
            out.truncate(trimmed);
            out.push_str(&format!("_{{{digits}}}"));
        } else if let Some(rep) = unicode_math_replacement(c) {
            out.push_str(rep);
        } else {
            out.push(c);
        }
    }
    collapse_spaces(out.trim())
}

/// Heuristic: `M_eff`, `T_ij`, `G_w`, `rho_c` in prose (single-letter or Greek-name base,
/// short subscript, not part of a path / snake_case identifier).
fn find_bare_identifiers(span: &str) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    let bytes = span.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' && i > 0 && i + 1 < bytes.len() {
            // Base: walk back over alphabetic chars.
            let mut base_start = i;
            while base_start > 0 && bytes[base_start - 1].is_ascii_alphabetic() {
                base_start -= 1;
            }
            let base = &span[base_start..i];
            // Subscript: walk forward over alphanumerics.
            let mut sub_end = i + 1;
            while sub_end < bytes.len() && bytes[sub_end].is_ascii_alphanumeric() {
                sub_end += 1;
            }
            let sub = &span[i + 1..sub_end];
            let before_base = span[..base_start].chars().next_back();
            let after_sub = span[sub_end..].chars().next();
            let base_ok = !base.is_empty() && (base.chars().count() == 1 || greek_command(base).is_some());
            let sub_ok = !sub.is_empty() && sub.chars().count() <= 4;
            let boundary_ok = before_base.map(|c| !is_word_char(c) && c != '\\' && c != '/' && c != '.' && c != '-' && c != '$' && c != '{').unwrap_or(true)
                && after_sub.map(|c| c != '_' && c != '(' && c != '/' && c != '.' || c == '.' && span[sub_end + 1..].chars().next().map(|n| !n.is_ascii_alphabetic()).unwrap_or(true)).unwrap_or(true);
            let looks_like_path = span[..base_start].ends_with('/') || span[sub_end..].starts_with(".md") || span[sub_end..].starts_with(".py");
            let markdown_emphasis = base.is_empty() && sub_end == i + 1;
            if base_ok && sub_ok && boundary_ok && !looks_like_path && !markdown_emphasis {
                results.push((base_start, sub_end));
                i = sub_end;
                continue;
            }
            i = sub_end.max(i + 1);
            continue;
        }
        i += 1;
    }
    results
}

fn greek_word_in_math_context(span: &str, idx: usize, len: usize) -> bool {
    let before = span[..idx].trim_end();
    let after = span[idx + len..].trim_start();
    let math_neighbors = ['=', '+', '*', '^', '_', '<', '>', '~', '≈', '≤', '≥'];
    // `delta-function`, `double-beta`, `Muon/Tau`, `(Electron/Muon/Tau)` are prose, not math.
    let raw_before = span[..idx].chars().next_back();
    let raw_after = span[idx + len..].chars().next();
    if matches!(raw_before, Some('-') | Some('/') | Some('(')) || matches!(raw_after, Some('-') | Some('/') | Some(')')) {
        return false;
    }
    let before_char = before.chars().next_back();
    let after_char = after.chars().next();
    let before_math = before_char.map(|c| math_neighbors.contains(&c) || c.is_ascii_digit()).unwrap_or(false);
    let after_math = after_char.map(|c| math_neighbors.contains(&c) || c.is_ascii_digit()).unwrap_or(false);
    // e.g. "the parameter lambda_0", "sigma = 2.58", "5 sigma"
    let attached_script = span[idx + len..].starts_with('_') || span[idx + len..].starts_with('^');
    before_math || after_math || attached_script
}

/// Detect equation-like runs in prose such as `rho_c = 3*H**2/(8*pi*G)` or `E = m*c**2`.
fn find_word_equations(span: &str) -> Vec<(usize, usize)> {
    let mut results = Vec::new();
    for (line_start, line) in line_offsets(span) {
        // Candidate = maximal run of equation characters.
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !is_equation_char(bytes[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && is_equation_char(bytes[i]) {
                i += 1;
            }
            let mut end = i;
            // Trim surrounding whitespace and trailing punctuation.
            let mut s = start;
            while s < end && (bytes[s] == b' ' || bytes[s] == b',' || bytes[s] == b'.' || bytes[s] == b')') {
                s += 1;
            }
            while end > s && (bytes[end - 1] == b' ' || bytes[end - 1] == b',' || bytes[end - 1] == b'.' || bytes[end - 1] == b':') {
                end -= 1;
            }
            if let Some((cs, ce)) = trim_prose_words(line, s, end) {
                let candidate = &line[cs..ce];
                if candidate.starts_with(|c: char| c.is_ascii_alphanumeric()) && looks_like_typed_equation(candidate) {
                    results.push((line_start + cs, line_start + ce));
                }
            }
        }
    }
    results
}

fn is_prose_word(piece: &str) -> bool {
    let word = piece.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if word.is_empty() || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let lower = word.to_ascii_lowercase();
    if word.len() <= 2 {
        return matches!(lower.as_str(), "is" | "in" | "of" | "to" | "at" | "as" | "by" | "on" | "or" | "an" | "be" | "we" | "if" | "so" | "it" | "no" | "do" | "up" | "us" | "he" | "my");
    }
    !GREEK_NAMES.contains(&lower.as_str())
        && !FUNCTION_NAMES.contains(&lower.as_str())
        && !matches!(lower.as_str(), "sqrt" | "abs" | "hbar" | "inf")
}

/// Narrow `[start, end)` to the longest run of whitespace-separated pieces with no
/// English words, so `The density is rho_c = 3*H**2 in these units` → `rho_c = 3*H**2`.
fn trim_prose_words(line: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let segment = &line[start..end];
    let mut pieces: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0;
    for piece in segment.split(' ') {
        if !piece.is_empty() {
            pieces.push((cursor, cursor + piece.len()));
        }
        cursor += piece.len() + 1;
    }
    let mut best: Option<(usize, usize)> = None;
    let mut run_start: Option<usize> = None;
    let close_run = |run_start: &mut Option<usize>, idx: usize, best: &mut Option<(usize, usize)>| {
        if let Some(rs) = run_start.take() {
            if idx > rs {
                let (bs, _) = pieces[rs];
                let (_, be) = pieces[idx - 1];
                let text = &segment[bs..be];
                if text.contains('=') && best.map(|(a, b)| be - bs > b - a).unwrap_or(true) {
                    *best = Some((bs, be));
                }
            }
        }
    };
    for (idx, (ps, pe)) in pieces.iter().enumerate() {
        if is_prose_word(&segment[*ps..*pe]) {
            close_run(&mut run_start, idx, &mut best);
        } else if run_start.is_none() {
            run_start = Some(idx);
        }
    }
    close_run(&mut run_start, pieces.len(), &mut best);
    best.map(|(bs, be)| {
        let mut be = be;
        while be > bs && matches!(segment.as_bytes()[be - 1], b',' | b'.' | b':' | b';') {
            be -= 1;
        }
        (start + bs, start + be)
    })
}

fn line_offsets(span: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in span.char_indices() {
        if c == '\n' {
            out.push((start, &span[start..i]));
            start = i + 1;
        }
    }
    out.push((start, &span[start..]));
    out
}

fn is_equation_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'^' | b'*' | b'+' | b'-' | b'/' | b'=' | b'(' | b')' | b'.' | b',' | b'<' | b'>' | b'~' | b'!' | b'\\' | b'{' | b'}')
}

fn looks_like_typed_equation(candidate: &str) -> bool {
    let has_relation = candidate.contains('=') || candidate.contains("<=") || candidate.contains(">=") || candidate.contains("->");
    let strong_signals = candidate.matches("**").count()
        + candidate.matches("sqrt(").count()
        + candidate.matches('^').count()
        + candidate.matches('*').count().min(3)
        + candidate.matches('/').count().min(2);
    if !has_relation || strong_signals == 0 {
        return false;
    }
    if candidate.contains("\\") || candidate.contains("http") || candidate.contains("  ") || candidate.contains("** ") {
        return false;
    }
    // Parentheses must balance and never close before opening.
    let mut depth: i32 = 0;
    for c in candidate.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return false;
    }
    let words: Vec<&str> = candidate.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').filter(|w| !w.is_empty()).collect();
    if words.len() < 2 {
        return false;
    }
    // Every alphabetic word must be a short identifier, Greek name, function name, or ident_sub.
    let prose_words = words
        .iter()
        .filter(|w| w.chars().all(|c| c.is_ascii_alphabetic()))
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            w.len() > 2 && !GREEK_NAMES.contains(&lower.as_str()) && !FUNCTION_NAMES.contains(&lower.as_str()) && !matches!(lower.as_str(), "sqrt" | "abs" | "hbar" | "inf")
        })
        .count();
    prose_words == 0 && candidate.chars().filter(|c| *c == '=').count() <= 2 && candidate.len() >= 5
}

// ---------------------------------------------------------------------------
// File-level entry points
// ---------------------------------------------------------------------------

pub fn scan_text(text: &str, file: &str, relative_path: &str) -> Vec<MathFinding> {
    let mut builder = FindingBuilder { text, file: file.to_string(), relative_path: relative_path.to_string(), findings: Vec::new() };
    for segment in segment_markdown(text) {
        match segment.kind {
            SegmentKind::Code => {}
            SegmentKind::Math => {
                let inner = &text[segment.inner_start..segment.inner_end];
                let whole = &text[segment.start..segment.end];
                scan_math_span(&mut builder, inner, segment.inner_start, whole, segment.start);
            }
            SegmentKind::Prose => {
                let span = &text[segment.start..segment.end];
                scan_prose_span(&mut builder, span, segment.start);
            }
        }
    }
    // Detect stray unmatched `$$` or `\[` left after segmentation.
    let unmatched_display = text.matches("$$").count() % 2 == 1;
    if unmatched_display {
        if let Some(pos) = text.rfind("$$") {
            builder.push(pos, "$$", "malformed", "manual", "Unmatched `$$` display-math delimiter.".to_string(), vec![]);
        }
    }
    let open_brackets = text.matches("\\[").count();
    let close_brackets = text.matches("\\]").count();
    if open_brackets != close_brackets {
        let needle = if open_brackets > close_brackets { "\\[" } else { "\\]" };
        if let Some(pos) = text.rfind(needle) {
            builder.push(pos, needle, "malformed", "manual", "Unmatched `\\[ … \\]` display-math delimiter.".to_string(), vec![]);
        }
    }
    let mut findings = builder.findings;
    findings.sort_by(|a, b| a.offset.cmp(&b.offset).then(b.original.len().cmp(&a.original.len())));
    findings.dedup_by(|a, b| a.offset == b.offset && a.original == b.original && a.rule == b.rule);
    // Drop findings nested inside a wider finding (e.g. `theta` inside `sin(theta)`),
    // except manual diagnostics on the whole span which carry no replacement.
    let mut kept: Vec<MathFinding> = Vec::with_capacity(findings.len());
    for finding in findings {
        let end = finding.offset + finding.original.len();
        let covered = kept.iter().any(|k| {
            k.severity != "manual" && k.offset <= finding.offset && k.offset + k.original.len() >= end && !(k.offset == finding.offset && k.original.len() == finding.original.len())
        });
        if !covered {
            kept.push(finding);
        }
    }
    kept
}

pub fn collect_markdown_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().map(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown")).unwrap_or(false) {
                out.push(path);
            }
        }
    }
    walk(root, &mut out);
    out
}

pub fn scan_paths(root: &Path, paths: &[PathBuf]) -> MathScanReport {
    let mut findings = Vec::new();
    let mut files = Vec::new();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(path) else { continue };
        let file = path.to_string_lossy().to_string();
        let relative = path.strip_prefix(root).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| file.clone());
        let file_findings = scan_text(&text, &file, &relative);
        for finding in &file_findings {
            *counts.entry(finding.rule.clone()).or_insert(0) += 1;
        }
        files.push(MathScanFileSummary { file: file.clone(), relative_path: relative, findings: file_findings.len() });
        findings.extend(file_findings);
    }
    for (id, finding) in findings.iter_mut().enumerate() {
        finding.id = id + 1;
    }
    MathScanReport {
        root: root.to_string_lossy().to_string(),
        files_scanned: paths.len(),
        files,
        findings,
        rule_counts: counts.into_iter().collect(),
    }
}

/// Apply the selected replacements. Each file is rewritten once; a `.bak` copy is
/// written beside it before the first change when `backup` is true.
pub fn apply_fixes(selections: &[MathFixSelection], backup: bool) -> Result<MathFixApplyReport, String> {
    let mut by_file: std::collections::BTreeMap<String, Vec<&MathFixSelection>> = std::collections::BTreeMap::new();
    for selection in selections {
        by_file.entry(selection.file.clone()).or_default().push(selection);
    }
    let mut report = MathFixApplyReport { files_changed: 0, fixes_applied: 0, skipped: Vec::new(), backups: Vec::new() };
    for (file, mut fixes) in by_file {
        let path = Path::new(&file);
        let mut text = fs::read_to_string(path).map_err(|e| format!("Failed to read {file}: {e}"))?;
        fixes.sort_by(|a, b| b.offset.cmp(&a.offset));
        let mut applied_here = 0usize;
        let mut last_start = usize::MAX;
        for fix in fixes {
            let end = fix.offset + fix.original.len();
            if end > text.len() || &text[fix.offset..end] != fix.original.as_str() {
                report.skipped.push(format!("{file}@{}: source text changed since scan", fix.offset));
                continue;
            }
            if end > last_start {
                report.skipped.push(format!("{file}@{}: overlaps another selected fix", fix.offset));
                continue;
            }
            text.replace_range(fix.offset..end, &fix.replacement);
            last_start = fix.offset;
            applied_here += 1;
        }
        if applied_here == 0 {
            continue;
        }
        if backup {
            let backup_path = path.with_extension(format!(
                "{}.bak",
                path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "md".to_string())
            ));
            fs::copy(path, &backup_path).map_err(|e| format!("Failed to write backup {}: {e}", backup_path.display()))?;
            report.backups.push(backup_path.to_string_lossy().to_string());
        }
        fs::write(path, text).map_err(|e| format!("Failed to write {file}: {e}"))?;
        report.files_changed += 1;
        report.fixes_applied += applied_here;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(findings: &[MathFinding]) -> Vec<(&str, &str, &str)> {
        findings.iter().map(|f| (f.rule.as_str(), f.original.as_str(), f.options.first().map(|s| s.as_str()).unwrap_or(""))).collect()
    }

    #[test]
    fn unicode_in_math_is_mapped_to_latex() {
        let text = "Energy: $E ≈ m × c²$ and $ρ − Λ$";
        let f = scan_text(text, "f.md", "f.md");
        let r = rules(&f);
        assert!(r.contains(&("unicode_math", "≈", "\\approx")));
        assert!(r.contains(&("unicode_math", "×", "\\times")));
        assert!(r.contains(&("unicode_math", "²", "^{2}")));
        assert!(r.contains(&("unicode_math", "ρ", "\\rho")));
        assert!(r.contains(&("unicode_math", "−", "-")));
        assert!(r.contains(&("unicode_math", "Λ", "\\Lambda")));
        assert!(f.iter().all(|x| x.severity == "auto"));
    }

    #[test]
    fn prose_symbols_are_wrapped_in_math() {
        let text = "The contrast is 400× larger and reaches 5.557σ for ΛCDM at 10⁻¹⁵ m.";
        let f = scan_text(text, "f.md", "f.md");
        let r = rules(&f);
        assert!(r.contains(&("unicode_prose", "400×", "$400\\times$")));
        assert!(r.contains(&("unicode_prose", "5.557σ", "$5.557\\sigma$")));
        assert!(r.contains(&("unicode_prose", "Λ", "$\\Lambda$")));
        assert!(r.contains(&("unicode_prose", "10⁻¹⁵", "$10^{-15}$")));
    }

    #[test]
    fn dashes_and_quotes_in_prose_are_left_alone() {
        let text = "A well–known result — “quoted” — with an em dash.";
        let f = scan_text(text, "f.md", "f.md");
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn code_blocks_and_inline_code_are_ignored() {
        let text = "```python\nx = 3*H**2/(8*pi*G)\nσ = 1\n```\nUse `M_eff` here.\n";
        let f = scan_text(text, "f.md", "f.md");
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn greek_names_offer_case_choice() {
        let text = "$lambda = 2 sigma$ and Lambda = 3 in prose";
        let f = scan_text(text, "f.md", "f.md");
        let lambda = f.iter().find(|x| x.original == "lambda").unwrap();
        assert_eq!(lambda.rule, "greek_name");
        assert_eq!(lambda.options, vec!["\\lambda", "\\Lambda"]);
        let sigma = f.iter().find(|x| x.original == "sigma").unwrap();
        assert_eq!(sigma.options, vec!["\\sigma", "\\Sigma"]);
        // Capitalised name in prose defaults to uppercase and is wrapped in $.
        let cap = f.iter().find(|x| x.original == "Lambda" && x.rule == "greek_name").unwrap();
        assert_eq!(cap.options, vec!["$\\Lambda$", "$\\lambda$"]);
    }

    #[test]
    fn english_greek_collisions_in_prose_need_math_context() {
        let text = "The beta release of the alpha channel shipped.";
        let f = scan_text(text, "f.md", "f.md");
        assert!(f.iter().all(|x| x.rule != "greek_name"), "{f:?}");
        let text = "We fit sigma = 2.58 and mu here.";
        let f = scan_text(text, "f.md", "f.md");
        assert!(f.iter().any(|x| x.rule == "greek_name" && x.original == "sigma"));
        assert!(f.iter().any(|x| x.rule == "greek_name" && x.original == "mu"));
    }

    #[test]
    fn shorthand_inside_math_is_normalised() {
        let text = "$x**2 + y_10 >= sqrt(a+b) - 1e-5 + sin(theta) + M_eff$";
        let f = scan_text(text, "f.md", "f.md");
        let r = rules(&f);
        assert!(r.contains(&("shorthand", "**2", "^{2}")));
        assert!(r.contains(&("shorthand", "_10", "_{10}")));
        assert!(r.contains(&("shorthand", ">=", "\\geq")));
        assert!(!f.iter().any(|x| x.original == "theta"), "nested greek finding should be absorbed by sin(theta)");
        assert!(r.contains(&("shorthand", "sqrt(a+b)", "\\sqrt{a + b}")));
        assert!(r.contains(&("shorthand", "1e-5", "10^{-5}")));
        assert!(r.contains(&("shorthand", "sin(theta)", "\\sin\\left(\\theta\\right)")));
        let eff = f.iter().find(|x| x.original == "_eff").unwrap();
        assert_eq!(eff.options, vec!["_{\\text{eff}}", "_{eff}"]);
        assert_eq!(eff.severity, "review");
    }

    #[test]
    fn dimension_shorthand_and_existing_latex_are_handled() {
        let text = "$3x3$ and $\\frac{a}{b} \\geq x^{-1}$ with $x_1^2$";
        let f = scan_text(text, "f.md", "f.md");
        let r = rules(&f);
        assert_eq!(r, vec![("shorthand", "x", " \\times ")]);
    }

    #[test]
    fn bare_identifiers_in_prose_are_flagged_with_options() {
        let text = "The effective mass M_eff and tensor T_ij and coupling G_w, but not snake_case_name or file_name.md.";
        let f = scan_text(text, "f.md", "f.md");
        let bare: Vec<&MathFinding> = f.iter().filter(|x| x.rule == "bare_identifier").collect();
        let originals: Vec<&str> = bare.iter().map(|x| x.original.as_str()).collect();
        assert_eq!(originals, vec!["M_eff", "T_ij", "G_w"]);
        assert_eq!(bare[0].options, vec!["$M_{\\text{eff}}$", "$M_{eff}$"]);
        assert_eq!(bare[2].options, vec!["$G_w$"]);
    }

    #[test]
    fn typed_equations_in_prose_become_latex() {
        let text = "The critical density is rho_c = 3*H**2/(8*pi*G) in these units.";
        let f = scan_text(text, "f.md", "f.md");
        let eq = f.iter().find(|x| x.rule == "word_equation").expect("word equation");
        assert_eq!(eq.original, "rho_c = 3*H**2/(8*pi*G)");
        assert_eq!(eq.options[0], "$\\rho_{c} = \\frac{3 H^{2}}{8 \\pi G}$");
    }

    #[test]
    fn shorthand_converter_examples() {
        assert_eq!(shorthand_to_latex("E = m*c**2"), "E = m c^{2}");
        assert_eq!(shorthand_to_latex("2*3"), "2 \\cdot 3");
        assert_eq!(shorthand_to_latex("a/b"), "\\frac{a}{b}");
        assert_eq!(shorthand_to_latex("(a+b)/(c-d)"), "\\frac{a + b}{c - d}");
        assert_eq!(shorthand_to_latex("3e8"), "3 \\times 10^{8}");
        assert_eq!(shorthand_to_latex("x != y -> z"), "x \\neq y \\to z");
        assert_eq!(shorthand_to_latex("exp(-r/l_scale)"), "\\exp\\left(-\\frac{r}{l_{\\text{scale}}}\\right)");
        assert_eq!(shorthand_to_latex("Delta_chi**2 <= 6.5"), "\\Delta_{\\chi}^{2} \\leq 6.5");
        assert_eq!(shorthand_to_latex("Omega_Lambda"), "\\Omega_{\\Lambda}");
        assert_eq!(shorthand_to_latex("H0 = 67.4"), "H_{0} = 67.4");
        assert_eq!(
            shorthand_to_latex("M_chirp = (c^3 / G) * (5/96 * pi^-8/3 * f^-11/3 * df/dt)^3/5"),
            "M_{\\text{chirp}} = (\\frac{c^{3}}{G}) (\\frac{5}{96} \\pi^{-8/3} f^{-11/3} \\frac{df}{dt})^{3/5}"
        );
        assert_eq!(shorthand_to_latex("V_gap(n) = 1 / sqrt(1+n)"), "V_{\\text{gap}}(n) = \\frac{1}{\\sqrt{1 + n}}");
        let f = scan_text("an area of 3 m² and T₂/T₁ < 0.45", "f.md", "f.md");
        let originals: Vec<&str> = f.iter().map(|x| x.original.as_str()).collect();
        assert!(originals.contains(&"m²"), "{originals:?}");
        assert!(originals.contains(&"T₂"), "{originals:?}");
        assert_eq!(f.iter().find(|x| x.original == "m²").unwrap().options[0], "$m^{2}$");
    }

    #[test]
    fn malformed_math_is_reported() {
        let text = "Broken $\\frac{a}{b$ here and $$ $$ empty.";
        let f = scan_text(text, "f.md", "f.md");
        assert!(f.iter().any(|x| x.rule == "malformed" && x.message.contains("Unbalanced braces")));
        assert!(f.iter().any(|x| x.rule == "malformed" && x.message.contains("Empty")));
    }

    #[test]
    fn emoji_in_prose_is_flagged_manual() {
        let text = "| status | ⚠️ pipeline-sensitive |";
        let f = scan_text(text, "f.md", "f.md");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule, "unsupported_symbol");
        assert_eq!(f[0].severity, "manual");
    }

    #[test]
    fn apply_fixes_rewrites_files_and_keeps_backup() {
        let dir = std::env::temp_dir().join(format!("math_fixer_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("doc.md");
        fs::write(&file, "Value 400× and $a ≈ b$\n").unwrap();
        let report = scan_paths(&dir, &[file.clone()]);
        assert_eq!(report.findings.len(), 2);
        let selections: Vec<MathFixSelection> = report
            .findings
            .iter()
            .map(|f| MathFixSelection { file: f.file.clone(), offset: f.offset, original: f.original.clone(), replacement: f.options[0].clone() })
            .collect();
        let applied = apply_fixes(&selections, true).unwrap();
        assert_eq!(applied.fixes_applied, 2);
        assert_eq!(applied.files_changed, 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), "Value $400\\times$ and $a \\approx b$\n");
        assert!(dir.join("doc.md.bak").exists());
        // Stale selection is skipped, not applied blindly.
        let stale = apply_fixes(&selections, false).unwrap();
        assert_eq!(stale.fixes_applied, 0);
        assert_eq!(stale.skipped.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn currency_and_escaped_dollars_are_not_math() {
        let text = "It costs $5 and $10 total — 400× more.";
        let f = scan_text(text, "f.md", "f.md");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule, "unicode_prose");
    }
}
