use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Deserialize, Default)]
struct GeminiApiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize, Default)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize, Default)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize, Default)]
struct GeminiPart {
    text: String,
}

#[derive(Deserialize, Default)]
struct GeminiModelListResponse {
    #[serde(default)]
    models: Vec<GeminiModelInfo>,
}

#[derive(Deserialize, Default)]
struct GeminiModelInfo {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "supportedGenerationMethods")]
    supported_generation_methods: Vec<String>,
}

#[derive(Deserialize, Default)]
struct OpenAiApiErrorResponse {
    #[serde(default)]
    error: OpenAiApiError,
}

#[derive(Deserialize, Default)]
struct OpenAiApiError {
    #[serde(default)]
    message: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    code: String,
}

// --- RUNTIME MEMORY STATE ---
pub struct AppState {
    pub workspace_path: std::sync::Mutex<String>,
}

#[derive(Debug, Deserialize, Default)]
struct LaunchFileEditorPayload {
    #[serde(default, alias = "filePath")]
    file_path: String,
    #[serde(default, alias = "terminalApp")]
    terminal_app: String,
    #[serde(default, alias = "editor")]
    editor: String,
}

fn build_file_editor_command(payload: &LaunchFileEditorPayload) -> Result<(String, Vec<String>), String> {
    let file_path = payload.file_path.trim();
    if file_path.is_empty() {
        return Err("No file path provided".to_string());
    }

    if !payload.editor.trim().is_empty() {
        let terminal_app = if payload.terminal_app.trim().is_empty() {
            "x-terminal-emulator"
        } else {
            payload.terminal_app.trim()
        };

        let editor = payload.editor.trim();
        let mut args = Vec::new();
        if terminal_app.contains("gnome-terminal") {
            args.push("--".to_string());
        } else {
            args.push("-e".to_string());
        }
        args.push(editor.to_string());
        args.push(file_path.to_string());
        return Ok((terminal_app.to_string(), args));
    }

    Ok(("xdg-open".to_string(), vec![file_path.to_string()]))
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CosmologyMetrics {
    pub scale_factor: f64,
    pub e_of_z: f64,
    pub omega_k: f64,
    pub comoving_distance_mpc: f64,
    pub luminosity_distance_mpc: f64,
}

fn compute_cosmology_metrics(h0: f64, omega_m: f64, omega_l: f64, omega_r: f64, z: f64) -> Result<CosmologyMetrics, String> {
    if z < 0.0 {
        return Err("Redshift must be non-negative".to_string());
    }

    let omega_k = 1.0 - omega_m - omega_l - omega_r;
    let scale_factor = 1.0 / (1.0 + z);
    let e_of_z = (omega_r * (1.0 + z).powi(4) + omega_m * (1.0 + z).powi(3) + omega_k * (1.0 + z).powi(2) + omega_l).sqrt();

    let hubble_distance = 2997.92458 / h0.max(1e-9);
    let mut integrand = 0.0;
    let steps = 2000usize;
    let dz = z as f64 / steps as f64;
    for i in 0..steps {
        let z_i = (i as f64 + 0.5) * dz;
        let one_plus_z = 1.0 + z_i;
        let integrand_value = 1.0 / ((omega_r * one_plus_z.powi(4) + omega_m * one_plus_z.powi(3) + omega_k * one_plus_z.powi(2) + omega_l).sqrt() * one_plus_z);
        integrand += integrand_value;
    }
    let comoving_distance_mpc = hubble_distance * dz * integrand;
    let luminosity_distance_mpc = (1.0 + z) * comoving_distance_mpc;

    Ok(CosmologyMetrics {
        scale_factor,
        e_of_z,
        omega_k,
        comoving_distance_mpc,
        luminosity_distance_mpc,
    })
}

fn default_true() -> bool {
    true
}

// --- PERSISTENT DISK CONFIGURATION ---
#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    last_root_dir: String,
    editor: String,
    terminal_app: String,
    gemini_api_key: String,
    openai_api_key: String,
    left_provider: String,
    left_model: String,
    right_provider: String,
    right_model: String,
    project_root_dir: String,
    theory_md_dir: String,
    master_axiom_file: String,
    tools_dir: String,
    theme: String,           // <-- NEW
    custom_accent: String,   // <-- NEW
    custom_bg_panel: String, // <-- NEW
    #[serde(default = "default_true")]
    left_preserve_thread_history: bool,
    #[serde(default = "default_true")]
    right_preserve_thread_history: bool,
    reuse_notes_next_session: bool,
    first_session_completed: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_root_dir: String::new(),
            editor: String::new(),
            terminal_app: String::new(),
            gemini_api_key: String::new(),
            openai_api_key: String::new(),
            left_provider: "openai".to_string(),
            left_model: "gpt-4.1".to_string(),
            right_provider: "openai".to_string(),
            right_model: "gpt-4.1-mini".to_string(),
            project_root_dir: String::new(),
            theory_md_dir: String::new(),
            master_axiom_file: String::new(),
            tools_dir: String::new(),
            theme: "dark".to_string(),
            custom_accent: String::new(),
            custom_bg_panel: String::new(),
            left_preserve_thread_history: true,
            right_preserve_thread_history: true,
            reuse_notes_next_session: false,
            first_session_completed: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ExitSessionOptions {
    pub create_new_axiom: bool,
    pub create_recap_md: bool,
    pub reuse_existing_notes_next_session: bool,
    pub scratchpad_content: String,
    pub workspace_path: String,
    pub recap_content_override: String,
    pub notes_content_override: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct SaveUserSettingsPayload {
    pub editor: String,
    #[serde(alias = "terminalApp")]
    pub terminal_app: String,
    #[serde(alias = "geminiKey")]
    pub gemini_key: String,
    #[serde(alias = "openaiKey")]
    pub openai_key: String,
    #[serde(alias = "leftProvider")]
    pub left_provider: String,
    #[serde(alias = "leftModel")]
    pub left_model: String,
    #[serde(alias = "rightProvider")]
    pub right_provider: String,
    #[serde(alias = "rightModel")]
    pub right_model: String,
    #[serde(alias = "projectRootDir")]
    pub project_root_dir: String,
    #[serde(alias = "theoryMdDir")]
    pub theory_md_dir: String,
    #[serde(alias = "masterAxiomFile")]
    pub master_axiom_file: String,
    #[serde(default, alias = "toolsDir")]
    pub tools_dir: String,
    pub theme: String,
    #[serde(alias = "customAccent")]
    pub custom_accent: String,
    #[serde(alias = "customBgPanel")]
    pub custom_bg_panel: String,
    #[serde(default = "default_true", alias = "leftPreserveThreadHistory")]
    pub left_preserve_thread_history: bool,
    #[serde(default = "default_true", alias = "rightPreserveThreadHistory")]
    pub right_preserve_thread_history: bool,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct EmpiricalAnalysisRequest {
    pub dataset_path: String,
    pub instrument: String,
    pub observation_method: String,
    pub hypothesis: String,
    pub target_variable: String,
    pub workspace_path: String,
    pub primer_mode: String,
}

#[derive(Serialize)]
pub struct FileEntry {
    name: String,
    path: String, // Add the path field here
    is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct ProbeEvidenceItem {
    relative_path: String,
    score: i32,
    snippets: Vec<String>,
}

// Helper: Resolve the OS-specific config path
fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
        
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    
    path.push("config.json");
    Ok(path)
}

fn default_home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

fn apply_unset_path_defaults(config: &mut AppConfig) {
    let Some(home_dir) = default_home_dir() else {
        return;
    };

    if config.last_root_dir.trim().is_empty() {
        config.last_root_dir = home_dir.clone();
    }

    if config.project_root_dir.trim().is_empty() {
        config.project_root_dir = home_dir;
    }

    if config.theory_md_dir.trim().is_empty() {
        config.theory_md_dir = config.project_root_dir.clone();
    }
}

fn build_workspace_tree_string(root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("Root path does not exist: {}", root.to_string_lossy()));
    }

    fn build_tree(dir: &Path, prefix: &str, output: &mut String) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        for (i, entry) in entries.iter().enumerate() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            if name_str == "target" || name_str.starts_with('.') {
                continue;
            }

            let is_last = i == entries.len() - 1;
            let pointer = if is_last { "└── " } else { "├── " };

            output.push_str(&format!("{}{}{}\n", prefix, pointer, name_str));

            if entry.file_type()?.is_dir() {
                let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                build_tree(&entry.path(), &new_prefix, output)?;
            }
        }

        Ok(())
    }

    let root_label = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    let mut tree_string = format!("{}\n", root_label);
    build_tree(root, "", &mut tree_string).map_err(|e| e.to_string())?;
    Ok(tree_string)
}

fn resolve_workspace_root(current_path: &str, config: &AppConfig) -> Option<PathBuf> {
    let candidates = [
        current_path.trim(),
        config.project_root_dir.trim(),
        config.last_root_dir.trim(),
    ];

    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }

        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn read_source_payload(source_name: &str, path: &Path, max_chars: usize) -> (serde_json::Value, Option<String>) {
    if !path.exists() {
        return (
            serde_json::json!({
                "name": source_name,
                "path": path.to_string_lossy().to_string(),
                "exists": false,
                "content": "",
                "bytes": 0,
                "lines": 0,
                "truncated": false
            }),
            Some(format!("Missing source: {} ({})", source_name, path.to_string_lossy())),
        );
    }

    match fs::read_to_string(path) {
        Ok(raw) => {
            let full_char_count = raw.chars().count();
            let truncated = full_char_count > max_chars;
            let content = if truncated {
                let head: String = raw.chars().take(max_chars).collect();
                format!(
                    "{}\n\n[...truncated {} chars from {} source for transport safety...]",
                    head,
                    full_char_count.saturating_sub(max_chars),
                    source_name
                )
            } else {
                raw
            };

            (
                serde_json::json!({
                    "name": source_name,
                    "path": path.to_string_lossy().to_string(),
                    "exists": true,
                    "content": content,
                    "bytes": fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                    "lines": content.lines().count(),
                    "truncated": truncated
                }),
                None,
            )
        }
        Err(e) => (
            serde_json::json!({
                "name": source_name,
                "path": path.to_string_lossy().to_string(),
                "exists": false,
                "content": "",
                "bytes": 0,
                "lines": 0,
                "truncated": false
            }),
            Some(format!("Unreadable source: {} ({})", source_name, e)),
        ),
    }
}

fn collect_topic_index(theory_dir: &str) -> Vec<String> {
    let path = Path::new(theory_dir);
    if !path.exists() || !path.is_dir() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut files = Vec::new();
    let _ = recursive_markdown_scan(path, &mut files);

    for file_path in files {
        if let Ok(content) = fs::read_to_string(&file_path) {
            let relative = file_path.strip_prefix(path).unwrap_or(&file_path).to_string_lossy().to_string();
            let mut headings = Vec::new();
            let mut summary = String::new();
            let mut seen_non_heading = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let heading_level = trimmed.chars().take_while(|c| *c == '#').count();
                if heading_level > 0 && trimmed.len() > heading_level {
                    let title = trimmed[heading_level..].trim().to_string();
                    if !title.is_empty() {
                        headings.push((heading_level, title));
                    }
                } else if !seen_non_heading {
                    summary = trimmed.to_string();
                    seen_non_heading = true;
                }
            }

            if headings.is_empty() {
                if !summary.is_empty() {
                    entries.push(format!("- {}: {}", relative, summary));
                }
                continue;
            }

            let mut formatted = format!("- {}", relative);
            for (level, title) in headings.iter().take(4) {
                let prefix = "  ".repeat(*level as usize);
                formatted.push_str(&format!("\n{}- {}", prefix, title));
            }
            if !summary.is_empty() {
                formatted.push_str(&format!("\n  - Summary: {}", summary));
            }
            entries.push(formatted);
        }
    }

    entries.sort();
    entries
}

fn collect_tool_inventory(project_root: &Path, tools_dir: &str) -> Vec<String> {
    let mut inventory = Vec::new();
    let tools_path = if tools_dir.trim().is_empty() {
        project_root.to_path_buf()
    } else {
        PathBuf::from(tools_dir)
    };

    if !tools_path.exists() {
        return inventory;
    }

    let mut candidates = Vec::new();
    let _ = recursive_file_scan(&tools_path, &mut candidates);
    for path in candidates {
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
        if !matches!(extension.as_str(), "py" | "sh" | "ipynb" | "m" | "jl") {
            continue;
        }
        let relative = path.strip_prefix(project_root).unwrap_or(&path).to_string_lossy().to_string();
        let mut summary = String::new();
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                    continue;
                }
                summary = trimmed.to_string();
                break;
            }
        }
        if summary.is_empty() {
            summary = "No obvious purpose header found; inspect the file directly.".to_string();
        }
        inventory.push(format!("- {}: {}", relative, summary));
    }

    inventory.sort();
    inventory
}

fn tokenize_keywords(text: &str) -> Vec<String> {
    let stop_words = [
        "the", "and", "for", "with", "that", "this", "into", "from", "then", "than", "have", "has", "been",
        "were", "was", "are", "is", "it", "in", "on", "of", "to", "a", "an", "or", "as", "by", "be", "our",
        "your", "their", "will", "can", "could", "should", "may", "about", "using", "used", "chapter", "section",
        "project", "file", "tool", "data", "analysis", "theory", "model",
    ];
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            let word = current.trim();
            if word.len() >= 3 && !stop_words.contains(&word) {
                tokens.push(word.to_string());
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        let word = current.trim();
        if word.len() >= 3 && !stop_words.contains(&word) {
            tokens.push(word.to_string());
        }
    }
    tokens
}

fn collect_keyword_tokens(path: &Path, content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
        tokens.extend(tokenize_keywords(stem));
    }

    let mut saw_body = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            let title = trimmed.trim_start_matches('#').trim();
            if !title.is_empty() {
                tokens.extend(tokenize_keywords(title));
            }
            continue;
        }
        if !saw_body {
            tokens.extend(tokenize_keywords(trimmed));
            saw_body = true;
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
}

fn extract_evidence_snippet(content: &str, query_terms: &[String], max_chars: usize) -> String {
    let lower_content = content.to_lowercase();
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    for term in query_terms {
        let needle = term.to_lowercase();
        if needle.is_empty() {
            continue;
        }
        if let Some(index) = lower_content.find(&needle) {
            let start = index.saturating_sub(80);
            let end = (index + needle.len() + 80).min(content.len());
            let length = end.saturating_sub(start);
            if length > best_len {
                best_start = start;
                best_len = length;
            }
        }
    }

    let effective_start = best_start.min(content.chars().count());
    let effective_end = (best_start + best_len).min(content.chars().count());
    let mut snippet = if best_len == 0 {
        let excerpt = content.lines().find(|line| !line.trim().is_empty()).unwrap_or_default();
        excerpt.trim().to_string()
    } else {
        content.chars().skip(effective_start).take(effective_end - effective_start).collect::<String>()
    };

    snippet = snippet.trim().to_string();
    if snippet.chars().count() > max_chars {
        let mut chars = snippet.chars();
        snippet = chars.by_ref().take(max_chars).collect();
    }
    snippet
}

fn build_document_tool_links(project_root: &Path, theory_dir: &str, tools_dir: &str) -> Vec<String> {
    let theory_path = Path::new(theory_dir);
    let tools_path = if tools_dir.trim().is_empty() {
        project_root.to_path_buf()
    } else {
        PathBuf::from(tools_dir)
    };

    let mut theory_files = Vec::new();
    let _ = recursive_markdown_scan(theory_path, &mut theory_files);

    let mut tool_files = Vec::new();
    if tools_path.exists() {
        let _ = recursive_file_scan(&tools_path, &mut tool_files);
    }

    let mut docs = Vec::new();
    for path in theory_files {
        if let Ok(content) = fs::read_to_string(&path) {
            let relative = path.strip_prefix(project_root).unwrap_or(&path).to_string_lossy().to_string();
            let keywords = collect_keyword_tokens(&path, &content);
            if !keywords.is_empty() {
                docs.push((relative, keywords));
            }
        }
    }

    let mut ranked_matches = Vec::new();
    for (doc_rel, doc_keywords) in docs {
        let mut tool_matches = Vec::new();
        for path in &tool_files {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
            if !matches!(extension.as_str(), "py" | "sh" | "ipynb" | "m" | "jl") {
                continue;
            }
            let tool_rel = path.strip_prefix(project_root).unwrap_or(path).to_string_lossy().to_string();
            if let Ok(content) = fs::read_to_string(path) {
                let tool_keywords = collect_keyword_tokens(path, &content);
                let shared_terms: Vec<String> = doc_keywords
                    .iter()
                    .filter(|token| tool_keywords.contains(*token))
                    .cloned()
                    .collect();
                let filename_score = if tool_rel.to_lowercase().contains(&doc_rel.to_lowercase()) {
                    2
                } else {
                    0
                };
                let overlap_score = shared_terms.len();
                let hint_score = if tool_keywords.iter().any(|token| token == "analyze" || token == "data") {
                    1
                } else {
                    0
                };
                let total_score = filename_score + overlap_score + hint_score;
                if total_score > 0 {
                    let evidence = extract_evidence_snippet(&content, &shared_terms, 140);
                    tool_matches.push((tool_rel, shared_terms, total_score, evidence));
                }
            }
        }
        if !tool_matches.is_empty() {
            tool_matches.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
            let mut ranked_lines = Vec::new();
            for (tool_rel, shared_terms, score, evidence) in tool_matches.iter().take(3) {
                ranked_lines.push(format!(
                    "- {} -> {} (score: {}, shared terms: {}, evidence: {})",
                    doc_rel,
                    tool_rel,
                    score,
                    shared_terms.join(", "),
                    evidence.replace('\n', " ")
                ));
            }
            ranked_matches.push(ranked_lines.join("\n"));
        }
    }

    ranked_matches.sort();
    ranked_matches
}

fn build_project_awareness_markdown(
    project_root: &Path,
    theory_dir: &str,
    master_axiom_path: &Path,
    tools_dir: &str,
    scan: &serde_json::Value,
) -> String {
    let topic_index = collect_topic_index(theory_dir);
    let tool_inventory = collect_tool_inventory(project_root, tools_dir);
    let document_tool_links = build_document_tool_links(project_root, theory_dir, tools_dir);
    let files_scanned = scan["files_scanned"].as_u64().unwrap_or(0);
    let lagrangian_hint = scan["lagrangian_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .unwrap_or("No Lagrangian/action marker detected yet.");
    let hypothesis_hint = scan["hypothesis_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .unwrap_or("No explicit hypothesis marker detected yet.");

    let topic_section = if topic_index.is_empty() {
        "- No theory markdown headings were found yet; the topic index will populate after the theory corpus is imported.".to_string()
    } else {
        topic_index.join("\n")
    };

    let tool_section = if tool_inventory.is_empty() {
        "- No reusable tools were discovered in the configured tools directory yet.".to_string()
    } else {
        tool_inventory.join("\n")
    };

    let document_tool_section = if document_tool_links.is_empty() {
        "- No likely document-tool links were inferred yet; add shared terminology or richer tool headers to improve matching.".to_string()
    } else {
        document_tool_links.join("\n\n")
    };

    format!(
        "# Project Awareness Index\n\n## Theory Topic Map\n{}\n\n## Active Theory Anchors\n- Master axiom: {}\n- Theory files scanned: {}\n- Lagrangian/action cue: {}\n- Hypothesis cue: {}\n\n## Tool Awareness\n{}\n\n## Ranked Retrieval Hints\n{}\n\n## Guidance\n- Use this index to locate relevant theory sections quickly without re-reading the entire manuscript each time.\n- Prefer existing tools and prior analyses before proposing brand-new implementations.\n- When the user asks about a specific topic, retrieve only the relevant node and its linked tool or experiment context.\n- When the user says a chapter used a specific analysis workflow, inspect the likely tool matches first and then verify the relevant document context.\n",
        topic_section,
        master_axiom_path.to_string_lossy(),
        files_scanned,
        lagrangian_hint,
        hypothesis_hint,
        tool_section,
        document_tool_section
    )
}

fn build_session_recap_markdown(
    project_root: &Path,
    theory_dir: &str,
    master_axiom_path: &Path,
    scan: &serde_json::Value,
) -> String {
    let files_scanned = scan["files_scanned"].as_u64().unwrap_or(0);
    let lagrangian_hint = scan["lagrangian_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .unwrap_or("No Lagrangian/action marker detected yet.");
    let hypothesis_hint = scan["hypothesis_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.as_str())
        .unwrap_or("No explicit hypothesis marker detected yet.");

    format!(
        "# Session Recap\n\n## Progress Snapshot\n- Workspace root: {}\n- Theory directory: {}\n- Files scanned in theory corpus: {}\n\n## Theory Anchor\n- Master axiom path: {}\n- Lagrangian/action cue: {}\n- Hypothesis cue: {}\n- Equation continuity marker: keep $\\mathcal{{L}}$ references aligned with the active branch assumptions.\n\n## Next Session Tasks\n1. Confirm today\'s primary research goal before opening new analysis branches.\n2. Identify one testable prediction to stress against incoming data.\n3. Record unresolved assumptions so the next startup primer can stay concise.\n",
        project_root.to_string_lossy(),
        theory_dir,
        files_scanned,
        master_axiom_path.to_string_lossy(),
        lagrangian_hint,
        hypothesis_hint
    )
}

fn build_ai_briefing_markdown(
    project_root: &Path,
    summary: &str,
    primer_path: &Path,
    recap_path: &Path,
    tree_path: &Path,
    master_axiom_path: &Path,
    awareness_markdown: &str,
    thread_context: Option<&str>,
) -> String {
    let axiom_excerpt = fs::read_to_string(master_axiom_path)
        .map(|content| {
            let max_chars = 2400usize;
            let char_count = content.chars().count();
            if char_count > max_chars {
                let head: String = content.chars().take(max_chars).collect();
                format!(
                    "{}\n\n[...truncated {} chars from master axiom snapshot...]",
                    head,
                    char_count.saturating_sub(max_chars)
                )
            } else {
                content
            }
        })
        .unwrap_or_else(|_| "Master axiom file is not readable yet.".to_string());

    let thread_context_block = thread_context.unwrap_or_default().trim();
    let thread_context_section = if thread_context_block.is_empty() {
        "- No thread-specific retrieval hints were supplied yet.".to_string()
    } else {
        format!("- Thread focus: {}", thread_context_block)
    };

    format!(
        "# AI Briefing Packet\n\n## Session Summary\n{}\n\n## Sources\n- Primer: {}\n- Session recap: {}\n- Workspace tree: {}\n- Master axiom: {}\n\n## Master Axiom Snapshot\n```md\n{}\n```\n\n## Startup Guidance\n- Use this packet to resume collaboration without replaying full history.\n- Anchor reasoning in the active axioms and assumptions before proposing new branches.\n- Keep responses concise, physically grounded, and explicit about uncertainty.\n- When the user references a chapter, experiment, or tool, use the project awareness index and the thread focus to locate the most relevant files before answering.\n\n## Project Awareness Index\n```md\n{}\n```\n\n## Thread Retrieval Hints\n{}\n\n## Context Notes\n- Workspace root: {}\n- Equation continuity key: $\\mathcal{{L}}$, boundary constraints, and observational consequences should remain traceable across branch updates.\n",
        summary,
        primer_path.to_string_lossy(),
        recap_path.to_string_lossy(),
        tree_path.to_string_lossy(),
        master_axiom_path.to_string_lossy(),
        axiom_excerpt,
        awareness_markdown,
        thread_context_section,
        project_root.to_string_lossy()
    )
}

fn build_first_session_briefing_markdown(project_root: &Path) -> String {
    format!(
        "# First Session Briefing Packet\n\n## Welcome\n- Greet the user and explain that this first run will establish the project context for future sessions.\n- Ask for the theory/model title so the session language remains aligned with the user\'s framework.\n\n## What the Primer Is\n- In this app, a \"primer\" and the \"briefing packet\" are the same practical concept: a compact context document for AI lanes.\n- The entity being briefed is the AI.\n- Purpose: keep AI aware of your current project state, assumptions, goals, and recent progress without replaying full chat history every time.\n- If you maintain this packet well, continuity stays strong across long sessions and across days.\n\n## Setup Checklist\n1. Import or open the project workspace folder.\n2. Confirm the theory markdown output directory in settings.\n3. Set or generate the master axiom file path.\n4. Save starter notes describing today\'s goals in the scratchpad.\n5. Export the workspace tree so source structure is visible.\n6. Build or refresh the briefing packet and verify both AI lanes received it.\n\n## Assistant Behavior\n- Offer step-by-step guidance instead of waiting idle.\n- Keep prompts concise and practical for first-session setup.\n- Ask one clarifying question at a time when configuration details are missing.\n- Explain setup terms briefly when needed (for example: primer, master axiom, theory markdown folder).\n- Remind the user that end-of-session recap can produce the next briefing packet automatically.\n\n## Expected Outcome\n- By the end of this first session, documentation should be strong enough to replace this starter packet with a session-specific packet.\n\n## Workspace Context\n- Project root: {}\n- Note: this starter packet is intended for first-run onboarding only.\n",
        project_root.to_string_lossy()
    )
}

fn resolve_startup_guide_path(project_root: &Path, theory_dir: &str) -> PathBuf {
    let preferred_dir = theory_dir.trim();
    let mut target_dir = if preferred_dir.is_empty() {
        project_root.to_path_buf()
    } else {
        PathBuf::from(preferred_dir)
    };

    if fs::create_dir_all(&target_dir).is_err() {
        target_dir = project_root.to_path_buf();
        let _ = fs::create_dir_all(&target_dir);
    }

    target_dir.join("first_session_startup_guide.md")
}

fn save_app_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let config_path = get_config_path(app)?;
    let config_json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_path, config_json).map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_exit_session_draft(
    workspace_path: String,
    scratchpad_content: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let current_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }
    apply_unset_path_defaults(&mut config);

    let preferred_workspace = if workspace_path.trim().is_empty() {
        current_path
    } else {
        workspace_path.trim().to_string()
    };

    let project_root = resolve_workspace_root(&preferred_workspace, &config)
        .ok_or_else(|| "No valid workspace root available. Load a workspace first.".to_string())?;

    let theory_dir = if !config.theory_md_dir.is_empty() {
        config.theory_md_dir.clone()
    } else {
        project_root.to_string_lossy().to_string()
    };

    let master_axiom_path = if config.master_axiom_file.trim().is_empty() {
        project_root.join("master_axiom.md")
    } else {
        PathBuf::from(&config.master_axiom_file)
    };

    let scan = scan_markdown_theory(&theory_dir);
    let mut recap = build_session_recap_markdown(&project_root, &theory_dir, &master_axiom_path, &scan);

    let scratchpad_trimmed = scratchpad_content.trim();
    if !scratchpad_trimmed.is_empty() {
        recap.push_str("\n## User Notes Snapshot\n");
        recap.push_str(scratchpad_trimmed);
        recap.push('\n');
    }

    let notes_path = project_root.join("next_session_notes.md");
    let existing_notes = fs::read_to_string(&notes_path).unwrap_or_default();
    let notes_draft = if !scratchpad_trimmed.is_empty() {
        scratchpad_trimmed.to_string()
    } else {
        existing_notes
    };

    let primer_path = project_root.join("ai_briefing.md");
    let primer_preview = fs::read_to_string(&primer_path)
        .unwrap_or_else(|_| "Primer file has not been generated yet for this workspace.".to_string());

    let payload = serde_json::json!({
        "status": "ok",
        "project_root": project_root.to_string_lossy().to_string(),
        "recap_draft": recap,
        "notes_draft": notes_draft,
        "reuse_notes_next_session": config.reuse_notes_next_session,
        "primer_path": primer_path.to_string_lossy().to_string(),
        "primer_preview": primer_preview
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn get_master_axiom_snapshot(
    workspace_path: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let current_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }
    apply_unset_path_defaults(&mut config);

    let preferred_workspace = if workspace_path.trim().is_empty() {
        current_path
    } else {
        workspace_path.trim().to_string()
    };

    let project_root = resolve_workspace_root(&preferred_workspace, &config)
        .ok_or_else(|| "No valid workspace root available. Load a workspace first.".to_string())?;

    let theory_dir = if !config.theory_md_dir.is_empty() {
        config.theory_md_dir.clone()
    } else {
        project_root.to_string_lossy().to_string()
    };

    let master_axiom_path = if config.master_axiom_file.trim().is_empty() {
        project_root.join("master_axiom.md")
    } else {
        PathBuf::from(&config.master_axiom_file)
    };

    if !master_axiom_path.exists() {
        let scan = scan_markdown_theory(&theory_dir);
        let template = build_master_axiom_template(
            &theory_dir,
            master_axiom_path.to_string_lossy().as_ref(),
            &scan,
        );
        if let Some(parent) = master_axiom_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create axiom directory: {e}"))?;
        }
        fs::write(&master_axiom_path, template).map_err(|e| format!("Failed to create master axiom file: {e}"))?;
    }

    let content = fs::read_to_string(&master_axiom_path)
        .map_err(|e| format!("Failed to read master axiom file: {e}"))?;

    let payload = serde_json::json!({
        "status": "ok",
        "path": master_axiom_path.to_string_lossy().to_string(),
        "content": content
    });

    Ok(payload.to_string())
}

// --- TAURI COMMANDS ---

#[tauri::command]
fn get_initial_state(app: AppHandle) -> AppConfig {
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(config_data) = fs::read_to_string(config_path) {
            let mut config = serde_json::from_str(&config_data).unwrap_or_default();
            apply_unset_path_defaults(&mut config);
            return config;
        }
    }

    let mut config = AppConfig::default();
    apply_unset_path_defaults(&mut config);
    config
}

#[tauri::command]
fn read_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let path_buf = std::path::PathBuf::from(&path);
    
    let entries = fs::read_dir(&path_buf)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let entry_path = entry.path();
        
        // Prevent adding the directory itself to its own contents
        if entry_path == path_buf { continue; }

        files.push(FileEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry_path.to_string_lossy().into_owned(),
            is_dir: entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
        });
    }
    
    Ok(files)
}

#[tauri::command]
#[allow(non_snake_case)]
fn export_workspace_tree(rootPath: String) -> Result<String, String> {
    let root = std::path::Path::new(&rootPath);
    let tree_string = build_workspace_tree_string(root)?;
    
    // Save to workspace root for easy discovery.
    let output_file_path = root.join("workspace_tree.txt");
    std::fs::write(&output_file_path, tree_string).map_err(|e| format!("Failed to write file: {}", e))?;
    
    Ok(format!("Workspace tree generated: {}", output_file_path.to_string_lossy()))
}

#[tauri::command]
fn save_root_directory(path: String, state: tauri::State<AppState>) -> Result<String, String> {
    // Locks the mutex to update the global memory state
    let mut current_path = state.workspace_path.lock().map_err(|e| e.to_string())?;
    *current_path = path.clone();
    Ok(format!("Workspace root saved: {}", path))
}

#[tauri::command]
fn git_pull(state: tauri::State<AppState>) -> Result<String, String> {
    let current_path = state.workspace_path.lock().map_err(|_| "Mutex poisoned")?.clone();
    if current_path.is_empty() { return Err("No workspace loaded.".to_string()); }

    let output = std::process::Command::new("git")
        .arg("pull")
        .current_dir(&current_path)
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if output.status.success() {
        Ok(format!("Pull successful:\n{}", String::from_utf8_lossy(&output.stdout)))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
#[allow(non_snake_case)]
fn restore_version(tag: String, rootPath: String) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["reset", "--hard", &tag])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("Project flushed. Rolled back strictly to version {}.", tag))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
#[allow(non_snake_case)]
fn save_as_hypothesis(name: String, rootPath: String) -> Result<String, String> {
    std::process::Command::new("git").arg("init").current_dir(&rootPath).output().ok();
    std::process::Command::new("git").args(["add", "."]).current_dir(&rootPath).output().ok();
    std::process::Command::new("git").args(["commit", "-m", "Auto-commit before branching"]).current_dir(&rootPath).output().ok();

    let output = std::process::Command::new("git")
        .args(["checkout", "-b", &name])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("Hypothesis branch '{}' created. Sandbox activated.", name))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
fn git_push(state: tauri::State<AppState>) -> Result<String, String> {
    let current_path = state.workspace_path.lock().map_err(|_| "Mutex poisoned")?.clone();
    if current_path.is_empty() { return Err("No workspace loaded.".to_string()); }

    let output = std::process::Command::new("git")
        .arg("push")
        .current_dir(&current_path)
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if output.status.success() {
        Ok(format!("Push successful:\n{}", String::from_utf8_lossy(&output.stderr))) // Git push often outputs success to stderr
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[cfg(test)]
mod llm_prompt_tests {
    use super::*;

    #[test]
    fn builds_prompt_text_from_history() {
        let history = vec![
            serde_json::json!({"role": "system", "content": "You are a helpful assistant"}),
            serde_json::json!({"role": "user", "content": "Summarize the theory"}),
            serde_json::json!({"role": "assistant", "content": "A concise summary"}),
        ];

        let prompt = build_prompt_from_history(&history);
        assert!(prompt.contains("System:"));
        assert!(prompt.contains("User:"));
        assert!(prompt.contains("Assistant:"));
    }
}

#[tauri::command]
async fn send_llm_prompt(pane: String, history: Vec<serde_json::Value>, app: tauri::AppHandle) -> Result<String, String> {
    let config = get_app_config(&app)?;
    let (provider, model) = provider_settings_for_pane(&config, &pane);

    let prompt = build_prompt_from_history(&history);
    let prompt_for_model = prompt;

    let provider_name = provider.to_ascii_lowercase();
    let openai_api_key = config.openai_api_key.clone();
    let gemini_api_key = config.gemini_api_key.clone();
    let request_history = history.clone();

    tauri::async_runtime::spawn_blocking(move || {
        match provider_name.as_str() {
            "openai" => call_openai(&openai_api_key, &model, &prompt_for_model),
            "gemini" => call_gemini(&gemini_api_key, &model, &request_history),
            other => Err(format!("Unsupported provider '{}'. Choose OpenAI or Gemini.", other)),
        }
    })
    .await
    .map_err(|e| format!("LLM worker task failed: {e}"))?
}

fn get_app_config(app: &AppHandle) -> Result<AppConfig, String> {
    let config_path = get_config_path(app)?;
    if let Ok(data) = fs::read_to_string(&config_path) {
        return serde_json::from_str::<AppConfig>(&data).map_err(|e| e.to_string());
    }
    Ok(AppConfig::default())
}

fn provider_settings_for_pane<'a>(config: &'a AppConfig, pane: &'a str) -> (&'a str, String) {
    let provider = if pane.eq_ignore_ascii_case("left") {
        config.left_provider.as_str()
    } else {
        config.right_provider.as_str()
    };

    let model = if pane.eq_ignore_ascii_case("left") {
        config.left_model.clone()
    } else {
        config.right_model.clone()
    };

    let provider = if provider.is_empty() { "openai" } else { provider };
    let pane_is_left = pane.eq_ignore_ascii_case("left");
    let model = if model.is_empty() {
        match provider.to_ascii_lowercase().as_str() {
            "openai" => {
                if pane_is_left {
                    "gpt-4.1".to_string()
                } else {
                    "gpt-4.1-mini".to_string()
                }
            }
            "gemini" => "gemini-2.0-flash".to_string(),
            _ => {
                if pane_is_left {
                    "gpt-4.1".to_string()
                } else {
                    "gpt-4.1-mini".to_string()
                }
            }
        }
    } else {
        model
    };

    (provider, model)
}

fn normalize_model_for_provider(provider: &str, model: &str) -> String {
    let trimmed = model.trim().trim_start_matches("models/");
    match provider.to_ascii_lowercase().as_str() {
        "gemini" => match trimmed {
            "gemini-2.0-flash" | "gemini-2.0-flash-lite" => trimmed.to_string(),
            "gemini-1.5-flash" | "gemini-2.5-flash" => "gemini-2.0-flash".to_string(),
            "gemini-1.5-pro" | "gemini-2.5-pro" | "gemini-3.6-flash" => "gemini-2.0-flash-lite".to_string(),
            "gemini-3-flash" => "gemini-2.0-flash".to_string(),
            other if other.is_empty() => "gemini-2.0-flash".to_string(),
            other => other.to_string(),
        },
        "openai" => match trimmed {
            "gpt-4" | "gpt-4o" => "gpt-4.1".to_string(),
            "gpt-4o-mini" => "gpt-4.1-mini".to_string(),
            other if other.is_empty() => "gpt-4.1-mini".to_string(),
            other => other.to_string(),
        },
        _ => trimmed.to_string(),
    }
}

fn build_prompt_from_history(history: &[serde_json::Value]) -> String {
    let mut prompt = String::new();
    for entry in history {
        if let Some(role) = entry.get("role").and_then(|v| v.as_str()) {
            let role_label = role
                .chars()
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + &role[1..].to_lowercase())
                .unwrap_or_else(|| role.to_string());

            if let Some(text) = entry.get("content").and_then(|v| v.as_str()) {
                prompt.push_str(&format!("{}: {}\n", role_label, text));
            } else if let Some(parts) = entry.get("parts").and_then(|v| v.as_array()) {
                let joined = parts.iter().filter_map(|part| part.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join(" ");
                prompt.push_str(&format!("{}: {}\n", role_label, joined));
            }
        }
    }
    prompt.trim().to_string()
}

fn call_openai(api_key: &str, model: &str, prompt: &str) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key is not configured. Please add one in Settings.".to_string());
    }

    fn parse_openai_error_details(response_text: &str) -> String {
        if let Ok(parsed) = serde_json::from_str::<OpenAiApiErrorResponse>(response_text) {
            let mut parts = Vec::new();
            if !parsed.error.message.trim().is_empty() {
                parts.push(parsed.error.message.trim().to_string());
            }
            if !parsed.error.r#type.trim().is_empty() {
                parts.push(format!("type={}", parsed.error.r#type.trim()));
            }
            if !parsed.error.code.trim().is_empty() {
                parts.push(format!("code={}", parsed.error.code.trim()));
            }

            if !parts.is_empty() {
                return parts.join(" | ");
            }
        }

        response_text.trim().to_string()
    }

    fn is_retryable_openai_model_error(status: reqwest::StatusCode, message: &str) -> bool {
        let lower = message.to_ascii_lowercase();
        status == reqwest::StatusCode::NOT_FOUND
            || status == reqwest::StatusCode::FORBIDDEN && lower.contains("access to model")
            || lower.contains("model") && (lower.contains("not found") || lower.contains("does not exist"))
            || lower.contains("you do not have access to model")
            || lower.contains("does not have access to model")
            || lower.contains("code=model_not_found")
    }

    fn openai_model_candidates(model: &str) -> Vec<String> {
        let requested = normalize_model_for_provider("openai", model);
        let mut candidates = vec![requested.clone()];

        for fallback in ["gpt-4.1-mini", "gpt-4.1", "gpt-4o-mini-2024-07-18"] {
            if !candidates
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(fallback))
            {
                candidates.push(fallback.to_string());
            }
        }

        candidates
    }

    let client = reqwest::blocking::Client::new();
    let mut attempted_models = Vec::new();
    let mut last_error = String::new();

    for candidate_model in openai_model_candidates(model) {
        attempted_models.push(candidate_model.clone());

        let response = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": candidate_model.clone(),
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.7
            }))
            .send()
            .map_err(|e| format!("OpenAI request failed: {e}"))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| format!("OpenAI response parsing failed: {e}"))?;

        if status.is_success() {
            let parsed: serde_json::Value = serde_json::from_str(&response_text)
                .map_err(|e| format!("OpenAI response parsing failed: {e}"))?;

            return parsed
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .ok_or_else(|| "OpenAI returned no response content".to_string());
        }

        let parsed_error = parse_openai_error_details(&response_text);
        last_error = format!(
            "OpenAI request failed for model '{}' with status {}: {}",
            candidate_model, status, parsed_error
        );

        // 429 frequently means billing/quota exhaustion or trial expiration, not local app overuse.
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(format!(
                "{} | This usually indicates billing/quota limits on the OpenAI account or project key, not that this local app over-requested.",
                last_error
            ));
        }

        if !is_retryable_openai_model_error(status, &parsed_error) {
            return Err(last_error);
        }
    }

    Err(format!(
        "{} | attempted models: {}",
        last_error,
        attempted_models.join(", ")
    ))
}

fn build_gemini_request_body(history: &[serde_json::Value]) -> Result<serde_json::Value, String> {
    let mut contents = Vec::new();

    for entry in history {
        let role = entry.get("role").and_then(|value| value.as_str()).unwrap_or_default();
        let text = if let Some(text) = entry.get("content").and_then(|value| value.as_str()) {
            text.to_string()
        } else if let Some(parts) = entry.get("parts").and_then(|value| value.as_array()) {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            String::new()
        };

        if text.trim().is_empty() {
            continue;
        }

        let gemini_role = match role.to_ascii_lowercase().as_str() {
            "assistant" | "model" => "model",
            "system" => "user",
            _ => "user",
        };

        contents.push(serde_json::json!({
            "role": gemini_role,
            "parts": [{"text": text}]
        }));
    }

    if contents.is_empty() {
        return Err("Gemini request body could not be built from the supplied history".to_string());
    }

    Ok(serde_json::json!({ "contents": contents }))
}

fn list_gemini_generate_content_models(
    client: &reqwest::blocking::Client,
    api_key: &str,
) -> Result<Vec<String>, String> {
    let response = client
        .get(format!("https://generativelanguage.googleapis.com/v1beta/models?key={api_key}"))
        .send()
        .map_err(|e| format!("Gemini model discovery failed: {e}"))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("Gemini model discovery response parsing failed: {e}"))?;

    if !status.is_success() {
        let parsed_error = serde_json::from_str::<serde_json::Value>(&response_text)
            .ok()
            .and_then(|json| {
                json.get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::to_string)
            })
            .unwrap_or(response_text);

        return Err(format!(
            "Gemini model discovery failed with status {}: {}",
            status, parsed_error
        ));
    }

    let parsed: GeminiModelListResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Gemini model discovery response parsing failed: {e}"))?;

    let mut models = Vec::new();
    for model in parsed.models {
        let supports_generate_content = model
            .supported_generation_methods
            .iter()
            .any(|method| method.eq_ignore_ascii_case("generateContent"));

        if supports_generate_content {
            let normalized = model.name.trim().trim_start_matches("models/").to_string();
            if !normalized.is_empty() {
                models.push(normalized);
            }
        }
    }

    Ok(models)
}

fn gemini_model_candidates(
    client: &reqwest::blocking::Client,
    api_key: &str,
    model: &str,
) -> Vec<String> {
    let mut candidates = vec![model.to_string()];

    fn is_chat_capable_model_name(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        let disallowed_markers = [
            "tts",
            "embedding",
            "aqa",
            "transcribe",
            "speech",
            "audio",
            "image",
            "vision",
            "veo",
            "live",
            "realtime",
        ];

        !disallowed_markers.iter().any(|marker| lower.contains(marker))
    }

    if let Ok(discovered_models) = list_gemini_generate_content_models(client, api_key) {
        let requested = model.to_ascii_lowercase();
        let mut preferred = Vec::new();
        let mut others = Vec::new();

        for discovered in discovered_models {
            if discovered.eq_ignore_ascii_case(model) {
                continue;
            }

            if !is_chat_capable_model_name(&discovered) {
                continue;
            }

            let lower = discovered.to_ascii_lowercase();
            let is_preferred = if requested.contains("pro") {
                lower.contains("pro")
            } else if requested.contains("flash") {
                lower.contains("flash")
            } else {
                true
            };

            if is_preferred {
                preferred.push(discovered);
            } else {
                others.push(discovered);
            }
        }

        candidates.extend(preferred);
        candidates.extend(others);
    }

    for fallback in [
        "gemini-2.0-flash",
        "gemini-2.0-flash-lite",
        "gemini-1.5-flash",
    ] {
        if !candidates.iter().any(|existing| existing == fallback) {
            candidates.push(fallback.to_string());
        }
    }

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&candidate))
        {
            deduped.push(candidate);
        }
    }

    deduped
}

fn call_gemini(api_key: &str, model: &str, history: &[serde_json::Value]) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("Gemini API key is not configured. Please add one in Settings.".to_string());
    }

    let normalized_model = normalize_model_for_provider("gemini", model);

    let client = reqwest::blocking::Client::new();
    let body = build_gemini_request_body(history).map_err(|e| format!("Gemini request body error: {e}"))?;

    let mut last_error = "Gemini request failed before receiving a response".to_string();
    let mut attempted_models = Vec::new();

    for candidate_model in gemini_model_candidates(&client, api_key, &normalized_model) {
        attempted_models.push(candidate_model.clone());
        let response = client
            .post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{candidate_model}:generateContent?key={api_key}"
            ))
            .json(&body)
            .send()
            .map_err(|e| format!("Gemini request failed: {e}"))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| format!("Gemini response parsing failed: {e}"))?;

        if status.is_success() {
            let parsed: serde_json::Value = serde_json::from_str(&response_text)
                .map_err(|e| format!("Gemini response parsing failed: {e}"))?;

            return parsed
                .get("candidates")
                .and_then(|candidates| candidates.get(0))
                .and_then(|candidate| candidate.get("content"))
                .and_then(|content| content.get("parts"))
                .and_then(|parts| parts.get(0))
                .and_then(|part| part.get("text"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .ok_or_else(|| "Gemini returned no response content".to_string());
        }

        let parsed_error = serde_json::from_str::<serde_json::Value>(&response_text)
            .ok()
            .and_then(|json| {
                json.get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| response_text.clone());

        last_error = format!(
            "Gemini request failed for model '{}' with status {}: {}",
            candidate_model, status, parsed_error
        );

        let retryable_model_error = status == reqwest::StatusCode::NOT_FOUND
            || parsed_error
                .to_ascii_lowercase()
                .contains("not supported for generatecontent")
            || parsed_error
                .to_ascii_lowercase()
                .contains("is not found for api version")
            || parsed_error
                .to_ascii_lowercase()
                .contains("multiturn chat is not enabled");

        if !retryable_model_error {
            return Err(last_error);
        }
    }

    Err(format!(
        "{} | attempted models: {}",
        last_error,
        attempted_models.join(", ")
    ))
}

fn recursive_markdown_scan(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            recursive_markdown_scan(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}

fn recursive_file_scan(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            recursive_file_scan(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn sanitize_slug(value: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_underscore = false;

    for ch in value.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch);
            last_was_underscore = false;
        } else {
            if !last_was_underscore {
                cleaned.push('_');
                last_was_underscore = true;
            }
        }
    }

    cleaned.trim_matches('_').to_string()
}

fn identify_theory_mode(scan: &serde_json::Value) -> &'static str {
    let content = scan["file_summaries"]
        .as_array()
        .map(|items| {
            items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join(" \n ")
        })
        .unwrap_or_default()
        + "\n"
        + &scan["headings"].as_array().map(|items| {
            items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join(" \n ")
        }).unwrap_or_default();

    let lower = content.to_lowercase();
    let has_left_field = ["bimodal", "emergent constraint", "seam stress", "boundary seam", "topological", "left-field", "emergent"].iter().any(|marker| lower.contains(marker));
    let has_mainstream = ["lambda", "cosmological constant", "einstein", "general relativity", "standard model", "flrw", "metric"].iter().any(|marker| lower.contains(marker));

    if has_left_field && !has_mainstream {
        "left_field"
    } else if has_left_field && has_mainstream {
        "hybrid"
    } else {
        "mainstream"
    }
}

fn collect_markdown_files(directory_path: &str) -> Result<Vec<String>, String> {
    let path = Path::new(directory_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Directory does not exist: {}", directory_path));
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| format!("Failed to read directory: {e}"))? {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let file_path = entry.path();
        if file_path.is_file() {
            if let Some(ext) = file_path.extension().and_then(|ext| ext.to_str()) {
                if ext.eq_ignore_ascii_case("md") {
                    files.push(file_path.to_string_lossy().to_string());
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

fn build_probe_terms(query: &str) -> Vec<String> {
    let lower = query.to_ascii_lowercase();
    let mut terms = Vec::new();

    let mut push_term = |term: &str| {
        let trimmed = term.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && !terms.iter().any(|existing| existing == &trimmed) {
            terms.push(trimmed);
        }
    };

    for token in lower
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .map(str::trim)
        .filter(|token| token.len() >= 4)
    {
        push_term(token);
    }

    if lower.contains("black hole") || lower.contains("blackhole") || lower.contains("black_hole") {
        push_term("black hole");
        push_term("blackhole");
        push_term("black_hole");
    }

    if lower.contains("collision") {
        push_term("collision");
        push_term("collisions");
    }

    if lower.contains("result") || lower.contains("study") || lower.contains("analysis") {
        push_term("results");
        push_term("analysis");
        push_term("empirical");
        push_term("validation");
        push_term("observational");
        push_term("evidence");
    }

    if lower.contains("appendix") {
        push_term("appendix");
    }

    terms
}

fn score_probe_content(relative_path: &str, content: &str, terms: &[String]) -> (i32, Vec<String>) {
    let path_lower = relative_path.to_ascii_lowercase();
    let content_lower = content.to_ascii_lowercase();
    let mut score = 0i32;

    for term in terms {
        if path_lower.contains(term) {
            score += 12;
        }

        let hits = content_lower.matches(term).count() as i32;
        score += (hits.min(8)) * 5;
    }

    if path_lower.contains("appendix_g") {
        score += 18;
    }

    if path_lower.contains("empirical") || path_lower.contains("validation") {
        score += 10;
    }

    let mut snippets = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line_lower = line.to_ascii_lowercase();
        if terms.iter().any(|term| line_lower.contains(term)) {
            let cleaned = line.trim();
            if cleaned.is_empty() {
                continue;
            }

            snippets.push(format!("L{}: {}", index + 1, cleaned));
            if snippets.len() >= 4 {
                break;
            }
        }
    }

    (score, snippets)
}

#[tauri::command]
fn collect_probe_evidence(workspace_path: String, query: String) -> Result<Vec<ProbeEvidenceItem>, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for probe evidence scan.".to_string());
    }

    let terms = build_probe_terms(&query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut markdown_files = Vec::new();
    let _ = recursive_markdown_scan(&root, &mut markdown_files);

    let mut ranked = Vec::new();
    for file in markdown_files {
        let relative_path = file
            .strip_prefix(&root)
            .unwrap_or(file.as_path())
            .to_string_lossy()
            .replace('\\', "/");

        let file_name_lower = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        if !file_name_lower.contains("appendix") {
            continue;
        }

        let content = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(_) => continue,
        };

        let truncated = if content.len() > 120_000 {
            &content[..120_000]
        } else {
            &content
        };

        let (score, snippets) = score_probe_content(&relative_path, truncated, &terms);
        ranked.push(ProbeEvidenceItem {
            relative_path,
            score,
            snippets,
        });
    }

    ranked.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });

    Ok(ranked.into_iter().take(8).collect())
}

fn render_manuscript_content(
    mode: &str,
    source_file: &str,
    files: &[String],
    output_dir: &str,
    format: &str,
    use_for_training: bool,
    source_dir: Option<&str>,
) -> Result<serde_json::Value, String> {
    let output_path = PathBuf::from(output_dir);
    fs::create_dir_all(&output_path).map_err(|e| format!("Failed to create output directory: {e}"))?;

    let content = if mode == "combine" {
        let selected_files = if files.is_empty() {
            let directory_to_use = source_dir.filter(|value| !value.trim().is_empty()).unwrap_or(output_dir);
            collect_markdown_files(directory_to_use)?
        } else {
            files.to_vec()
        };

        let mut combined = String::new();
        for file in &selected_files {
            let path = Path::new(file);
            if !path.exists() {
                continue;
            }
            let text = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
            combined.push_str(&format!("# {}\n\n{}\n\n", path.file_stem().unwrap_or_default().to_string_lossy(), text));
        }
        combined
    } else {
        let path = Path::new(source_file);
        if !path.exists() {
            return Err(format!("Source file does not exist: {}", source_file));
        }
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?
    };

    let output_name = if mode == "combine" {
        "master_manuscript".to_string()
    } else {
        let path = Path::new(source_file);
        path.file_stem().unwrap_or_default().to_string_lossy().to_string()
    };

    let output_ext = match format {
        "pdf" => "pdf",
        "docx" => "docx",
        _ => "md",
    };
    let rendered_path = output_path.join(format!("{}.{}", output_name, output_ext));
    fs::write(&rendered_path, &content).map_err(|e| format!("Failed to write rendered artifact: {e}"))?;

    let training_path = if use_for_training {
        let training_dir = output_path.join("ai_training");
        fs::create_dir_all(&training_dir).map_err(|e| format!("Failed to create training directory: {e}"))?;
        let training_file = training_dir.join(format!("{}.{}", output_name, output_ext));
        fs::copy(&rendered_path, &training_file).map_err(|e| format!("Failed to copy training artifact: {e}"))?;
        Some(training_file.to_string_lossy().to_string())
    } else {
        None
    };

    Ok(serde_json::json!({
        "output_path": rendered_path.to_string_lossy().to_string(),
        "format": output_ext,
        "training_path": training_path
    }))
}

fn import_theory_source(source_path: &Path, output_dir: &Path) -> Result<serde_json::Value, String> {
    if !source_path.exists() {
        return Err(format!("Source path does not exist: {}", source_path.display()));
    }

    let source_extension = source_path.extension().and_then(|ext| ext.to_str()).unwrap_or_default().to_lowercase();
    let mut created_files = Vec::new();

    fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create output directory: {e}"))?;

    let source_type = if source_extension == "md" {
        "markdown"
    } else if source_extension == "docx" || source_extension == "doc" {
        "document"
    } else {
        "manuscript"
    };

    let contents = fs::read_to_string(source_path).map_err(|e| format!("Failed to read source file: {e}"))?;

    if source_type == "markdown" {
        let file_name = source_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let target_path = output_dir.join(format!("{}.md", file_name));
        fs::write(&target_path, &contents).map_err(|e| format!("Failed to write markdown import: {e}"))?;
        created_files.push(target_path.to_string_lossy().to_string());
    } else {
        let mut sections: Vec<(String, String)> = Vec::new();
        let mut current_title = "chapter_1".to_string();
        let mut current_body = String::new();

        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.to_lowercase().starts_with("chapter") || trimmed.to_lowercase().starts_with("section") {
                if !current_body.trim().is_empty() {
                    sections.push((current_title.clone(), current_body.trim().to_string()));
                }
                current_title = sanitize_slug(trimmed);
                current_body.clear();
            } else {
                current_body.push_str(trimmed);
                current_body.push('\n');
            }
        }

        if !current_body.trim().is_empty() {
            sections.push((current_title.clone(), current_body.trim().to_string()));
        }

        if sections.is_empty() {
            let fallback_path = output_dir.join("chapter_1.md");
            fs::write(&fallback_path, &contents).map_err(|e| format!("Failed to write fallback chapter import: {e}"))?;
            created_files.push(fallback_path.to_string_lossy().to_string());
        } else {
            for (_index, (title, body)) in sections.iter().enumerate() {
                let file_name = format!("{}.md", sanitize_slug(title));
                let target_path = output_dir.join(file_name);
                let markdown = format!("# {}\n\n{}\n", title, body);
                fs::write(&target_path, markdown).map_err(|e| format!("Failed to write imported section: {e}"))?;
                created_files.push(target_path.to_string_lossy().to_string());
            }

            let equation_path = output_dir.join("equations.md");
            let equation_content = contents
                .lines()
                .filter(|line| line.contains('$') || line.contains("\\math") || line.contains("\\frac") || line.contains("\\partial"))
                .collect::<Vec<_>>()
                .join("\n");
            if !equation_content.trim().is_empty() {
                fs::write(&equation_path, format!("# Equations\n\n{}\n", equation_content)).map_err(|e| format!("Failed to write equations file: {e}"))?;
                created_files.push(equation_path.to_string_lossy().to_string());
            }
        }
    }

    let scan = scan_markdown_theory(output_dir.to_string_lossy().as_ref());
    let mode = identify_theory_mode(&scan);

    Ok(serde_json::json!({
        "source_type": source_type,
        "mode": mode,
        "files_created": created_files,
        "output_dir": output_dir.to_string_lossy().to_string()
    }))
}

fn scan_markdown_theory(theory_dir: &str) -> serde_json::Value {
    let mut files = Vec::new();
    let theory_path = Path::new(theory_dir);

    if theory_path.exists() {
        let _ = recursive_markdown_scan(theory_path, &mut files);
    }

    let mut headings = Vec::new();
    let mut lagrangian_candidates = Vec::new();
    let mut hypothesis_candidates = Vec::new();
    let mut equation_candidates = Vec::new();
    let mut file_summaries = Vec::new();

    for path in &files {
        if let Ok(content) = fs::read_to_string(path) {
            let relative_path = path.strip_prefix(theory_path).unwrap_or(path)
                .to_string_lossy()
                .to_string();

            if let Some(first_heading) = content.lines().find(|line| line.trim_start().starts_with('#')) {
                headings.push(format!("{}: {}", relative_path, first_heading.trim().trim_start_matches('#').trim()));
            }

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let lower = trimmed.to_lowercase();
                if lower.contains("lagrangian") || lower.contains("\\mathcal{l}") || lower.contains("l =") || lower.contains("action") {
                    lagrangian_candidates.push(format!("{}: {}", relative_path, trimmed));
                }

                if lower.contains("hypoth") || lower.contains("axiom") || lower.contains("postulat") {
                    hypothesis_candidates.push(format!("{}: {}", relative_path, trimmed));
                }

                if lower.contains("$$") || lower.contains("\\begin") || lower.contains("\\frac") || lower.contains("\\partial") || lower.contains("\\mathcal") {
                    equation_candidates.push(format!("{}: {}", relative_path, trimmed));
                }
            }

            if let Some(summary_line) = content.lines().find_map(|line| {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            }) {
                file_summaries.push(format!("{}: {}", relative_path, summary_line));
            }
        }
    }

    serde_json::json!({
        "theory_dir": theory_dir,
        "files_scanned": files.len(),
        "headings": headings,
        "lagrangian_candidates": lagrangian_candidates,
        "hypothesis_candidates": hypothesis_candidates,
        "equation_candidates": equation_candidates,
        "file_summaries": file_summaries
    })
}

fn detect_theory_style(scan: &serde_json::Value) -> &'static str {
    let combined = scan["headings"].as_array().map(|items| {
        items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join(" \n ")
    }).unwrap_or_default()
    + "\n"
    + &scan["file_summaries"].as_array().map(|items| {
        items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>().join(" \n ")
    }).unwrap_or_default();

    let lowercase = combined.to_lowercase();

    let left_field_markers = [
        "bimodal",
        "manifold",
        "emergent constraint",
        "seam stress",
        "boundary seam",
        "topological",
        "nonstandard",
        "left-field",
        "alternative",
        "emergent",
    ];

    let mainstream_markers = [
        "lambda",
        "cosmological constant",
        "einstein",
        "general relativity",
        "standard model",
        "perturbation",
        "metric",
        "flrw",
    ];

    let has_left_field = left_field_markers.iter().any(|marker| lowercase.contains(marker));
    let has_mainstream = mainstream_markers.iter().any(|marker| lowercase.contains(marker));

    if has_left_field && !has_mainstream {
        "left_field"
    } else if has_left_field && has_mainstream {
        "hybrid"
    } else {
        "mainstream"
    }
}

fn build_master_axiom_template(theory_dir: &str, master_axiom_path: &str, scan: &serde_json::Value) -> String {
    let theory_label = scan["headings"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("this cosmological model");

    let lagrangian = scan["lagrangian_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("Add the Lagrangian or action functional for the theory here.");

    let hypothesis = scan["hypothesis_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("State the core explanatory hypothesis here.");

    let style = detect_theory_style(scan);

    let (structure_section, _assumptions_section, _predictions_section) = if style == "left_field" {
        (
            "## Structural Assumptions\n- Describe the foundational geometry, interaction domain, or manifold topology assumed by the model.\n- Note any boundary-condition-like constraints or seam-like operators introduced by the theory.",
            "## Model Constraints\n- Identify any explicit constraints, conservation-like rules, or emergent operator requirements.\n- Distinguish what is postulated from what is derived or inferred.",
            "## Derived Signatures\n1. Specify a signature, scaling relation, or topological pattern that should emerge from the model.\n2. Describe the boundary or transition regime where the theory predicts a distinct behavior.\n3. Note what would count as a meaningful divergence from competing interpretations."
        )
    } else {
        (
            "## Assumptions\n- Assumption 1: State the foundational conditions under which the model is expected to hold.\n- Assumption 2: State any symmetry, conservation law, or boundary condition that is required.",
            "## Hypothesis\n{}",
            "## Predictions\n1. Specify a measurable signature or scaling relation that follows from the hypothesis.\n2. State a limiting case or boundary condition that should produce a distinct outcome.\n3. Describe the expected observational or analytic difference from competing models."
        )
    };

    let mut template = format!(
        "# Master Axiom\n\n## Core Axiom\nThe {} framework is treated here as a structured model candidate rather than as an assumed truth. Its purpose is to define a coherent internal rule set that can be tested against empirical data and compared against alternative formulations.\n\n{}\n\n",
        theory_label,
        structure_section
    );

    if style == "left_field" {
        template.push_str(&format!(
            "## Model Constraints\n- Identify any explicit constraints, conservation-like rules, or emergent operator requirements.\n- Distinguish what is postulated from what is derived or inferred.\n\n## Hypothesis\n{}\n\n## Derived Signatures\n1. Specify a signature, scaling relation, or topological pattern that should emerge from the model.\n2. Describe the boundary or transition regime where the theory predicts a distinct behavior.\n3. Note what would count as a meaningful divergence from competing interpretations.\n\n## Observational Consequences\n- Identify the observational patterns, data products, or simulation outputs implied by the theory.\n- Explain how those consequences would be distinguished from alternative interpretations.\n\n## Testable Criteria\n- What evidence would confirm the hypothesis?\n- What evidence would falsify or constrain it?\n\n## Lagrangian / Action\n{}\n\n## Source Context\n- Theory directory: {}\n- Master axiom file: {}\n- Files scanned: {}\n",
            hypothesis,
            lagrangian,
            theory_dir,
            master_axiom_path,
            scan["files_scanned"].as_u64().unwrap_or(0)
        ));
    } else {
        template.push_str(&format!(
            "## Hypothesis\n{}\n\n## Predictions\n1. Specify a measurable signature or scaling relation that follows from the hypothesis.\n2. State a limiting case or boundary condition that should produce a distinct outcome.\n3. Describe the expected observational or analytic difference from competing models.\n\n## Observational Consequences\n- Identify the observational patterns, data products, or simulation outputs implied by the theory.\n- Explain how those consequences would be distinguished from alternative interpretations.\n\n## Testable Criteria\n- What evidence would confirm the hypothesis?\n- What evidence would falsify or constrain it?\n\n## Lagrangian / Action\n{}\n\n## Source Context\n- Theory directory: {}\n- Master axiom file: {}\n- Files scanned: {}\n",
            hypothesis,
            lagrangian,
            theory_dir,
            master_axiom_path,
            scan["files_scanned"].as_u64().unwrap_or(0)
        ));
    }

    template
}

// NOTE: The long-term architecture for theory ingestion is intentionally
// multi-mode and paradigm-agnostic. The IDE should detect the model family from
// the theory markdown tree and then select an appropriate parser mode (for example:
// mainstream-cosmology, hybrid, or left-field/emergent). If the user provides a
// manuscript document instead of a markdown directory, the IDE should be able to
// split that document into chapter/section markdown files and then continue the
// same ingestion workflow from those generated files.

fn try_generate_with_gemini(api_key: &str, theory_dir: &str, scan: &serde_json::Value, fallback_template: &str) -> Option<String> {
    if api_key.trim().is_empty() {
        return None;
    }

    let heading_summary = scan["headings"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("No headings found");

    let lagrangian_summary = scan["lagrangian_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("No Lagrangian detected");

    let hypothesis_summary = scan["hypothesis_candidates"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|value| value.as_str())
        .unwrap_or("No hypothesis detected");

    let prompt = format!(
        "You are helping produce a scientific master axiom file for a cosmological theory repository.\n\nTheory directory: {}\n\nDetected heading: {}\nDetected Lagrangian/action: {}\nDetected hypothesis/axiom candidate: {}\n\nWrite a polished markdown master axiom document with sections: Core Axiom, Assumptions, Hypothesis, Predictions, Observational Consequences, Testable Criteria, and Lagrangian / Action. Keep it concise, scientific, and suitable for a researcher to refine.\n\nIf the evidence is sparse, preserve the human-in-the-loop placeholders rather than inventing unsupported details.\n\nFallback template:\n{}",
        theory_dir, heading_summary, lagrangian_summary, hypothesis_summary, fallback_template
    );

    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }]
    });

    for candidate_model in ["gemini-2.0-flash", "gemini-2.0-flash-lite"] {
        let response = client
            .post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{candidate_model}:generateContent?key={}",
                api_key
            ))
            .json(&body)
            .send()
            .ok()?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                continue;
            }
            return None;
        }

        let text_body = response.text().ok()?;
        let parsed: GeminiApiResponse = serde_json::from_str(&text_body).ok()?;
        if let Some(result) = parsed.candidates.into_iter().find_map(|candidate| {
            candidate.content.parts.into_iter().find_map(|part| {
                let text = part.text.trim();
                if text.is_empty() { None } else { Some(text.to_string()) }
            })
        })
        {
            return Some(result);
        }
    }

    None
}

#[tauri::command]
fn compile_ai_briefing(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<String, String> {
    let current_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }

    apply_unset_path_defaults(&mut config);

    let project_root = resolve_workspace_root(&current_path, &config)
        .ok_or_else(|| "No valid workspace root available. Load a workspace first.".to_string())?;

    let theory_dir = if !config.theory_md_dir.is_empty() {
        config.theory_md_dir.clone()
    } else if !config.project_root_dir.is_empty() {
        config.project_root_dir.clone()
    } else {
        project_root.to_string_lossy().to_string()
    };

    let tools_dir = if config.tools_dir.trim().is_empty() {
        let fallback = project_root.join("src").join("analysis");
        fallback.to_string_lossy().to_string()
    } else {
        config.tools_dir.clone()
    };

    let scan = scan_markdown_theory(&theory_dir);
    let master_axiom_path = if config.master_axiom_file.trim().is_empty() {
        project_root.join("master_axiom.md")
    } else {
        PathBuf::from(&config.master_axiom_file)
    };

    let template = build_master_axiom_template(
        &theory_dir,
        master_axiom_path.to_string_lossy().as_ref(),
        &scan,
    );

    if !master_axiom_path.exists() {
        if let Some(parent) = master_axiom_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&master_axiom_path, &template);
    }

    let tree_path = project_root.join("workspace_tree.txt");
    if let Ok(tree) = build_workspace_tree_string(&project_root) {
        let _ = fs::write(&tree_path, tree);
    }

    let primer_path = project_root.join("next_session_notes.md");
    let recap_path = project_root.join("session_recap.md");
    let awareness_path = project_root.join("project_awareness.md");
    let briefing_path = project_root.join("ai_briefing.md");
    let recap_existed_before = recap_path.exists();
    let briefing_existed_before = briefing_path.exists();
    let first_session_bootstrap = !config.first_session_completed && !briefing_existed_before && !recap_existed_before;

    if !recap_path.exists() {
        let recap = build_session_recap_markdown(&project_root, &theory_dir, &master_axiom_path, &scan);
        let _ = fs::write(&recap_path, recap);
    }

    let mut diagnostics: Vec<String> = Vec::new();
    let (primer_payload, primer_diag) = if config.reuse_notes_next_session {
        read_source_payload("primer", &primer_path, 12_000)
    } else {
        (
            serde_json::json!({
                "name": "primer",
                "path": primer_path.to_string_lossy().to_string(),
                "exists": false,
                "content": "",
                "bytes": 0,
                "lines": 0,
                "truncated": false,
                "disabled": true
            }),
            None,
        )
    };
    let (recap_payload, recap_diag) = read_source_payload("session_recap", &recap_path, 12_000);
    let (tree_payload, tree_diag) = read_source_payload("workspace_tree", &tree_path, 20_000);
    let (axiom_payload, axiom_diag) = read_source_payload("master_axiom", &master_axiom_path, 16_000);

    if let Some(diag) = primer_diag {
        diagnostics.push(diag);
    }
    if let Some(diag) = recap_diag {
        diagnostics.push(diag);
    }
    if let Some(diag) = tree_diag {
        diagnostics.push(diag);
    }
    if let Some(diag) = axiom_diag {
        diagnostics.push(diag);
    }

    let files_scanned = scan["files_scanned"].as_u64().unwrap_or(0);
    let lagrangian_count = scan["lagrangian_candidates"].as_array().map(|a| a.len()).unwrap_or(0);
    let hypothesis_count = scan["hypothesis_candidates"].as_array().map(|a| a.len()).unwrap_or(0);

    let summary = format!(
        "{}",
        if first_session_bootstrap {
            format!(
                "First-session bootstrap active for {}. Greet the user, help configure project import/theory markdown output/master axiom path, and guide setup until the first recap-driven packet can replace this starter.",
                project_root.to_string_lossy()
            )
        } else {
            format!(
                "Startup primer assembled for {} with {} scanned markdown files, {} Lagrangian/action cues, and {} hypothesis cues. Keep responses concise, anchor in current axioms, and align today\'s goals before deeper analysis.",
                project_root.to_string_lossy(),
                files_scanned,
                lagrangian_count,
                hypothesis_count
            )
        }
    );

    let awareness_markdown = build_project_awareness_markdown(
        &project_root,
        &theory_dir,
        &master_axiom_path,
        &tools_dir,
        &scan,
    );

    let ai_briefing_markdown = if first_session_bootstrap {
        build_first_session_briefing_markdown(&project_root)
    } else {
        build_ai_briefing_markdown(
            &project_root,
            &summary,
            &primer_path,
            &recap_path,
            &tree_path,
            &master_axiom_path,
            &awareness_markdown,
            None,
        )
    };
    if !awareness_path.exists() {
        let _ = fs::write(&awareness_path, &awareness_markdown);
    }
    if !briefing_path.exists() {
        let _ = fs::write(&briefing_path, ai_briefing_markdown);
    }

    let (briefing_payload, briefing_diag) = read_source_payload("ai_briefing", &briefing_path, 12_000);
    if let Some(diag) = briefing_diag {
        diagnostics.push(diag);
    }

    let status = if diagnostics.is_empty() { "Ready" } else { "Partial" };

    let briefing = serde_json::json!({
        "status": status,
        "project_root": project_root.to_string_lossy().to_string(),
        "theory_directory": theory_dir,
        "master_axiom_file": master_axiom_path.to_string_lossy().to_string(),
        "files_scanned": scan["files_scanned"],
        "lagrangian_candidates": scan["lagrangian_candidates"],
        "hypothesis_candidates": scan["hypothesis_candidates"],
        "template": template,
        "summary": summary,
        "diagnostics": diagnostics,
        "primer": primer_payload,
        "session_recap": recap_payload,
        "workspace_tree": tree_payload,
        "master_axiom": axiom_payload,
        "ai_briefing": briefing_payload,
        "generated_files": {
            "session_recap": recap_path.to_string_lossy().to_string(),
            "ai_briefing": briefing_path.to_string_lossy().to_string(),
            "workspace_tree": tree_path.to_string_lossy().to_string(),
            "scratchpad": primer_path.to_string_lossy().to_string(),
            "master_axiom": master_axiom_path.to_string_lossy().to_string(),
            "project_awareness": awareness_path.to_string_lossy().to_string()
        },
        "reuse_notes_next_session": config.reuse_notes_next_session,
        "first_session_bootstrap": first_session_bootstrap,
        "onboarding_steps": [
            "Import or open project workspace",
            "Set theory markdown output directory",
            "Set or generate master axiom file",
            "Capture goals in scratchpad notes",
            "Export workspace tree",
            "Refresh and sync briefing packet to both AI lanes"
        ]
    });

    Ok(briefing.to_string())
}

#[tauri::command]
fn prepare_exit_session(
    options: ExitSessionOptions,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let current_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }
    apply_unset_path_defaults(&mut config);

    let preferred_workspace = if options.workspace_path.trim().is_empty() {
        current_path
    } else {
        options.workspace_path.trim().to_string()
    };

    let project_root = resolve_workspace_root(&preferred_workspace, &config)
        .ok_or_else(|| "No valid workspace root available. Load a workspace first.".to_string())?;

    let theory_dir = if !config.theory_md_dir.is_empty() {
        config.theory_md_dir.clone()
    } else {
        project_root.to_string_lossy().to_string()
    };

    let master_axiom_path = if config.master_axiom_file.trim().is_empty() {
        project_root.join("master_axiom.md")
    } else {
        PathBuf::from(&config.master_axiom_file)
    };

    let recap_path = project_root.join("session_recap.md");
    let notes_path = project_root.join("next_session_notes.md");
    let tree_path = project_root.join("workspace_tree.txt");
    let awareness_path = project_root.join("project_awareness.md");
    let briefing_path = project_root.join("ai_briefing.md");
    let startup_guide_path = resolve_startup_guide_path(&project_root, &theory_dir);

    let mut actions: Vec<String> = Vec::new();

    if options.create_new_axiom {
        let scan = scan_markdown_theory(&theory_dir);
        let template = build_master_axiom_template(
            &theory_dir,
            master_axiom_path.to_string_lossy().as_ref(),
            &scan,
        );
        if let Some(parent) = master_axiom_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create master axiom parent directory: {e}"))?;
        }
        fs::write(&master_axiom_path, template).map_err(|e| format!("Failed to write master axiom: {e}"))?;
        actions.push(format!("Master axiom refreshed: {}", master_axiom_path.to_string_lossy()));
    }

    if options.create_recap_md {
        let recap_override = options.recap_content_override.trim();
        let recap = if !recap_override.is_empty() {
            recap_override.to_string()
        } else {
            let scan = scan_markdown_theory(&theory_dir);
            let mut generated = build_session_recap_markdown(&project_root, &theory_dir, &master_axiom_path, &scan);
            let scratchpad_trimmed = options.scratchpad_content.trim();
            if !scratchpad_trimmed.is_empty() {
                generated.push_str("\n## User Notes Snapshot\n");
                generated.push_str(scratchpad_trimmed);
                generated.push('\n');
            }
            generated
        };
        fs::write(&recap_path, recap).map_err(|e| format!("Failed to write session recap: {e}"))?;
        actions.push(format!("Session recap saved: {}", recap_path.to_string_lossy()));

        // Replace first-run packet with session-derived briefing once recap exists.
        if let Ok(tree) = build_workspace_tree_string(&project_root) {
            let _ = fs::write(&tree_path, tree);
        }
        let exit_summary = "Post-session briefing packet generated from latest session recap and workspace context.";
        let tools_dir = if config.tools_dir.trim().is_empty() {
            project_root.join("src").join("analysis").to_string_lossy().to_string()
        } else {
            config.tools_dir.clone()
        };
        let scan = scan_markdown_theory(&theory_dir);
        let awareness_markdown = build_project_awareness_markdown(
            &project_root,
            &theory_dir,
            &master_axiom_path,
            &tools_dir,
            &scan,
        );
        let packet = build_ai_briefing_markdown(
            &project_root,
            exit_summary,
            &notes_path,
            &recap_path,
            &tree_path,
            &master_axiom_path,
            &awareness_markdown,
            None,
        );
        let _ = fs::write(&awareness_path, &awareness_markdown);
        fs::write(&briefing_path, packet).map_err(|e| format!("Failed to write briefing packet: {e}"))?;
        actions.push(format!("Briefing packet updated: {}", briefing_path.to_string_lossy()));
    }

    config.reuse_notes_next_session = options.reuse_existing_notes_next_session;
    if options.reuse_existing_notes_next_session {
        let notes_override = options.notes_content_override.trim();
        let notes_to_save = if !notes_override.is_empty() {
            notes_override
        } else {
            options.scratchpad_content.trim()
        };
        if !notes_to_save.is_empty() {
            fs::write(&notes_path, notes_to_save)
                .map_err(|e| format!("Failed to write next-session notes: {e}"))?;
            actions.push(format!("Carry-over notes saved: {}", notes_path.to_string_lossy()));
        } else {
            fs::write(&notes_path, "No carry-over notes were saved in the previous session.")
                .map_err(|e| format!("Failed to write empty carry-over note placeholder: {e}"))?;
            actions.push("Carry-over notes enabled, but scratchpad was empty. Added placeholder note file.".to_string());
        }
    } else if notes_path.exists() {
        let _ = fs::remove_file(&notes_path);
        actions.push("Carry-over notes cleared for next session.".to_string());
    }

    // Mark onboarding as complete after first full exit workflow so startup
    // instruction prompts do not reappear in AI lanes on subsequent launches.
    config.first_session_completed = true;

    let startup_guide = build_first_session_briefing_markdown(&project_root);
    fs::write(&startup_guide_path, startup_guide)
        .map_err(|e| format!("Failed to write startup guide: {e}"))?;
    actions.push(format!(
        "Startup guide saved for reference: {}",
        startup_guide_path.to_string_lossy()
    ));

    save_app_config(&app, &config)?;

    let payload = serde_json::json!({
        "status": "ok",
        "project_root": project_root.to_string_lossy().to_string(),
        "actions": actions,
        "options": {
            "create_new_axiom": options.create_new_axiom,
            "create_recap_md": options.create_recap_md,
            "reuse_existing_notes_next_session": options.reuse_existing_notes_next_session
        }
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn import_theory_source_command(source_path: String, output_dir: String, app: tauri::AppHandle) -> Result<String, String> {
    let source_path = PathBuf::from(&source_path);
    let output_dir = PathBuf::from(&output_dir);
    let result = import_theory_source(&source_path, &output_dir)?;

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }

    let mut payload = result;
    payload["master_axiom_file"] = serde_json::Value::String(config.master_axiom_file.clone());
    Ok(payload.to_string())
}

#[tauri::command]
fn generate_master_axiom_from_theory(theory_dir: String, master_axiom_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }

    let effective_theory_dir = if theory_dir.trim().is_empty() {
        if !config.theory_md_dir.is_empty() {
            config.theory_md_dir.clone()
        } else if !config.project_root_dir.is_empty() {
            config.project_root_dir.clone()
        } else {
            String::new()
        }
    } else {
        theory_dir
    };

    let effective_output_path = if master_axiom_path.trim().is_empty() {
        if !config.master_axiom_file.is_empty() {
            config.master_axiom_file.clone()
        } else if !effective_theory_dir.is_empty() {
            PathBuf::from(&effective_theory_dir).join("master_axiom.md").to_string_lossy().to_string()
        } else {
            "master_axiom.md".to_string()
        }
    } else {
        master_axiom_path
    };

    let scan = scan_markdown_theory(&effective_theory_dir);
    let fallback_template = build_master_axiom_template(&effective_theory_dir, &effective_output_path, &scan);
    let mut final_content = fallback_template.clone();
    let mut status = "Generated locally from scanned markdown".to_string();

    if let Some(ai_content) = try_generate_with_gemini(&config.gemini_api_key, &effective_theory_dir, &scan, &fallback_template) {
        final_content = ai_content;
        status = "Generated with Gemini".to_string();
    }

    let output_path = PathBuf::from(&effective_output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create output directory: {e}"))?;
    }
    fs::write(&output_path, &final_content).map_err(|e| format!("Failed to write master axiom file: {e}"))?;

    let payload = serde_json::json!({
        "status": status,
        "master_axiom_file": output_path.to_string_lossy().to_string(),
        "files_scanned": scan["files_scanned"],
        "template": final_content
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn list_markdown_files(directory_path: String) -> Result<String, String> {
    let files = collect_markdown_files(&directory_path).map_err(|e| e.to_string())?;
    Ok(serde_json::to_string(&files).map_err(|e| e.to_string())?)
}

#[tauri::command]
fn render_manuscript(
    mode: String,
    source_file: String,
    files: Vec<String>,
    output_dir: String,
    format: String,
    use_for_training: bool,
    source_dir: Option<String>,
) -> Result<String, String> {
    let result = render_manuscript_content(&mode, &source_file, &files, &output_dir, &format, use_for_training, source_dir.as_deref()).map_err(|e| e.to_string())?;
    Ok(result.to_string())
}

#[tauri::command]
fn launch_file_editor(payload: LaunchFileEditorPayload) -> Result<String, String> {
    let (program, args) = build_file_editor_command(&payload)?;
    let mut cmd = std::process::Command::new(&program);
    cmd.args(&args);

    cmd.spawn()
        .map_err(|e| format!("Failed to launch {} with args {:?}: {}", program, args, e))?;

    Ok(format!("Editor successfully launched with {}: {}", program, payload.file_path))
}

#[tauri::command]
fn detach_terminal_shell(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<String, String> {
    // 1. Grab the current active workspace directory
    let current_path = state.workspace_path.lock().map_err(|_| "Mutex poisoned")?.clone();
    
    // 2. Default to a standard Linux terminal emulator if none is set
    let mut term_app = "x-terminal-emulator".to_string(); 
    
    // 3. Read the user's config to see if they specified a custom terminal (e.g., 'alacritty')
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = std::fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&data) {
                if !config.terminal_app.is_empty() {
                    term_app = config.terminal_app;
                }
            }
        }
    }

    // 4. Determine where to open the terminal (fallback to current dir if no workspace loaded)
    let path_to_open = if current_path.is_empty() { "." } else { &current_path };

    // 5. Spawn the native OS process natively detached from the Tauri app
    std::process::Command::new(&term_app)
        .current_dir(path_to_open)
        .spawn()
        .map_err(|e| format!("Failed to launch terminal '{}'. Is it installed? Error: {}", term_app, e))?;

    Ok(format!("Detached native terminal ({}) launched at {:?}", term_app, path_to_open))
}

#[tauri::command]
#[allow(non_snake_case)]
fn get_version_tags(rootPath: String) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .arg("tag")
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Split the output by lines, trim whitespace, and ignore empty lines
        let tags: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(tags)
    } else {
        Err("Failed to fetch tags. Is this folder an active repository?".to_string())
    }
}

#[tauri::command]
fn save_as_version(tag: String, root_path: String) -> Result<String, String> {
    let src = std::path::Path::new(&root_path);

    if !src.exists() {
        return Err(format!("Source path does not exist: {}", root_path));
    }

    if !src.is_dir() {
        return Err(format!("Source path is not a directory: {}", root_path));
    }

    let src = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let parent = src.parent().ok_or_else(|| "Invalid parent directory".to_string())?;
    let folder_name = src.file_name().ok_or_else(|| "Invalid file name".to_string())?;
    let dest_dir = parent.join(format!("{}_{}", folder_name.to_string_lossy(), tag));

    if dest_dir.exists() {
        std::fs::remove_dir_all(&dest_dir).map_err(|e| format!("Failed to replace existing version folder: {}", e))?;
    }

    fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if matches!(name.as_ref(), ".git" | "target" | "node_modules" | "dist" | "build" | ".venv" | "venv" | "__pycache__" | ".idea" | ".vscode") {
                continue;
            }

            let file_type = entry.file_type()?;
            let entry_path = entry.path();
            let dest_path = dst.join(&file_name);

            if file_type.is_symlink() {
                continue;
            } else if file_type.is_dir() {
                copy_recursive(&entry_path, &dest_path)?;
            } else if file_type.is_file() {
                std::fs::copy(&entry_path, &dest_path)?;
            }
        }
        Ok(())
    }

    copy_recursive(&src, &dest_dir)
        .map_err(|e| format!("Failed to save version: {}", e))?;

    Ok(format!("Version '{}' saved successfully to {:?}", tag, dest_dir))
}

#[tauri::command]
#[allow(non_snake_case)]
fn save_equation_to_md(content: String, path: String) -> Result<String, String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {e}"))?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(format!("Equation saved successfully to {}", path))
}

#[tauri::command]
fn save_scratchpad_content(content: String, path: String) -> Result<String, String> {
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {e}"))?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(format!("Scratchpad saved successfully to {}", path))
}

fn build_empirical_analysis_primer(request: &EmpiricalAnalysisRequest) -> String {
    let dataset = if request.dataset_path.trim().is_empty() {
        "unspecified dataset".to_string()
    } else {
        request.dataset_path.trim().to_string()
    };

    let instrument = if request.instrument.trim().is_empty() {
        "unspecified instrument".to_string()
    } else {
        request.instrument.trim().to_string()
    };

    let method = if request.observation_method.trim().is_empty() {
        "unspecified observation method".to_string()
    } else {
        request.observation_method.trim().to_string()
    };

    let hypothesis = if request.hypothesis.trim().is_empty() {
        "No formal hypothesis supplied yet.".to_string()
    } else {
        request.hypothesis.trim().to_string()
    };

    let target = if request.target_variable.trim().is_empty() {
        "a quantity of interest".to_string()
    } else {
        request.target_variable.trim().to_string()
    };

    let mode = if request.primer_mode.trim().is_empty() {
        "focused".to_string()
    } else {
        request.primer_mode.trim().to_string()
    };

    format!(
        "# Empirical Analysis Primer\n\n## Objective\n- Dataset: {dataset}\n- Instrument: {instrument}\n- Observation method: {method}\n- Target variable: {target}\n- Mode: {mode}\n\n## Working Hypothesis\n{hypothesis}\n\n## Analysis Plan\n1. Define the measurement window and selection criteria.\n2. Record calibration, uncertainties, and data provenance.\n3. Compare the empirical signal against the current cosmological model.\n4. Flag discrepancies, residuals, and model updates for the human user.\n5. Summarize the fit quality and propose the next experiment.\n\n## AI Cooperation Structure\n- Analytical thread: fit the model, quantify residuals, and test robustness.\n- Creative thread: interpret the result, generate alternative hypotheses, and suggest follow-up observations.\n- Human steering: approve the analysis target, select cuts, and judge whether the conclusion is physically meaningful.\n\n## Deliverables\n- A short note summarizing the fit quality.\n- A comparison between predicted and observed values.\n- A follow-up question or next experiment to pursue."
    )
}

#[tauri::command]
fn compute_cosmology_metrics_command(h0: f64, omega_m: f64, omega_l: f64, omega_r: f64, z: f64) -> Result<CosmologyMetrics, String> {
    compute_cosmology_metrics(h0, omega_m, omega_l, omega_r, z)
}

#[tauri::command]
fn generate_empirical_analysis_primer(
    request: EmpiricalAnalysisRequest,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    let primer_content = build_empirical_analysis_primer(&request);

    let workspace_root = if request.workspace_path.trim().is_empty() {
        state.workspace_path.lock().map_err(|_| "Mutex poisoned")?.clone()
    } else {
        request.workspace_path.clone()
    };

    let target_path = if workspace_root.is_empty() {
        std::path::PathBuf::from("empirical_analysis_primer.md")
    } else {
        std::path::PathBuf::from(&workspace_root).join("empirical_analysis_primer.md")
    };

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create primer directory: {e}"))?;
    }

    std::fs::write(&target_path, &primer_content).map_err(|e| format!("Failed to save primer: {e}"))?;

    let payload = serde_json::json!({
        "path": target_path.to_string_lossy().to_string(),
        "content": primer_content
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn save_user_settings(payload: SaveUserSettingsPayload, app: tauri::AppHandle) -> Result<String, String> {
    let config_path = get_config_path(&app)?;

    let mut config = if let Ok(data) = std::fs::read_to_string(&config_path) {
        serde_json::from_str::<AppConfig>(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    config.editor = payload.editor;
    config.terminal_app = payload.terminal_app;
    config.gemini_api_key = payload.gemini_key;
    config.openai_api_key = payload.openai_key;
    config.left_provider = payload.left_provider;
    config.left_model = payload.left_model;
    config.right_provider = payload.right_provider;
    config.right_model = payload.right_model;
    config.project_root_dir = payload.project_root_dir;
    config.theory_md_dir = payload.theory_md_dir;
    config.master_axiom_file = payload.master_axiom_file;
    config.tools_dir = payload.tools_dir;
    config.theme = payload.theme;
    config.custom_accent = payload.custom_accent;
    config.custom_bg_panel = payload.custom_bg_panel;
    config.left_preserve_thread_history = payload.left_preserve_thread_history;
    config.right_preserve_thread_history = payload.right_preserve_thread_history;

    let config_json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, config_json).map_err(|e| e.to_string())?;

    Ok("Settings saved successfully".to_string())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_flat_cosmology_metrics_for_zero_redshift() {
        let metrics = compute_cosmology_metrics(70.0, 0.3, 0.7, 0.0, 0.0).unwrap();
        assert!((metrics.scale_factor - 1.0).abs() < 1e-9);
        assert!(metrics.e_of_z > 0.0);
        assert!(metrics.comoving_distance_mpc >= 0.0);
    }

    #[test]
    fn rejects_negative_redshift() {
        let result = compute_cosmology_metrics(70.0, 0.3, 0.7, 0.0, -0.1);
        assert!(result.is_err());
    }

    #[test]
    fn normalizes_unsupported_gemini_models_to_supported_flash_models() {
        assert_eq!(normalize_model_for_provider("gemini", "gemini-2.0-flash"), "gemini-2.0-flash");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-2.0-flash-lite"), "gemini-2.0-flash-lite");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-3-flash"), "gemini-2.0-flash");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-3.6-flash"), "gemini-2.0-flash-lite");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-1.5-pro"), "gemini-2.0-flash-lite");
    }

    #[test]
    fn keeps_explicit_openai_model_selection_when_supported() {
        assert_eq!(normalize_model_for_provider("openai", "gpt-4o"), "gpt-4.1");
        assert_eq!(normalize_model_for_provider("openai", "gpt-4o-mini"), "gpt-4.1-mini");
        assert_eq!(normalize_model_for_provider("openai", "gpt-4"), "gpt-4.1");
    }

    #[test]
    fn builds_gemini_contents_from_chat_history() {
        let history = vec![
            serde_json::json!({"role": "system", "content": "You are a helpful assistant"}),
            serde_json::json!({"role": "user", "parts": [{"text": "Summarize the theory"}]}),
            serde_json::json!({"role": "assistant", "content": "A concise summary"}),
        ];

        let body = build_gemini_request_body(&history).unwrap();
        let contents = body.get("contents").and_then(|value| value.as_array()).unwrap();

        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0].get("role").and_then(|value| value.as_str()), Some("user"));
        assert_eq!(contents[1].get("role").and_then(|value| value.as_str()), Some("user"));
        assert_eq!(contents[2].get("role").and_then(|value| value.as_str()), Some("model"));
    }

    #[test]
    fn builds_scientific_master_axiom_template_from_markdown_scan() {
        let temp_dir = std::env::temp_dir().join("physics_ide_master_axiom_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("chapter1.md"),
            "# Chapter 1\n\nThe theory uses a Lagrangian.\n\n$$\\mathcal{L} = \\frac{1}{2}\\partial_\\mu \\phi \\partial^\\mu \\phi - V(\\phi)$$\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("abstract.md"),
            "## Abstract\n\nWe hypothesize that this theory fits the data.\n",
        )
        .unwrap();

        let scan = scan_markdown_theory(temp_dir.to_str().unwrap());
        assert_eq!(scan["files_scanned"].as_u64().unwrap(), 2);
        assert!(scan["lagrangian_candidates"].as_array().unwrap().len() >= 1);

        let template = build_master_axiom_template(temp_dir.to_str().unwrap(), "", &scan);
        assert!(template.contains("## Hypothesis"));
        assert!(template.contains("## Predictions"));
        assert!(template.contains("## Observational Consequences"));
    }

    #[test]
    fn classifies_left_field_theory_as_non_mainstream() {
        let temp_dir = std::env::temp_dir().join("physics_ide_left_field_style_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        fs::write(
            temp_dir.join("bmi.md"),
            "# BMI Theory\n\nThis model uses a bimodal manifold interaction framework with emergent constraint operators and seam stress.\n",
        )
        .unwrap();

        let scan = scan_markdown_theory(temp_dir.to_str().unwrap());
        let style = detect_theory_style(&scan);
        let template = build_master_axiom_template(temp_dir.to_str().unwrap(), "", &scan);

        assert_eq!(style, "left_field");
        assert!(template.contains("## Structural Assumptions"));
    }

    #[test]
    fn imports_plaintext_manuscript_into_markdown_sections() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_import_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let source_path = temp_dir.join("manuscript.txt");
        fs::write(
            &source_path,
            "Chapter 1: Foundations\n\nThis chapter introduces the model.\n\nSection 1.1: Core claim\n\nThe theory uses a metric relation and a state variable.\n\n$$\\mathcal{L} = \\frac{1}{2}\\partial_\\mu \\phi \\partial^\\mu \\phi - V(\\phi)$$\n",
        )
        .unwrap();

        let output_dir = temp_dir.join("imported");
        let result = import_theory_source(&source_path, &output_dir).unwrap();

        assert_eq!(result["source_type"].as_str().unwrap(), "manuscript");
        assert!(output_dir.join("chapter_1_foundations.md").exists());
        assert!(output_dir.join("equations.md").exists());
        assert!(result["files_created"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn save_as_version_skips_git_and_generated_directories() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_version_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let source_dir = temp_dir.join("source");
        fs::create_dir_all(source_dir.join(".git/objects")).unwrap();
        fs::create_dir_all(source_dir.join("build/output")).unwrap();
        fs::write(source_dir.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        fs::write(source_dir.join("notes.md"), "snapshot me").unwrap();
        fs::write(source_dir.join("build/output/ignored.txt"), "ignore me").unwrap();

        let result = save_as_version("v1.0.0".to_string(), source_dir.to_string_lossy().to_string()).unwrap();
        let snapshot_dir = temp_dir.join("source_v1.0.0");

        assert!(snapshot_dir.join("notes.md").exists());
        assert!(!snapshot_dir.join(".git").exists());
        assert!(!snapshot_dir.join("build").exists());
        assert!(result.contains("saved successfully"));
    }

    #[test]
    fn extract_evidence_snippet_handles_multibyte_characters() {
        let content = "A short intro with ✅ emoji before the analysis topic and some more words to pad the excerpt.";
        let snippet = extract_evidence_snippet(content, &["analysis".to_string()], 120);
        assert!(snippet.contains("analysis"));
        assert!(!snippet.is_empty());
    }

    #[test]
    fn builds_project_awareness_markdown_from_theory_and_tools() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_awareness_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir.join("theory")).unwrap();
        fs::create_dir_all(&temp_dir.join("tools")).unwrap();

        fs::write(
            temp_dir.join("theory").join("chapter_one.md"),
            "# Chapter One\n\nThis section explores the data and analysis workflow.\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("tools").join("analyze_data.py"),
            "# Analyze data\n\nprint('hello')\n",
        )
        .unwrap();

        let scan = scan_markdown_theory(temp_dir.join("theory").to_str().unwrap());
        let awareness = build_project_awareness_markdown(
            &temp_dir,
            temp_dir.join("theory").to_str().unwrap(),
            &temp_dir.join("master_axiom.md"),
            temp_dir.join("tools").to_str().unwrap(),
            &scan,
        );

        assert!(awareness.contains("## Theory Topic Map"));
        assert!(awareness.contains("Chapter One"));
        assert!(awareness.contains("analyze_data.py"));
        assert!(awareness.contains("Ranked Retrieval Hints"));
        assert!(awareness.contains("shared terms"));
    }

    #[test]
    fn deserialize_save_user_settings_payload_accepts_camel_and_snake_case() {
        let camel_payload = serde_json::json!({
            "editor": "vim",
            "terminalApp": "gnome-terminal",
            "geminiKey": "key",
            "openaiKey": "openai",
            "leftProvider": "gemini",
            "leftModel": "gemini-2.0-flash",
            "rightProvider": "openai",
            "rightModel": "gpt-4.1-mini",
            "projectRootDir": "/tmp/project",
            "theoryMdDir": "/tmp/theory",
            "masterAxiomFile": "/tmp/master.md",
            "theme": "dark",
            "customAccent": "#33d17a",
            "customBgPanel": "#2f4f3f",
            "leftPreserveThreadHistory": false,
            "rightPreserveThreadHistory": false
        });

        let snake_payload = serde_json::json!({
            "editor": "vim",
            "terminal_app": "gnome-terminal",
            "gemini_key": "key",
            "openai_key": "openai",
            "left_provider": "gemini",
            "left_model": "gemini-2.0-flash",
            "right_provider": "openai",
            "right_model": "gpt-4.1-mini",
            "project_root_dir": "/tmp/project",
            "theory_md_dir": "/tmp/theory",
            "master_axiom_file": "/tmp/master.md",
            "theme": "dark",
            "custom_accent": "#33d17a",
            "custom_bg_panel": "#2f4f3f",
            "left_preserve_thread_history": false,
            "right_preserve_thread_history": false
        });

        let default_history_payload = serde_json::json!({
            "editor": "vim",
            "terminal_app": "gnome-terminal",
            "gemini_key": "key",
            "openai_key": "openai",
            "left_provider": "gemini",
            "left_model": "gemini-2.0-flash",
            "right_provider": "openai",
            "right_model": "gpt-4.1-mini",
            "project_root_dir": "/tmp/project",
            "theory_md_dir": "/tmp/theory",
            "master_axiom_file": "/tmp/master.md",
            "theme": "dark",
            "custom_accent": "#33d17a",
            "custom_bg_panel": "#2f4f3f"
        });

        let camel: SaveUserSettingsPayload = serde_json::from_value(camel_payload).unwrap();
        let snake: SaveUserSettingsPayload = serde_json::from_value(snake_payload).unwrap();
        let defaulted: SaveUserSettingsPayload = serde_json::from_value(default_history_payload).unwrap();

        assert_eq!(camel.terminal_app, "gnome-terminal");
        assert_eq!(snake.terminal_app, "gnome-terminal");
        assert_eq!(camel.project_root_dir, "/tmp/project");
        assert_eq!(snake.project_root_dir, "/tmp/project");
        assert!(!camel.left_preserve_thread_history);
        assert!(!camel.right_preserve_thread_history);
        assert!(!snake.left_preserve_thread_history);
        assert!(!snake.right_preserve_thread_history);
        assert!(defaulted.left_preserve_thread_history);
        assert!(defaulted.right_preserve_thread_history);
    }

    #[test]
    fn build_file_editor_command_uses_configured_cli_editor() {
        let payload = LaunchFileEditorPayload {
            file_path: "/tmp/readme.md".to_string(),
            terminal_app: "gnome-terminal".to_string(),
            editor: "micro".to_string(),
        };

        let (program, args) = build_file_editor_command(&payload).unwrap();
        assert_eq!(program, "gnome-terminal");
        assert_eq!(args, vec!["--", "micro", "/tmp/readme.md"]);
    }

    #[test]
    fn build_file_editor_command_falls_back_to_xdg_open_when_editor_missing() {
        let payload = LaunchFileEditorPayload {
            file_path: "/tmp/readme.md".to_string(),
            ..Default::default()
        };

        let (program, args) = build_file_editor_command(&payload).unwrap();
        assert_eq!(program, "xdg-open");
        assert_eq!(args, vec!["/tmp/readme.md"]);
    }

    #[test]
    fn builds_empirical_analysis_primer_with_dataset_context() {
        let request = EmpiricalAnalysisRequest {
            dataset_path: "/data/cms_run.csv".to_string(),
            instrument: "CMS".to_string(),
            observation_method: "Collision reconstruction".to_string(),
            hypothesis: "A feature in the dijet spectrum reflects a new cosmological signature.".to_string(),
            target_variable: "Invariant mass".to_string(),
            workspace_path: "/tmp/physics-ide".to_string(),
            primer_mode: "focused".to_string(),
        };

        let primer = build_empirical_analysis_primer(&request);
        assert!(primer.contains("Empirical Analysis Primer"));
        assert!(primer.contains("CMS"));
        assert!(primer.contains("Invariant mass"));
        assert!(primer.contains("Collision reconstruction"));
    }
}

// --- MAIN RUNNER ---

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Setup block runs before the window opens to seed our runtime AppState
        .setup(|app| {
            // Read the JSON file to grab the last known workspace path
            let initial_path = match get_config_path(app.handle()) {
                Ok(config_path) => {
                    if let Ok(data) = fs::read_to_string(config_path) {
                        let config: AppConfig = serde_json::from_str(&data).unwrap_or_default();
                        config.last_root_dir
                    } else {
                        String::new()
                    }
                },
                Err(_) => String::new(),
            };

            // Inject the persistent path into Tauri's fast runtime State
            app.manage(AppState {
                workspace_path: std::sync::Mutex::new(initial_path),
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            save_root_directory,
            save_user_settings,
            read_directory,
            save_as_version,
            restore_version,
            save_as_hypothesis,
            get_version_tags,
            save_equation_to_md,
            save_scratchpad_content,
            compute_cosmology_metrics_command,
            generate_empirical_analysis_primer,
            export_workspace_tree,
            send_llm_prompt,
            compile_ai_briefing,
            import_theory_source_command,
            generate_master_axiom_from_theory,
            list_markdown_files,
            collect_probe_evidence,
            render_manuscript,
            launch_file_editor,
            detach_terminal_shell,
            git_pull,
            git_push,
            prepare_exit_session,
            generate_exit_session_draft,
            get_master_axiom_snapshot
            // Add your other commands here...
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
