use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Key, Nonce};
use pbkdf2::pbkdf2_hmac;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Once;
use tauri::{AppHandle, Manager};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    pub llm_usage: std::sync::Mutex<std::collections::HashMap<String, ThreadUsageState>>,
    embedding_model: std::sync::Mutex<Option<fastembed::TextEmbedding>>,
}

#[derive(Debug, Clone, Serialize, Default)]
struct ProviderUsage {
    provider: String,
    model: String,
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

struct ProviderReply {
    content: String,
    usage: ProviderUsage,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ThreadUsageState {
    request_count: u64,
    cached_input_report_count: u64,
    cache_reported_input_tokens: u64,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_tokens: u64,
    total_tokens: u64,
    last: ProviderUsage,
}

impl ThreadUsageState {
    fn record(&mut self, usage: &ProviderUsage) {
        self.request_count += 1;
        if usage.cached_input_tokens.is_some() {
            self.cached_input_report_count += 1;
            self.cache_reported_input_tokens += usage.input_tokens.unwrap_or(0);
        }
        self.input_tokens += usage.input_tokens.unwrap_or(0);
        self.cached_input_tokens += usage.cached_input_tokens.unwrap_or(0);
        self.output_tokens += usage.output_tokens.unwrap_or(0);
        self.reasoning_tokens += usage.reasoning_tokens.unwrap_or(0);
        self.total_tokens += usage.total_tokens.unwrap_or(0);
        self.last = usage.clone();
    }
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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

fn default_ai_file_access_mode() -> String {
    "disabled".to_string()
}

const ENCRYPTED_SECRET_PREFIX: &str = "enc:v1:";
const LOCAL_SECRET_FILE_NAME: &str = "ai-secret-store.key";
const LOCAL_SECRET_SALT: &[u8] = b"physics-ide-ai-secrets-v1";

fn normalize_api_key(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let _ = write!(&mut encoded, "%{:02X}", byte);
            }
        }
    }
    encoded
}

fn derive_aes_key_from_master_secret(master_secret: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(master_secret.as_bytes(), LOCAL_SECRET_SALT, 200_000, &mut key);
    key
}

fn encrypt_secret_with_key(secret: &str, master_secret: &str) -> Result<String, String> {
    if secret.trim().is_empty() {
        return Ok(String::new());
    }

    let key = derive_aes_key_from_master_secret(master_secret);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, secret.as_bytes().as_ref())
        .map_err(|e| format!("Failed to encrypt secret: {e}"))?;

    Ok(format!(
        "{}{}:{}",
        ENCRYPTED_SECRET_PREFIX,
        base64::encode(&nonce_bytes),
        base64::encode(&ciphertext)
    ))
}

fn decrypt_secret_with_key(encrypted_secret: &str, master_secret: &str) -> Result<String, String> {
    if encrypted_secret.trim().is_empty() {
        return Ok(String::new());
    }

    if !encrypted_secret.starts_with(ENCRYPTED_SECRET_PREFIX) {
        return Ok(encrypted_secret.to_string());
    }

    let payload = encrypted_secret
        .strip_prefix(ENCRYPTED_SECRET_PREFIX)
        .ok_or_else(|| "Malformed encrypted secret payload".to_string())?;
    let (nonce_b64, ciphertext_b64) = payload
        .split_once(':')
        .ok_or_else(|| "Malformed encrypted secret payload".to_string())?;

    let key = derive_aes_key_from_master_secret(master_secret);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let nonce_bytes = base64::decode(nonce_b64)
        .map_err(|e| format!("Failed to decode secret nonce: {e}"))?;
    let ciphertext_bytes = base64::decode(ciphertext_b64)
        .map_err(|e| format!("Failed to decode secret payload: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext_bytes.as_ref())
        .map_err(|e| format!("Failed to decrypt secret: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| format!("Failed to decode decrypted secret text: {e}"))
}

fn get_secret_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(LOCAL_SECRET_FILE_NAME);
    Ok(path)
}

fn load_or_create_master_secret(app: &AppHandle) -> Result<String, String> {
    let path = get_secret_store_path(app)?;
    if path.exists() {
        let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = base64::encode(&bytes);
    fs::write(&path, &secret).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&path, permissions).map_err(|e| e.to_string())?;
    }

    Ok(secret)
}

fn encrypt_secret_for_storage(secret: &str, app: &AppHandle) -> Result<String, String> {
    let master_secret = load_or_create_master_secret(app)?;
    encrypt_secret_with_key(secret, &master_secret)
}

fn decrypt_secret_from_storage(encrypted_secret: &str, app: &AppHandle) -> Result<String, String> {
    let master_secret = load_or_create_master_secret(app)?;
    decrypt_secret_with_key(encrypted_secret, &master_secret)
}

fn deserialize_app_config_from_storage(data: &str, app: &AppHandle) -> Result<AppConfig, String> {
    let mut config = serde_json::from_str::<AppConfig>(data).unwrap_or_default();
    config.gemini_api_key = decrypt_secret_from_storage(&config.gemini_api_key, app)?;
    config.openai_api_key = decrypt_secret_from_storage(&config.openai_api_key, app)?;
    config.github_api_key = decrypt_secret_from_storage(&config.github_api_key, app)?;
    Ok(config)
}

fn serialize_app_config_for_storage(config: &AppConfig, app: &AppHandle) -> Result<String, String> {
    let mut config_for_disk = config.clone();
    config_for_disk.gemini_api_key = encrypt_secret_for_storage(&config.gemini_api_key, app)?;
    config_for_disk.openai_api_key = encrypt_secret_for_storage(&config.openai_api_key, app)?;
    config_for_disk.github_api_key = encrypt_secret_for_storage(&config.github_api_key, app)?;
    serde_json::to_string_pretty(&config_for_disk).map_err(|e| e.to_string())
}

fn load_app_config(app: &AppHandle) -> Result<AppConfig, String> {
    let config_path = get_config_path(app)?;
    if let Ok(data) = fs::read_to_string(&config_path) {
        return deserialize_app_config_from_storage(&data, app);
    }

    Ok(AppConfig::default())
}

fn update_workspace_root_in_config(config: &mut AppConfig, workspace_path: &str) {
    let trimmed = workspace_path.trim();
    if trimmed.is_empty() {
        return;
    }

    let normalized = normalize_api_key(trimmed);
    config.last_root_dir = normalized.clone();
    config.project_root_dir = normalized.clone();

    if config.theory_md_dir.trim().is_empty() {
        config.theory_md_dir = normalized.clone();
    }
}

// --- PERSISTENT DISK CONFIGURATION ---
#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    last_root_dir: String,
    editor: String,
    terminal_app: String,
    gemini_api_key: String,
    openai_api_key: String,
    github_username: String,
    github_api_key: String,
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
    #[serde(default = "default_ai_file_access_mode")]
    ai_file_access_mode: String,
    reuse_notes_next_session: bool,
    first_session_completed: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TheoryProfilesStore {
    active_profile: String,
    profiles: BTreeMap<String, AppConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_root_dir: String::new(),
            editor: String::new(),
            terminal_app: String::new(),
            gemini_api_key: String::new(),
            openai_api_key: String::new(),
            github_username: String::new(),
            github_api_key: String::new(),
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
            ai_file_access_mode: default_ai_file_access_mode(),
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
    #[serde(default, alias = "githubUsername")]
    pub github_username: String,
    #[serde(default, alias = "githubApiKey")]
    pub github_api_key: String,
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
    #[serde(default = "default_ai_file_access_mode", alias = "aiFileAccessMode")]
    pub ai_file_access_mode: String,
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

#[derive(Debug, Clone)]
struct RetrievalChunk {
    id: String,
    path: String,
    chunk_index: usize,
    line_start: usize,
    line_end: usize,
    heading: String,
    content: String,
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
    // Keep startup blank for first-session mode so a theory can be truly closed.
    if !config.first_session_completed {
        return;
    }

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

#[cfg(test)]
fn build_workspace_tree_string(root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("Root path does not exist: {}", root.to_string_lossy()));
    }

    fn collect_tree_entries(dir: &Path, base_dir: &Path, output: &mut Vec<String>) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        for entry in entries {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            if name_str == "target" || name_str.starts_with('.') {
                continue;
            }

            let entry_path = entry.path();
            let relative_path = entry_path
                .strip_prefix(base_dir)
                .unwrap_or(&entry_path)
                .to_string_lossy()
                .replace('\\', "/");
            let normalized_path = relative_path.trim_start_matches('/');

            if entry.file_type()?.is_dir() {
                output.push(format!("/{normalized_path}/"));
                collect_tree_entries(&entry_path, base_dir, output)?;
            } else {
                output.push(format!("/{normalized_path}"));
            }
        }

        Ok(())
    }

    let mut tree_entries = Vec::new();
    collect_tree_entries(root, root, &mut tree_entries).map_err(|e| e.to_string())?;

    let root_label = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    let mut tree_string = format!("# Workspace Tree\n\n## Root\n- {}\n\n## Entries\n", root_label);
    for entry in tree_entries {
        tree_string.push_str(&format!("- {}\n", entry));
    }

    Ok(tree_string)
}

fn build_compact_workspace_root(root: &Path) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("Root path does not exist: {}", root.to_string_lossy()));
    }

    let root_label = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    Ok(format!("@tree-v1\n{root_label}/\n"))
}

fn workspace_tree_for_export(root: &Path, visible_tree: Option<&str>) -> Result<String, String> {
    if !root.exists() {
        return Err(format!("Root path does not exist: {}", root.to_string_lossy()));
    }

    let Some(candidate) = visible_tree.map(str::trim).filter(|tree| !tree.is_empty()) else {
        return build_compact_workspace_root(root);
    };

    if !candidate.starts_with("@tree-v1\n") {
        return Err("Visible workspace tree must use the @tree-v1 format.".to_string());
    }
    if candidate.len() > 200_000 {
        return Err("Visible workspace tree exceeds the 200 KB export limit.".to_string());
    }

    Ok(format!("{candidate}\n"))
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

fn describe_ai_file_access_mode(mode: &str) -> String {
    let normalized = mode.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "read_write" => "AI file access status: enabled for read/write access within the active workspace root only.".to_string(),
        "read" | "read_only" => "AI file access status: enabled for read-only access within the active workspace root only.".to_string(),
        _ => "AI file access status: disabled. The AI may reason over project context but cannot read or edit workspace files unless access is enabled in Settings.".to_string(),
    }
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
    ai_file_access_mode: &str,
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
    let file_access_status = describe_ai_file_access_mode(ai_file_access_mode);

    format!(
        "# AI Briefing Packet\n\n## Session Summary\n{}\n\n## Sources\n- Primer: {}\n- Session recap: {}\n- Workspace tree: {}\n- Master axiom: {}\n\n## Master Axiom Snapshot\n```md\n{}\n```\n\n## Startup Guidance\n- Use this packet to resume collaboration without replaying full history.\n- Anchor reasoning in the active axioms and assumptions before proposing new branches.\n- Keep responses concise, physically grounded, and explicit about uncertainty.\n- {}\n- When the user references a chapter, experiment, or tool, use the project awareness index and the thread focus to locate the most relevant files before answering.\n\n## Project Awareness Index\n```md\n{}\n```\n\n## Thread Retrieval Hints\n{}\n\n## Context Notes\n- Workspace root: {}\n- Equation continuity key: $\\mathcal{{L}}$, boundary constraints, and observational consequences should remain traceable across branch updates.\n",
        summary,
        primer_path.to_string_lossy(),
        recap_path.to_string_lossy(),
        tree_path.to_string_lossy(),
        master_axiom_path.to_string_lossy(),
        axiom_excerpt,
        file_access_status,
        awareness_markdown,
        thread_context_section,
        project_root.to_string_lossy()
    )
}

fn build_first_session_briefing_markdown(project_root: &Path, ai_file_access_mode: &str) -> String {
    format!(
        "# First Session Briefing Packet\n\n## Welcome\n- Greet the user and explain that this first run will establish the project context for future sessions.\n- Ask for the theory/model title so the session language remains aligned with the user\'s framework.\n\n## What the Primer Is\n- In this app, a \"primer\" and the \"briefing packet\" are the same practical concept: a compact context document for AI lanes.\n- The entity being briefed is the AI.\n- Purpose: keep AI aware of your current project state, assumptions, goals, and recent progress without replaying full chat history every time.\n- If you maintain this packet well, continuity stays strong across long sessions and across days.\n\n## Setup Checklist\n1. Import or open the project workspace folder.\n2. Confirm the theory markdown output directory in settings.\n3. Set or generate the master axiom file path.\n4. Save starter notes describing today\'s goals in the scratchpad.\n5. Export the workspace tree so source structure is visible.\n6. Build or refresh the briefing packet and verify both AI lanes received it.\n\n## Assistant Behavior\n- Offer step-by-step guidance instead of waiting idle.\n- Keep prompts concise and practical for first-session setup.\n- Ask one clarifying question at a time when configuration details are missing.\n- Explain setup terms briefly when needed (for example: primer, master axiom, theory markdown folder).\n- Remind the user that end-of-session recap can produce the next briefing packet automatically.\n- {}\n\n## Expected Outcome\n- By the end of this first session, documentation should be strong enough to replace this starter packet with a session-specific packet.\n\n## Workspace Context\n- Project root: {}\n- Note: this starter packet is intended for first-run onboarding only.\n",
        describe_ai_file_access_mode(ai_file_access_mode),
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
    let config_json = serialize_app_config_for_storage(config, app)?;
    let config_path = get_config_path(app)?;
    fs::write(config_path, config_json).map_err(|e| e.to_string())
}

fn get_theory_profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push("theory_profiles.json");
    Ok(path)
}

fn load_theory_profiles_store(app: &AppHandle) -> Result<TheoryProfilesStore, String> {
    let path = get_theory_profiles_path(app)?;
    if !path.exists() {
        return Ok(TheoryProfilesStore::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut store = serde_json::from_str::<TheoryProfilesStore>(&contents).unwrap_or_default();
    for profile in store.profiles.values_mut() {
        *profile = deserialize_app_config_from_storage(
            &serde_json::to_string(profile).map_err(|e| e.to_string())?,
            app,
        )?;
    }
    Ok(store)
}

fn save_theory_profiles_store(app: &AppHandle, store: &TheoryProfilesStore) -> Result<(), String> {
    let path = get_theory_profiles_path(app)?;
    let mut disk_store = store.clone();
    for profile in disk_store.profiles.values_mut() {
        let json = serialize_app_config_for_storage(profile, app)?;
        *profile = serde_json::from_str::<AppConfig>(&json).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&disk_store).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn normalize_profile_name(raw_name: &str) -> Result<String, String> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        return Err("Profile name cannot be empty".to_string());
    }
    if trimmed.len() > 120 {
        return Err("Profile name is too long (max 120 characters)".to_string());
    }
    Ok(trimmed.to_string())
}

#[derive(Clone)]
struct HelpDocRecord {
    id: &'static str,
    title: &'static str,
    path: &'static str,
    content: &'static str,
}

fn help_docs_catalog() -> Vec<HelpDocRecord> {
    vec![
        HelpDocRecord {
            id: "app-layers",
            title: "App Layers",
            path: "docs/help/app-layers.md",
            content: include_str!("../../docs/help/app-layers.md"),
        },
        HelpDocRecord {
            id: "gui-button-glossary",
            title: "GUI Button Glossary",
            path: "docs/help/gui-button-glossary.md",
            content: include_str!("../../docs/help/gui-button-glossary.md"),
        },
        HelpDocRecord {
            id: "push-pull-context-and-errors",
            title: "Push/Pull Context and Common Errors",
            path: "docs/help/push-pull-context-and-errors.md",
            content: include_str!("../../docs/help/push-pull-context-and-errors.md"),
        },
        HelpDocRecord {
            id: "startup-initial-setup-checklist",
            title: "Startup Initial Setup Workflow and Checklist",
            path: "docs/help/startup-initial-setup-checklist.md",
            content: include_str!("../../docs/help/startup-initial-setup-checklist.md"),
        },
    ]
}

fn normalize_help_query_tokens(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

fn find_help_doc_by_id(doc_id: &str) -> Option<HelpDocRecord> {
    let normalized = doc_id.trim().to_lowercase();
    help_docs_catalog()
        .into_iter()
        .find(|doc| doc.id == normalized)
}

fn bounded_levenshtein(a: &str, b: &str, max_distance: usize) -> Option<usize> {
    if a == b {
        return Some(0);
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.is_empty() {
        return (b_chars.len() <= max_distance).then_some(b_chars.len());
    }
    if b_chars.is_empty() {
        return (a_chars.len() <= max_distance).then_some(a_chars.len());
    }

    let len_a = a_chars.len();
    let len_b = b_chars.len();
    if len_a.abs_diff(len_b) > max_distance {
        return None;
    }

    let mut prev: Vec<usize> = (0..=len_b).collect();
    let mut curr: Vec<usize> = vec![0; len_b + 1];

    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;

        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            let deletion = prev[j + 1] + 1;
            let insertion = curr[j] + 1;
            let substitution = prev[j] + cost;
            let best = deletion.min(insertion).min(substitution);
            curr[j + 1] = best;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    let distance = prev[len_b];
    (distance <= max_distance).then_some(distance)
}

fn fuzzy_match_distance(token: &str, text: &str) -> Option<usize> {
    if token.is_empty() {
        return None;
    }

    let max_distance = if token.len() <= 4 { 1 } else { 2 };
    let mut best: Option<usize> = None;

    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if word.len().abs_diff(token.len()) > max_distance {
            continue;
        }

        if let Some(distance) = bounded_levenshtein(token, word, max_distance) {
            match best {
                Some(current) if current <= distance => {}
                _ => best = Some(distance),
            }
            if distance == 0 {
                break;
            }
        }
    }

    best
}

fn compute_help_doc_score(doc: &HelpDocRecord, tokens: &[String]) -> i32 {
    if tokens.is_empty() {
        return 0;
    }

    let title = doc.title.to_lowercase();
    let content = doc.content.to_lowercase();
    let mut score = 0;

    for token in tokens {
        let mut exact_hit = false;

        if title.contains(token) {
            score += 7;
            exact_hit = true;
        }
        if content.contains(token) {
            score += 3;
            exact_hit = true;
        }

        if token.ends_with('s') && token.len() > 2 {
            let singular = &token[..token.len() - 1];
            if title.contains(singular) {
                score += 4;
                exact_hit = true;
            }
            if content.contains(singular) {
                score += 2;
                exact_hit = true;
            }
        }

        if !exact_hit {
            if let Some(distance) = fuzzy_match_distance(token, &title) {
                score += match distance {
                    0 => 6,
                    1 => 4,
                    _ => 2,
                };
            }

            if let Some(distance) = fuzzy_match_distance(token, &content) {
                score += match distance {
                    0 => 2,
                    1 => 1,
                    _ => 1,
                };
            }
        }
    }

    score
}

fn build_help_doc_snippet(content: &str, tokens: &[String]) -> String {
    let lowered = content.replace('\n', " ").to_lowercase();

    let mut hit_index = 0usize;
    for token in tokens {
        if let Some(index) = lowered.find(token) {
            hit_index = index;
            break;
        }
        if token.ends_with('s') && token.len() > 2 {
            let singular = &token[..token.len() - 1];
            if let Some(index) = lowered.find(singular) {
                hit_index = index;
                break;
            }
        }
    }

    let start = hit_index.saturating_sub(80);
    let end = (hit_index + 160).min(lowered.len());
    let mut snippet = lowered[start..end].trim().to_string();

    if start > 0 {
        snippet = format!("...{}", snippet);
    }
    if end < lowered.len() {
        snippet.push_str("...");
    }
    snippet
}

fn build_markdown_doc_snippet(content: &str, tokens: &[String]) -> String {
    let lowered = content.replace('\n', " ").to_lowercase();

    let mut hit_index = 0usize;
    for token in tokens {
        if let Some(index) = lowered.find(token) {
            hit_index = index;
            break;
        }
        if token.ends_with('s') && token.len() > 2 {
            let singular = &token[..token.len() - 1];
            if let Some(index) = lowered.find(singular) {
                hit_index = index;
                break;
            }
        }
    }

    let start = hit_index.saturating_sub(80);
    let end = (hit_index + 160).min(lowered.len());
    let mut snippet = lowered[start..end].trim().to_string();

    if start > 0 {
        snippet = format!("...{}", snippet);
    }
    if end < lowered.len() {
        snippet.push_str("...");
    }
    snippet
}

fn collect_markdown_files_recursive(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.exists() || !root.is_dir() {
        return Err(format!("Directory does not exist: {}", root.to_string_lossy()));
    }

    fn scan(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<(), String> {
        let skip_dirs = [
            ".git",
            "node_modules",
            "target",
            "dist",
            "build",
            ".venv",
            "venv",
            "__pycache__",
            ".idea",
            ".vscode",
            ".physics-ide",
            "versions",
        ];

        for entry in fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {e}"))? {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("Failed to read file type: {e}"))?;

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if skip_dirs.iter().any(|skip| skip.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                scan(&path, acc)?;
                continue;
            }

            if file_type.is_file() {
                let is_md = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("md"))
                    .unwrap_or(false);
                if is_md {
                    acc.push(path);
                }
            }
        }

        Ok(())
    }

    let mut files = Vec::new();
    scan(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    if let (Ok(canon_path), Ok(canon_root)) = (path.canonicalize(), root.canonicalize()) {
        canon_path.starts_with(canon_root)
    } else {
        false
    }
}

fn ensure_ai_file_access(config: &AppConfig, action: &str, target_path: &Path, workspace_root: &Path) -> Result<(), String> {
    let requested_mode = config.ai_file_access_mode.trim().to_ascii_lowercase();
    let allow_write = requested_mode == "read_write";
    let allow_read = matches!(requested_mode.as_str(), "read" | "read_only" | "read_write");

    match action {
        "read" if !allow_read => {
            return Err("AI file access is disabled for reads. Enable it in Settings to allow read operations.".to_string());
        }
        "write" if !allow_write => {
            return Err("AI file access is disabled for writes. Enable read/write access in Settings to allow edit operations.".to_string());
        }
        _ => {}
    }

    let target_exists = target_path.exists();
    let normalized_target = if target_exists {
        target_path.canonicalize().unwrap_or_else(|_| target_path.to_path_buf())
    } else {
        target_path.to_path_buf()
    };
    let normalized_root = workspace_root.canonicalize().unwrap_or_else(|_| workspace_root.to_path_buf());

    if !normalized_target.starts_with(&normalized_root) {
        return Err(format!("AI {} operation is blocked because the target is outside the active workspace.", action));
    }

    Ok(())
}

fn compute_markdown_doc_score(title: &str, content: &str, tokens: &[String]) -> i32 {
    if tokens.is_empty() {
        return 0;
    }

    let title_lower = title.to_lowercase();
    let content_lower = content.to_lowercase();
    let mut score = 0;

    for token in tokens {
        let mut exact_hit = false;

        if title_lower.contains(token) {
            score += 7;
            exact_hit = true;
        }
        if content_lower.contains(token) {
            score += 3;
            exact_hit = true;
        }

        if token.ends_with('s') && token.len() > 2 {
            let singular = &token[..token.len() - 1];
            if title_lower.contains(singular) {
                score += 4;
                exact_hit = true;
            }
            if content_lower.contains(singular) {
                score += 2;
                exact_hit = true;
            }
        }

        if !exact_hit {
            if let Some(distance) = fuzzy_match_distance(token, &title_lower) {
                score += match distance {
                    0 => 6,
                    1 => 4,
                    _ => 2,
                };
            }

            if let Some(distance) = fuzzy_match_distance(token, &content_lower) {
                score += match distance {
                    0 => 2,
                    1 => 1,
                    _ => 1,
                };
            }
        }
    }

    score
}

#[tauri::command]
fn get_help_doc(doc_id: String) -> Result<String, String> {
    let normalized = doc_id.trim();
    if normalized.is_empty() {
        return Err("Help document id cannot be empty.".to_string());
    }

    let doc = find_help_doc_by_id(normalized)
        .ok_or_else(|| format!("Help document '{}' not found.", normalized))?;

    let payload = serde_json::json!({
        "id": doc.id,
        "title": doc.title,
        "path": doc.path,
        "content": doc.content
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn search_help_docs(query: String) -> Result<String, String> {
    let tokens = normalize_help_query_tokens(&query);
    if tokens.is_empty() {
        return Ok(serde_json::json!({
            "query": query,
            "results": []
        })
        .to_string());
    }

    let mut ranked: Vec<serde_json::Value> = help_docs_catalog()
        .into_iter()
        .filter_map(|doc| {
            let score = compute_help_doc_score(&doc, &tokens);
            if score <= 0 {
                return None;
            }

            Some(serde_json::json!({
                "id": doc.id,
                "title": doc.title,
                "path": doc.path,
                "score": score,
                "snippet": build_help_doc_snippet(doc.content, &tokens)
            }))
        })
        .collect();

    ranked.sort_by(|a, b| {
        let score_a = a.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let score_b = b.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        score_b.cmp(&score_a)
    });

    let payload = serde_json::json!({
        "query": query,
        "results": ranked
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn list_markdown_documents(directory_path: String) -> Result<String, String> {
    let root = PathBuf::from(directory_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err(format!("Markdown directory does not exist: {}", directory_path));
    }

    let docs: Vec<serde_json::Value> = collect_markdown_files_recursive(&root)?
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .to_string();
            serde_json::json!({
                "path": path.to_string_lossy().to_string(),
                "relative_path": relative
            })
        })
        .collect();

    Ok(serde_json::json!({
        "directory": root.to_string_lossy().to_string(),
        "documents": docs
    })
    .to_string())
}

#[tauri::command]
fn read_markdown_document(directory_path: String, file_path: String) -> Result<String, String> {
    let root = PathBuf::from(directory_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err(format!("Markdown directory does not exist: {}", directory_path));
    }

    let doc_path = PathBuf::from(file_path.trim());
    if !doc_path.exists() || !doc_path.is_file() {
        return Err(format!("Markdown file does not exist: {}", file_path));
    }

    if !is_path_within_root(&doc_path, &root) {
        return Err("Requested markdown file is outside of the configured markdown directory.".to_string());
    }

    let content = fs::read_to_string(&doc_path).map_err(|e| format!("Failed to read markdown file: {e}"))?;
    let relative = doc_path
        .strip_prefix(&root)
        .unwrap_or(doc_path.as_path())
        .to_string_lossy()
        .to_string();

    let title = content
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                Some(trimmed.trim_start_matches('#').trim().to_string())
            } else {
                None
            }
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            doc_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "Markdown Document".to_string())
        });

    Ok(serde_json::json!({
        "title": title,
        "path": doc_path.to_string_lossy().to_string(),
        "relative_path": relative,
        "content": content
    })
    .to_string())
}

#[tauri::command]
fn search_markdown_documents(directory_path: String, query: String) -> Result<String, String> {
    let root = PathBuf::from(directory_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err(format!("Markdown directory does not exist: {}", directory_path));
    }

    let tokens = normalize_help_query_tokens(&query);
    if tokens.is_empty() {
        return Ok(serde_json::json!({
            "query": query,
            "results": []
        })
        .to_string());
    }

    let mut results = Vec::new();
    for path in collect_markdown_files_recursive(&root)? {
        let content = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };

        let relative = path
            .strip_prefix(&root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();

        let title = content
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('#') {
                    Some(trimmed.trim_start_matches('#').trim().to_string())
                } else {
                    None
                }
            })
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| relative.clone());

        let score = compute_markdown_doc_score(&title, &content, &tokens);
        if score <= 0 {
            continue;
        }

        results.push(serde_json::json!({
            "title": title,
            "path": path.to_string_lossy().to_string(),
            "relative_path": relative,
            "score": score,
            "snippet": build_markdown_doc_snippet(&content, &tokens)
        }));
    }

    results.sort_by(|a, b| {
        let score_a = a.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let score_b = b.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        score_b.cmp(&score_a)
    });

    Ok(serde_json::json!({
        "query": query,
        "results": results
    })
    .to_string())
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

    let mut config = load_app_config(&app).unwrap_or_default();
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

fn has_required_master_axiom_sections(content: &str) -> (bool, Vec<String>) {
    let required_sections = [
        "## Core Axiom",
        "## Hypothesis",
        "## Predictions",
        "## Observational Consequences",
    ];

    let lower = content.to_ascii_lowercase();
    let mut missing = Vec::new();
    for section in required_sections {
        if !lower.contains(&section.to_ascii_lowercase()) {
            missing.push(section.to_string());
        }
    }

    (missing.is_empty(), missing)
}

fn collect_named_artifacts(search_roots: &[PathBuf], keywords: &[&str], limit: usize) -> Vec<String> {
    let mut found = Vec::new();

    for root in search_roots {
        if !root.exists() || !root.is_dir() {
            continue;
        }

        let mut files = Vec::new();
        if recursive_file_scan(root, &mut files).is_err() {
            continue;
        }

        for path in files {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();

            if keywords.iter().any(|keyword| file_name.contains(keyword)) {
                found.push(path.to_string_lossy().to_string());
                if found.len() >= limit {
                    return found;
                }
            }
        }
    }

    found
}

fn build_theory_import_checklist(
    project_root: &Path,
    theory_dir: &str,
    master_axiom_path: &Path,
    tools_dir: &str,
) -> serde_json::Value {
    let theory_path = PathBuf::from(theory_dir);
    let scan = scan_markdown_theory(theory_dir);
    let files_scanned = scan["files_scanned"].as_u64().unwrap_or(0) as usize;
    let heading_count = scan["headings"].as_array().map(|items| items.len()).unwrap_or(0);
    let summary_count = scan["file_summaries"].as_array().map(|items| items.len()).unwrap_or(0);

    let import_complete = theory_path.exists() && theory_path.is_dir() && files_scanned > 0;
    let import_status = if import_complete {
        "complete"
    } else if theory_path.exists() && theory_path.is_dir() {
        "in_progress"
    } else {
        "not_started"
    };
    let import_stage = serde_json::json!({
        "id": "import",
        "label": "Import",
        "status": import_status,
        "evidence": format!("Theory directory: {} | Markdown files scanned: {}", theory_path.to_string_lossy(), files_scanned),
        "missing_items": if import_complete {
            Vec::<String>::new()
        } else {
            vec!["Import theory source and ensure markdown files are present in the configured theory directory.".to_string()]
        }
    });

    let scan_complete = files_scanned > 0 && (heading_count > 0 || summary_count > 0);
    let scan_status = if scan_complete {
        "complete"
    } else if files_scanned > 0 {
        "in_progress"
    } else {
        "not_started"
    };
    let scan_stage = serde_json::json!({
        "id": "scan",
        "label": "Scan",
        "status": scan_status,
        "evidence": format!(
            "Files scanned: {} | Heading cues: {} | Summary cues: {}",
            files_scanned,
            heading_count,
            summary_count
        ),
        "missing_items": if scan_complete {
            Vec::<String>::new()
        } else {
            vec!["Run scan after import and verify the corpus contains headings or non-empty summaries.".to_string()]
        }
    });

    let (axiom_complete, mut axiom_missing) = if master_axiom_path.exists() {
        match fs::read_to_string(master_axiom_path) {
            Ok(content) => has_required_master_axiom_sections(&content),
            Err(_) => (false, vec!["Master axiom exists but could not be read.".to_string()]),
        }
    } else {
        (false, vec!["Master axiom file does not exist yet.".to_string()])
    };

    if !axiom_complete && master_axiom_path.exists() && axiom_missing.is_empty() {
        axiom_missing.push("Master axiom is missing one or more required sections.".to_string());
    }

    let axiom_needs_attention = axiom_missing
        .iter()
        .any(|item| item.to_ascii_lowercase().contains("could not be read"));
    let axiom_status = if axiom_complete {
        "complete"
    } else if axiom_needs_attention {
        "needs_attention"
    } else if master_axiom_path.exists() {
        "in_progress"
    } else {
        "not_started"
    };

    let axiom_stage = serde_json::json!({
        "id": "master_axiom",
        "label": "Master axiom",
        "status": axiom_status,
        "evidence": format!("Master axiom path: {}", master_axiom_path.to_string_lossy()),
        "missing_items": axiom_missing
    });

    let briefing_files = [
        project_root.join("ai_briefing.md"),
        project_root.join("session_recap.md"),
        project_root.join("project_awareness.md"),
        project_root.join("workspace_tree.txt"),
    ];
    let mut missing_briefing = Vec::new();
    let mut existing_briefing = Vec::new();
    for file in briefing_files {
        if file.exists() {
            existing_briefing.push(file.to_string_lossy().to_string());
        } else {
            missing_briefing.push(file.to_string_lossy().to_string());
        }
    }
    let briefing_complete = missing_briefing.is_empty();
    let briefing_status = if briefing_complete {
        "complete"
    } else if !existing_briefing.is_empty() {
        "in_progress"
    } else {
        "not_started"
    };
    let briefing_stage = serde_json::json!({
        "id": "briefing",
        "label": "Briefing",
        "status": briefing_status,
        "evidence": format!("Generated files present: {}", existing_briefing.len()),
        "artifacts": existing_briefing,
        "missing_items": if briefing_complete {
            Vec::<String>::new()
        } else {
            vec![format!("Missing briefing artifacts: {}", missing_briefing.join(", "))]
        }
    });

    let mut artifact_roots = vec![
        project_root.join("experiments"),
        project_root.join("analysis"),
        project_root.join("reports"),
        project_root.join("results"),
        project_root.join("benchmarks"),
        project_root.join("output"),
        project_root.join("outputs"),
        project_root.join("artifacts"),
    ];

    if !tools_dir.trim().is_empty() {
        artifact_roots.push(PathBuf::from(tools_dir));
    }

    let has_artifact_roots = artifact_roots.iter().any(|root| root.exists() && root.is_dir());

    let experiment_artifacts = collect_named_artifacts(
        &artifact_roots,
        &["experiment", "probe", "batch", "suite", "analysis", "metrics", "run"],
        8,
    );
    let experiments_complete = !experiment_artifacts.is_empty();
    let experiments_status = if experiments_complete {
        "complete"
    } else if has_artifact_roots {
        "in_progress"
    } else {
        "not_started"
    };
    let experiments_stage = serde_json::json!({
        "id": "run_experiments",
        "label": "Run experiments",
        "status": experiments_status,
        "evidence": format!("Detected experiment-related artifacts: {}", experiment_artifacts.len()),
        "artifacts": experiment_artifacts,
        "missing_items": if experiments_complete {
            Vec::<String>::new()
        } else {
            vec!["No experiment artifacts found yet. Run at least one probe, analysis, or experiment output flow and save results under experiments/results/reports paths.".to_string()]
        }
    });

    let score_artifacts = collect_named_artifacts(
        &artifact_roots,
        &["score", "scoring", "benchmark", "evaluation", "validation", "accuracy", "pass_fail"],
        8,
    );
    let scoring_complete = !score_artifacts.is_empty();
    let scoring_status = if scoring_complete {
        "complete"
    } else if has_artifact_roots {
        "in_progress"
    } else {
        "not_started"
    };
    let scoring_stage = serde_json::json!({
        "id": "score_outcomes",
        "label": "Score outcomes",
        "status": scoring_status,
        "evidence": format!("Detected scoring-related artifacts: {}", score_artifacts.len()),
        "artifacts": score_artifacts,
        "missing_items": if scoring_complete {
            Vec::<String>::new()
        } else {
            vec!["No scoring artifacts found yet. Save at least one evaluation/benchmark/scorecard artifact after running experiments.".to_string()]
        }
    });

    let stages = vec![
        import_stage,
        scan_stage,
        axiom_stage,
        briefing_stage,
        experiments_stage,
        scoring_stage,
    ];

    let completed_count = stages
        .iter()
        .filter(|stage| stage["status"].as_str().unwrap_or_default() == "complete")
        .count();

    let next_step = stages
        .iter()
        .find(|stage| stage["status"].as_str().unwrap_or_default() != "complete")
        .and_then(|stage| stage["label"].as_str())
        .unwrap_or("All stages complete")
        .to_string();

    serde_json::json!({
        "status": if completed_count == stages.len() { "complete" } else { "incomplete" },
        "project_root": project_root.to_string_lossy().to_string(),
        "theory_directory": theory_path.to_string_lossy().to_string(),
        "master_axiom_file": master_axiom_path.to_string_lossy().to_string(),
        "completed_count": completed_count,
        "total_count": stages.len(),
        "next_recommended_step": next_step,
        "stages": stages,
        "notes": [
            "Checklist verification is artifact-based and theory-agnostic.",
            "Experiment and scoring stages rely on detected saved outputs; add explicit output folders for stronger verification fidelity."
        ]
    })
}

#[tauri::command]
fn verify_theory_import_checklist(
    workspace_path: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let current_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();

    let mut config = load_app_config(&app).unwrap_or_default();
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

    let checklist = build_theory_import_checklist(
        &project_root,
        &theory_dir,
        &master_axiom_path,
        &config.tools_dir,
    );

    Ok(checklist.to_string())
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

    let mut config = load_app_config(&app).unwrap_or_default();
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
    if let Ok(mut config) = load_app_config(&app) {
        apply_unset_path_defaults(&mut config);
        return config;
    }

    let mut config = AppConfig::default();
    apply_unset_path_defaults(&mut config);
    config
}

#[tauri::command]
fn list_theory_profiles(app: AppHandle) -> Result<String, String> {
    let store = load_theory_profiles_store(&app)?;
    let profile_names: Vec<String> = store.profiles.keys().cloned().collect();

    let payload = serde_json::json!({
        "active_profile": store.active_profile,
        "profiles": profile_names,
        "count": store.profiles.len()
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn save_theory_profile(profile_name: String, state: tauri::State<AppState>, app: AppHandle) -> Result<String, String> {
    let normalized_name = normalize_profile_name(&profile_name)?;
    let mut config = load_app_config(&app).unwrap_or_default();

    let workspace_path = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?
        .clone();
    let workspace_trimmed = workspace_path.trim();
    if !workspace_trimmed.is_empty() {
        config.last_root_dir = workspace_trimmed.to_string();
        if config.project_root_dir.trim().is_empty() {
            config.project_root_dir = workspace_trimmed.to_string();
        }
    }

    save_app_config(&app, &config)?;

    let mut store = load_theory_profiles_store(&app)?;
    store.profiles.insert(normalized_name.clone(), config);
    store.active_profile = normalized_name.clone();
    save_theory_profiles_store(&app, &store)?;

    let payload = serde_json::json!({
        "status": "ok",
        "saved_profile": normalized_name,
        "count": store.profiles.len()
    });
    Ok(payload.to_string())
}

#[tauri::command]
fn load_theory_profile(profile_name: String, state: tauri::State<AppState>, app: AppHandle) -> Result<String, String> {
    let normalized_name = normalize_profile_name(&profile_name)?;
    let mut store = load_theory_profiles_store(&app)?;
    let profile = store
        .profiles
        .get(&normalized_name)
        .cloned()
        .ok_or_else(|| format!("Theory profile '{}' was not found", normalized_name))?;

    let workspace_path = if !profile.project_root_dir.trim().is_empty() {
        profile.project_root_dir.trim().to_string()
    } else {
        profile.last_root_dir.trim().to_string()
    };

    save_app_config(&app, &profile)?;

    let mut runtime_workspace = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?;
    *runtime_workspace = workspace_path.clone();

    store.active_profile = normalized_name.clone();
    save_theory_profiles_store(&app, &store)?;

    let payload = serde_json::json!({
        "status": "ok",
        "loaded_profile": normalized_name,
        "workspace_path": workspace_path
    });
    Ok(payload.to_string())
}

#[tauri::command]
fn rename_theory_profile(current_name: String, new_name: String, app: AppHandle) -> Result<String, String> {
    let current_normalized = normalize_profile_name(&current_name)?;
    let new_normalized = normalize_profile_name(&new_name)?;

    if current_normalized == new_normalized {
        let payload = serde_json::json!({
            "status": "ok",
            "renamed_from": current_normalized,
            "renamed_to": new_normalized,
            "count": load_theory_profiles_store(&app)?.profiles.len()
        });
        return Ok(payload.to_string());
    }

    let mut store = load_theory_profiles_store(&app)?;
    if !store.profiles.contains_key(&current_normalized) {
        return Err(format!("Theory profile '{}' was not found", current_normalized));
    }
    if store.profiles.contains_key(&new_normalized) {
        return Err(format!("Theory profile '{}' already exists", new_normalized));
    }

    let profile = store
        .profiles
        .remove(&current_normalized)
        .ok_or_else(|| format!("Theory profile '{}' was not found", current_normalized))?;
    store.profiles.insert(new_normalized.clone(), profile);

    if store.active_profile == current_normalized {
        store.active_profile = new_normalized.clone();
    }

    save_theory_profiles_store(&app, &store)?;

    let payload = serde_json::json!({
        "status": "ok",
        "renamed_from": current_normalized,
        "renamed_to": new_normalized,
        "count": store.profiles.len(),
        "active_profile": store.active_profile
    });
    Ok(payload.to_string())
}

#[tauri::command]
fn delete_theory_profile(profile_name: String, app: AppHandle) -> Result<String, String> {
    let normalized_name = normalize_profile_name(&profile_name)?;
    let mut store = load_theory_profiles_store(&app)?;

    let removed = store.profiles.remove(&normalized_name);
    if removed.is_none() {
        return Err(format!("Theory profile '{}' was not found", normalized_name));
    }

    if store.active_profile == normalized_name {
        store.active_profile = String::new();
    }

    save_theory_profiles_store(&app, &store)?;

    let payload = serde_json::json!({
        "status": "ok",
        "deleted_profile": normalized_name,
        "count": store.profiles.len(),
        "active_profile": store.active_profile
    });

    Ok(payload.to_string())
}

#[tauri::command]
fn close_active_theory(state: tauri::State<AppState>, app: AppHandle) -> Result<String, String> {
    let mut config = load_app_config(&app).unwrap_or_default();

    config.last_root_dir = String::new();
    config.project_root_dir = String::new();
    config.theory_md_dir = String::new();
    config.master_axiom_file = String::new();
    config.tools_dir = String::new();
    config.reuse_notes_next_session = false;
    config.first_session_completed = false;

    save_app_config(&app, &config)?;

    let mut runtime_workspace = state
        .workspace_path
        .lock()
        .map_err(|_| "Workspace path mutex poisoned".to_string())?;
    *runtime_workspace = String::new();

    if let Ok(mut store) = load_theory_profiles_store(&app) {
        store.active_profile = String::new();
        let _ = save_theory_profiles_store(&app, &store);
    }

    let payload = serde_json::json!({
        "status": "ok",
        "message": "Active theory closed. App returned to first-session mode."
    });

    Ok(payload.to_string())
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
    
    files.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
    });

    Ok(files)
}

#[tauri::command]
#[allow(non_snake_case)]
fn export_workspace_tree(rootPath: String, visibleTree: Option<String>) -> Result<String, String> {
    let root = std::path::Path::new(&rootPath);
    let tree_string = workspace_tree_for_export(root, visibleTree.as_deref())?;

    // Save AI-friendly markdown first, then keep a compatibility text export.
    let markdown_output_path = root.join("workspace_tree.md");
    let legacy_output_path = root.join("workspace_tree.txt");
    std::fs::write(&markdown_output_path, &tree_string).map_err(|e| format!("Failed to write markdown file: {}", e))?;
    std::fs::write(&legacy_output_path, &tree_string).map_err(|e| format!("Failed to write compatibility file: {}", e))?;

    Ok(format!("Visible workspace map generated: {}", markdown_output_path.to_string_lossy()))
}

#[tauri::command]
fn save_root_directory(path: String, state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<String, String> {
    let mut current_path = state.workspace_path.lock().map_err(|e| e.to_string())?;
    *current_path = path.clone();

    let mut config = load_app_config(&app).unwrap_or_default();

    update_workspace_root_in_config(&mut config, &path);
    save_app_config(&app, &config)?;

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
    let normalized_tag = normalize_version_tag(&tag)?;
    let root = std::path::Path::new(&rootPath);

    if !root.exists() || !root.is_dir() {
        return Err(format!("Workspace path is invalid: {}", rootPath));
    }

    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let versions_dir = root.join("versions");
    let snapshot_dir = versions_dir.join(&normalized_tag);

    if !snapshot_dir.exists() || !snapshot_dir.is_dir() {
        return Err(format!("Version '{}' not found in local versions folder.", normalized_tag));
    }

    fn should_skip_version_copy(name: &str) -> bool {
        matches!(
            name,
            ".git" | "versions" | "target" | "node_modules" | "dist" | "build" | ".venv" | "venv" | "__pycache__" | ".idea" | ".vscode"
        )
    }

    fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if should_skip_version_copy(name.as_ref()) {
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

    // Remove existing workspace content except .git and versions before restore.
    for entry in std::fs::read_dir(&root).map_err(|e| format!("Failed to read workspace root: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read workspace entry: {}", e))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if name.as_ref() == ".git" || name.as_ref() == "versions" {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to remove directory during restore ({}): {}", path.to_string_lossy(), e))?;
        } else {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove file during restore ({}): {}", path.to_string_lossy(), e))?;
        }
    }

    copy_recursive(&snapshot_dir, &root)
        .map_err(|e| format!("Failed to restore local version '{}': {}", normalized_tag, e))?;

    Ok(format!(
        "Local version '{}' restored from {}",
        normalized_tag,
        snapshot_dir.to_string_lossy()
    ))
}

#[tauri::command]
#[allow(non_snake_case)]
fn save_as_hypothesis(name: String, rootPath: String) -> Result<String, String> {
    let branch_name = name.trim();
    if branch_name.is_empty() {
        return Err("Hypothesis branch name cannot be empty.".to_string());
    }

    let repo_check = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if !repo_check.status.success() {
        return Err("This workspace is not an active git repository.".to_string());
    }

    let output = std::process::Command::new("git")
        .args(["checkout", "-b", branch_name])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(format!("Hypothesis branch '{}' created. Sandbox activated.", branch_name))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
#[allow(non_snake_case)]
fn get_current_branch(rootPath: String) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[tauri::command]
#[allow(non_snake_case)]
fn terminate_hypothesis_branch(rootPath: String, branchName: String, force: bool) -> Result<String, String> {
    let target_branch = branchName.trim();
    if target_branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }

    if matches!(target_branch, "main" | "master") {
        return Err("Refusing to terminate primary branch (main/master).".to_string());
    }

    let current_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if !current_output.status.success() {
        return Err(String::from_utf8_lossy(&current_output.stderr).trim().to_string());
    }

    let current_branch = String::from_utf8_lossy(&current_output.stdout).trim().to_string();

    let main_exists = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
        .current_dir(&rootPath)
        .status()
        .map_err(|e| e.to_string())?
        .success();
    let master_exists = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet", "refs/heads/master"])
        .current_dir(&rootPath)
        .status()
        .map_err(|e| e.to_string())?
        .success();

    if current_branch == target_branch {
        let fallback = if main_exists {
            "main"
        } else if master_exists {
            "master"
        } else {
            return Err("No fallback branch (main/master) found for checkout before deletion.".to_string());
        };

        let checkout_output = std::process::Command::new("git")
            .args(["checkout", fallback])
            .current_dir(&rootPath)
            .output()
            .map_err(|e| e.to_string())?;

        if !checkout_output.status.success() {
            return Err(format!(
                "Failed to checkout '{}' before branch termination: {}",
                fallback,
                String::from_utf8_lossy(&checkout_output.stderr).trim()
            ));
        }
    }

    let delete_flag = if force { "-D" } else { "-d" };
    let delete_output = std::process::Command::new("git")
        .args(["branch", delete_flag, target_branch])
        .current_dir(&rootPath)
        .output()
        .map_err(|e| e.to_string())?;

    if !delete_output.status.success() {
        return Err(String::from_utf8_lossy(&delete_output.stderr).trim().to_string());
    }

    let fallback_now = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&rootPath)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(format!(
        "Hypothesis branch '{}' terminated. Active branch: {}",
        target_branch, fallback_now
    ))
}

#[tauri::command]
fn git_push(state: tauri::State<AppState>) -> Result<String, String> {
    let current_path = state.workspace_path.lock().map_err(|_| "Mutex poisoned")?.clone();
    if current_path.is_empty() { return Err("No workspace loaded.".to_string()); }

    let branch_output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&current_path)
        .output()
        .map_err(|e| format!("Failed to inspect current branch: {}", e))?;

    if !branch_output.status.success() {
        return Err(String::from_utf8_lossy(&branch_output.stderr).trim().to_string());
    }

    let current_branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    if current_branch != "main" && current_branch != "master" {
        return Err(format!(
            "Push blocked on branch '{}'. Hypothesis branches are local-only; merge or restore into main/master before pushing.",
            current_branch
        ));
    }

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

    #[test]
    fn estimates_serialized_prompt_usage_with_role_breakdown() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Stable instructions",
                "context_source": "startup_primer",
                "context_trigger": "application_start"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Current question\nAttachment text",
                "context_source": "current_request",
                "context_parts": [
                    {"source": "current_request", "content": "Current question"},
                    {"source": "attachment", "content": "Attachment text"}
                ]
            }),
        ];

        let usage = estimate_prompt_usage_value(&history);
        let serialized = build_prompt_from_history(&history);

        assert_eq!(usage["total"]["bytes"].as_u64().unwrap(), serialized.len() as u64);
        assert!(!serialized.contains("context_source"));
        assert!(!serialized.contains("application_start"));
        assert_eq!(usage["method"].as_str().unwrap(), "character_heuristic_4_chars_per_token");
        assert_eq!(usage["by_role"]["system"]["messages"].as_u64().unwrap(), 1);
        assert_eq!(usage["by_role"]["user"]["messages"].as_u64().unwrap(), 1);
        assert_eq!(usage["by_source"]["startup_primer"]["segments"].as_u64().unwrap(), 1);
        assert_eq!(usage["by_source"]["current_request"]["content_characters"].as_u64().unwrap(), 16);
        assert_eq!(usage["by_source"]["attachment"]["content_characters"].as_u64().unwrap(), 15);
        assert_eq!(usage["provenance"][0]["trigger"].as_str().unwrap(), "application_start");
    }

    #[test]
    fn benchmark_prompt_estimate_stays_within_approved_ceiling() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Role: theory analysis.\nAxiom: E = mc^2.\nTree: @tree-v1\nproject/\n  theory.md",
                "context_source": "startup_primer"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Compare the axiom with the active project structure.",
                "context_source": "current_request"
            }),
        ];

        let usage = estimate_prompt_usage_value(&history);
        let estimated_tokens = usage["total"]["estimated_tokens"].as_u64().unwrap();

        assert!(estimated_tokens <= 40, "Benchmark prompt estimate increased to {estimated_tokens} tokens");
    }

    #[test]
    fn attachment_workflow_estimate_stays_within_approved_ceiling() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Role: inspect project evidence.\nAxiom: p = mv.",
                "context_source": "startup_primer"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Check this result.\n[Attached text file: results.txt]\nvelocity=3; mass=2",
                "context_source": "current_request",
                "context_parts": [
                    {"source": "current_request", "content": "Check this result."},
                    {"source": "attachment", "content": "[Attached text file: results.txt]\nvelocity=3; mass=2"}
                ]
            }),
        ];

        let usage = estimate_prompt_usage_value(&history);
        let estimated_tokens = usage["total"]["estimated_tokens"].as_u64().unwrap();

        assert!(estimated_tokens <= 40, "Attachment workflow estimate increased to {estimated_tokens} tokens");
        assert!(usage["by_source"]["attachment"]["estimated_tokens"].as_u64().unwrap() > 0);
    }

    #[test]
    fn retrieval_context_is_reported_as_a_distinct_prompt_source() {
        let history = vec![serde_json::json!({
            "role": "user",
            "content": "Question plus bounded retrieval evidence",
            "context_source": "context_probe",
            "context_parts": [
                {"source": "context_probe", "content": "Question"},
                {"source": "retrieval", "content": "Source-grounded evidence"}
            ]
        })];

        let usage = estimate_prompt_usage_value(&history);
        assert_eq!(usage["by_source"]["context_probe"]["segments"].as_u64().unwrap(), 1);
        assert_eq!(usage["by_source"]["retrieval"]["segments"].as_u64().unwrap(), 1);
        assert!(usage["by_source"]["retrieval"]["estimated_tokens"].as_u64().unwrap() > 0);
    }

    #[test]
    fn provider_dispatch_rejects_retrieval_context_over_declared_budget() {
        let bounded = vec![serde_json::json!({
            "role": "user",
            "content": "bounded retrieval",
            "retrieval_budget_characters": 500,
            "context_parts": [
                {"source": "retrieval", "content": "e".repeat(500)}
            ]
        })];
        let oversized = vec![serde_json::json!({
            "role": "user",
            "content": "oversized retrieval",
            "retrieval_budget_characters": 500,
            "context_parts": [
                {"source": "retrieval", "content": "e".repeat(501)}
            ]
        })];

        validate_retrieval_context_budgets(&bounded).unwrap();
        let error = validate_retrieval_context_budgets(&oversized).unwrap_err();
        assert!(error.contains("501/500"));
    }

    #[test]
    fn canonical_assembler_orders_volatility_and_preserves_roles() {
        let history = vec![
            serde_json::json!({
                "role": "user",
                "content": "Earlier question",
                "context_source": "current_request",
                "context_volatility": "dynamic"
            }),
            serde_json::json!({
                "role": "assistant",
                "content": "Earlier answer",
                "context_source": "assistant_history",
                "context_volatility": "dynamic"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Stable contract",
                "context_source": "application_contract",
                "context_volatility": "stable"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Project context",
                "context_source": "startup_primer",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Current question",
                "context_source": "current_request",
                "context_volatility": "dynamic"
            }),
        ];

        let assembly = assemble_canonical_messages(&history);
        let contents = assembly
            .messages
            .iter()
            .map(|message| message["content"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(contents, vec!["Stable contract", "Project context", "Earlier question", "Earlier answer", "Current question"]);
        assert_eq!(assembly.messages[0]["role"], "system");
        assert_eq!(assembly.messages[3]["role"], "assistant");
        assert!(assembly.messages.iter().all(|message| message.get("context_source").is_none()));
        assert_eq!(assembly.sections[0]["tier"], "stable");
        assert_eq!(assembly.sections[4]["tier"], "current_request");
    }

    #[test]
    fn canonical_assembler_keeps_only_latest_declared_context_slot() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Old primer",
                "context_source": "startup_primer",
                "context_slot": "baseline_context",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Refreshed primer",
                "context_source": "briefing_refresh",
                "context_slot": "baseline_context",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Question",
                "context_source": "current_request",
                "context_volatility": "dynamic"
            }),
        ];

        let assembly = assemble_canonical_messages(&history);
        let contents = assembly.messages.iter().map(|message| message["content"].as_str().unwrap()).collect::<Vec<_>>();

        assert_eq!(contents, vec!["Refreshed primer", "Question"]);
    }

    #[test]
    fn session_notes_replace_independently_from_baseline_awareness() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Retrieval-first baseline",
                "context_slot": "baseline_context",
                "context_source": "startup_awareness",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Old idea note",
                "context_slot": "session_notes",
                "context_source": "idea_pad",
                "context_volatility": "dynamic"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Current idea note",
                "context_slot": "session_notes",
                "context_source": "idea_pad",
                "context_volatility": "dynamic"
            }),
        ];

        let assembly = assemble_canonical_messages(&history);
        let contents = assembly
            .messages
            .iter()
            .map(|message| message["content"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(contents, vec!["Retrieval-first baseline", "Current idea note"]);
    }

    #[test]
    fn openai_request_body_keeps_canonical_message_roles() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "Contract"}),
            serde_json::json!({"role": "user", "content": "Question"}),
            serde_json::json!({"role": "assistant", "content": "Answer"}),
        ];

        let body = build_openai_request_body("gpt-4.1-mini", &messages);

        assert_eq!(body["messages"], serde_json::Value::Array(messages));
        assert_eq!(body["model"], "gpt-4.1-mini");
    }

    #[test]
    fn stable_prefix_hash_changes_only_with_reusable_context() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Application contract",
                "context_source": "application_contract",
                "context_volatility": "stable"
            }),
            serde_json::json!({
                "role": "system",
                "content": "Project axiom A",
                "context_source": "startup_primer",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Question one",
                "context_source": "current_request",
                "context_volatility": "dynamic"
            }),
        ];
        let mut changed_request = history.clone();
        changed_request[2]["content"] = serde_json::json!("Question two");
        let mut changed_project = history.clone();
        changed_project[1]["content"] = serde_json::json!("Project axiom B");

        let baseline = assemble_canonical_messages(&history);
        let request_change = assemble_canonical_messages(&changed_request);
        let project_change = assemble_canonical_messages(&changed_project);

        assert_eq!(baseline.stable_prefix["sha256"], request_change.stable_prefix["sha256"]);
        assert_ne!(baseline.stable_prefix["sha256"], project_change.stable_prefix["sha256"]);
        assert_eq!(baseline.stable_prefix["message_count"], 2);
    }

    #[test]
    fn parses_openai_cached_and_reasoning_usage() {
        let response = serde_json::json!({
            "usage": {
                "prompt_tokens": 1200,
                "completion_tokens": 90,
                "total_tokens": 1290,
                "prompt_tokens_details": {"cached_tokens": 1024},
                "completion_tokens_details": {"reasoning_tokens": 40}
            }
        });

        let usage = parse_openai_usage(&response, "gpt-4.1");

        assert_eq!(usage.input_tokens, Some(1200));
        assert_eq!(usage.cached_input_tokens, Some(1024));
        assert_eq!(usage.reasoning_tokens, Some(40));
    }

    #[test]
    fn parses_gemini_usage_metadata() {
        let response = serde_json::json!({
            "usageMetadata": {
                "promptTokenCount": 800,
                "cachedContentTokenCount": 600,
                "candidatesTokenCount": 70,
                "thoughtsTokenCount": 20,
                "totalTokenCount": 890
            }
        });

        let usage = parse_gemini_usage(&response, "gemini-2.5-flash");

        assert_eq!(usage.input_tokens, Some(800));
        assert_eq!(usage.cached_input_tokens, Some(600));
        assert_eq!(usage.output_tokens, Some(70));
        assert_eq!(usage.reasoning_tokens, Some(20));
    }

    #[test]
    fn accumulates_thread_usage_without_inventing_cache_metadata() {
        let mut totals = ThreadUsageState::default();
        totals.record(&ProviderUsage {
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            input_tokens: Some(100),
            cached_input_tokens: None,
            output_tokens: Some(20),
            reasoning_tokens: None,
            total_tokens: Some(120),
        });
        totals.record(&ProviderUsage {
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            input_tokens: Some(100),
            cached_input_tokens: Some(64),
            output_tokens: Some(10),
            reasoning_tokens: Some(4),
            total_tokens: Some(110),
        });

        assert_eq!(totals.request_count, 2);
        assert_eq!(totals.cached_input_report_count, 1);
        assert_eq!(totals.cache_reported_input_tokens, 100);
        assert_eq!(totals.input_tokens, 200);
        assert_eq!(totals.cached_input_tokens, 64);
        assert_eq!(totals.reasoning_tokens, 4);
        assert_eq!(totals.total_tokens, 230);
    }

    #[test]
    fn cache_probe_payloads_share_prefix_and_change_only_suffix() {
        let history = vec![
            serde_json::json!({
                "role": "system",
                "content": "Reusable application and project context",
                "context_source": "startup_primer",
                "context_volatility": "slowly_changing"
            }),
            serde_json::json!({
                "role": "user",
                "content": "Existing question",
                "context_source": "current_request",
                "context_volatility": "dynamic"
            }),
        ];
        let assembly = assemble_canonical_messages(&history);
        let first = build_cache_probe_messages(&assembly, "CACHE_PROBE_A");
        let second = build_cache_probe_messages(&assembly, "CACHE_PROBE_B");

        assert_eq!(&first[..first.len() - 1], &second[..second.len() - 1]);
        assert_ne!(first.last(), second.last());
        assert_eq!(first.len(), assembly.stable_prefix["message_count"].as_u64().unwrap() as usize + 1);
    }

    #[test]
    fn classifies_cache_probe_results_without_inventing_hits() {
        assert_eq!(cache_probe_status(None), "metadata_unavailable");
        assert_eq!(cache_probe_status(Some(0)), "no_cache_hit");
        assert_eq!(cache_probe_status(Some(1024)), "cache_hit");
    }

    #[test]
    fn cache_probe_rejects_short_prefix_before_network() {
        let history = vec![serde_json::json!({
            "role": "system",
            "content": "Short context",
            "context_source": "startup_primer",
            "context_volatility": "slowly_changing"
        })];
        let assembly = assemble_canonical_messages(&history);

        assert!(!cache_probe_is_eligible(&assembly));
    }

    #[test]
    fn structural_ab_order_is_deterministic_and_payloads_share_question() {
        let fingerprint = "abc123";
        let question = "Which equation defines the field?";
        let first_order = structural_ab_structural_first(fingerprint, question);
        let second_order = structural_ab_structural_first(fingerprint, question);
        let legacy = build_structural_ab_messages("legacy context", question);
        let structural = build_structural_ab_messages("@ctx-v1|structural context", question);

        assert_eq!(first_order, second_order);
        assert_eq!(legacy.last(), structural.last());
        assert_ne!(legacy[1]["content"], structural[1]["content"]);
        assert_eq!(legacy[0]["role"], "system");
        assert_eq!(legacy[2]["role"], "user");
    }

    #[test]
    fn structural_ab_requires_exactly_one_embedded_core() {
        assert!(structural_ab_context_is_valid("Primer\n@ctx-v1|core\nEnd"));
        assert!(!structural_ab_context_is_valid("Primer without core"));
        assert!(!structural_ab_context_is_valid("@ctx-v1|one\n@ctx-v1|two"));
        assert!(!structural_ab_context_is_valid(&format!("@ctx-v1|{}", "x".repeat(100_001))));
    }

    #[test]
    fn parses_openai_visible_text_and_rejects_empty_output() {
        let string_response = serde_json::json!({
            "choices": [{"finish_reason": "stop", "message": {"content": "Visible answer"}}],
            "usage": {"completion_tokens": 4}
        });
        let parts_response = serde_json::json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": [{"type": "text", "text": "Part one"}, {"type": "text", "text": "Part two"}]}
            }],
            "usage": {"completion_tokens": 6}
        });
        let empty_response = serde_json::json!({
            "choices": [{"finish_reason": "length", "message": {"content": ""}}],
            "usage": {
                "completion_tokens": 350,
                "completion_tokens_details": {"reasoning_tokens": 350}
            }
        });

        assert_eq!(parse_openai_visible_content(&string_response).unwrap(), "Visible answer");
        assert_eq!(parse_openai_visible_content(&parts_response).unwrap(), "Part one\nPart two");
        let error = parse_openai_visible_content(&empty_response).unwrap_err();
        assert!(error.contains("finish_reason=length"));
        assert!(error.contains("reasoning_tokens=350"));
    }

    #[test]
    fn retries_gemini_auth_for_access_token_type_errors() {
        let response_text = r#"{"error":{"code":401,"message":"Request had invalid authentication credentials. Expected OAuth 2 access token, login cookie or other valid authentication credential.","status":"UNAUTHENTICATED","details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"ACCESS_TOKEN_TYPE_UNSUPPORTED"}]}}"#;

        assert!(gemini_auth_error_requires_bearer_fallback(
            reqwest::StatusCode::UNAUTHORIZED,
            response_text,
        ));
    }

    #[test]
    fn does_not_retry_gemini_auth_for_model_not_found_errors() {
        let response_text = r#"{"error":{"code":404,"message":"models/gemini-2.0-flash is not found for API version v1beta","status":"NOT_FOUND"}}"#;

        assert!(!gemini_auth_error_requires_bearer_fallback(
            reqwest::StatusCode::NOT_FOUND,
            response_text,
        ));
    }
}

#[tauri::command]
fn read_attachment_file(path: String) -> Result<String, String> {
    let target_path = std::path::Path::new(&path);
    if !target_path.exists() {
        return Err(format!("Attachment file does not exist: {}", path));
    }
    if !target_path.is_file() {
        return Err(format!("Attachment path is not a file: {}", path));
    }

    let mime = mime_guess::from_path(target_path).first_or_octet_stream().to_string();
    let content = if mime.starts_with("text/") || mime == "application/json" || mime == "application/xml" || mime == "application/javascript" || mime == "application/x-yaml" || mime == "application/x-httpd-php" || mime == "application/x-shellscript" {
        std::fs::read_to_string(target_path)
            .map_err(|e| format!("Failed to read text file: {e}"))?
    } else if mime.starts_with("image/") {
        let bytes = std::fs::read(target_path)
            .map_err(|e| format!("Failed to read image file: {e}"))?;
        let encoded = base64::encode(&bytes);
        format!("<image-data mime=\"{mime}\" encoding=\"base64\">\n{encoded}\n</image-data>")
    } else {
        let bytes = std::fs::read(target_path)
            .map_err(|e| format!("Failed to read binary file: {e}"))?;
        let encoded = base64::encode(&bytes);
        format!("<binary-data mime=\"{mime}\" encoding=\"base64\">\n{encoded}\n</binary-data>")
    };

    let payload = serde_json::json!({
        "path": target_path.to_string_lossy().to_string(),
        "kind": if mime.starts_with("image/") { "image" } else if mime.starts_with("text/") || mime == "application/json" || mime == "application/xml" || mime == "application/javascript" || mime == "application/x-yaml" || mime == "application/x-httpd-php" || mime == "application/x-shellscript" { "text" } else { "binary" },
        "mime": mime,
        "content": content
    });

    Ok(payload.to_string())
}

#[tauri::command]
async fn send_llm_prompt(
    pane: String,
    history: Vec<serde_json::Value>,
    thread_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    validate_retrieval_context_budgets(&history)?;
    let config = get_app_config(&app)?;
    let (provider, model) = provider_settings_for_pane(&config, &pane);
    let canonical_messages = assemble_canonical_messages(&history).messages;

    let provider_name = provider.to_ascii_lowercase();
    let openai_api_key = normalize_api_key(&config.openai_api_key);
    let gemini_api_key = normalize_api_key(&config.gemini_api_key);

    let reply = tauri::async_runtime::spawn_blocking(move || {
        match provider_name.as_str() {
            "openai" => call_openai(&openai_api_key, &model, &canonical_messages),
            "gemini" => call_gemini(&gemini_api_key, &model, &canonical_messages),
            other => Err(format!("Unsupported provider '{}'. Choose OpenAI or Gemini.", other)),
        }
    })
    .await
    .map_err(|e| format!("LLM worker task failed: {e}"))??;

    let usage_key = format!("{}:{}", pane.to_ascii_lowercase(), thread_id.unwrap_or_else(|| "default".to_string()));
    record_thread_usage(&app.state::<AppState>(), &usage_key, &reply.usage)?;
    Ok(reply.content)
}

fn validate_retrieval_context_budgets(history: &[serde_json::Value]) -> Result<(), String> {
    for entry in history {
        let retrieval_characters = entry
            .get("context_parts")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|part| part.get("source").and_then(|value| value.as_str()) == Some("retrieval"))
            .map(|part| {
                part.get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum::<usize>();
        if retrieval_characters == 0 {
            continue;
        }
        let budget = entry
            .get("retrieval_budget_characters")
            .and_then(|value| value.as_u64())
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS)
            .clamp(
                MIN_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS,
                MAX_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS,
            );
        if retrieval_characters > budget {
            return Err(format!(
                "Retrieval context exceeds its provider-dispatch budget: {retrieval_characters}/{budget} characters. Re-run retrieval with a bounded evidence packet."
            ));
        }
    }
    Ok(())
}

fn record_thread_usage(state: &tauri::State<AppState>, key: &str, usage: &ProviderUsage) -> Result<(), String> {
    let mut store = state.llm_usage.lock().map_err(|_| "LLM usage mutex poisoned".to_string())?;
    let totals = store.entry(key.to_string()).or_default();
    totals.record(usage);
    Ok(())
}

#[tauri::command]
fn get_llm_usage(pane: String, thread_id: Option<String>, state: tauri::State<AppState>) -> Result<String, String> {
    let usage_key = format!("{}:{}", pane.to_ascii_lowercase(), thread_id.unwrap_or_else(|| "default".to_string()));
    let store = state.llm_usage.lock().map_err(|_| "LLM usage mutex poisoned".to_string())?;
    Ok(serde_json::to_string(&store.get(&usage_key).cloned().unwrap_or_default()).map_err(|e| e.to_string())?)
}

const OPENAI_CACHE_MINIMUM_ESTIMATED_TOKENS: u64 = 1024;

fn build_cache_probe_messages(assembly: &CanonicalAssembly, suffix: &str) -> Vec<serde_json::Value> {
    let stable_message_count = assembly.stable_prefix["message_count"].as_u64().unwrap_or(0) as usize;
    let mut messages = assembly.messages[..stable_message_count.min(assembly.messages.len())].to_vec();
    messages.push(serde_json::json!({
        "role": "user",
        "content": format!("Return only this marker: {suffix}")
    }));
    messages
}

fn cache_probe_status(cached_input_tokens: Option<u64>) -> &'static str {
    match cached_input_tokens {
        Some(value) if value > 0 => "cache_hit",
        Some(_) => "no_cache_hit",
        None => "metadata_unavailable",
    }
}

fn cache_probe_is_eligible(assembly: &CanonicalAssembly) -> bool {
    assembly.stable_prefix["estimated_tokens"].as_u64().unwrap_or(0) >= OPENAI_CACHE_MINIMUM_ESTIMATED_TOKENS
}

#[tauri::command]
async fn run_openai_cache_probe(
    pane: String,
    history: Vec<serde_json::Value>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let config = get_app_config(&app)?;
    let (provider, model) = provider_settings_for_pane(&config, &pane);
    if !provider.eq_ignore_ascii_case("openai") {
        return Err("OpenAI cache probe requires the selected lane to use OpenAI.".to_string());
    }

    let assembly = assemble_canonical_messages(&history);
    let prefix_estimated_tokens = assembly.stable_prefix["estimated_tokens"].as_u64().unwrap_or(0);
    if !cache_probe_is_eligible(&assembly) {
        return Ok(serde_json::json!({
            "status": "ineligible_prefix",
            "network_requests": 0,
            "minimum_estimated_tokens": OPENAI_CACHE_MINIMUM_ESTIMATED_TOKENS,
            "prefix_estimated_tokens": prefix_estimated_tokens,
            "stable_prefix": assembly.stable_prefix,
            "message": "Stable prefix is below the cache-probe threshold; no paid requests were sent."
        }).to_string());
    }

    let first_messages = build_cache_probe_messages(&assembly, "CACHE_PROBE_A");
    let second_messages = build_cache_probe_messages(&assembly, "CACHE_PROBE_B");
    let api_key = normalize_api_key(&config.openai_api_key);
    let requested_model = model.clone();
    let stable_prefix = assembly.stable_prefix.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let first_started = std::time::Instant::now();
        let first = call_openai_with_options(&api_key, &model, &first_messages, Some(8), false)?;
        let first_latency_ms = first_started.elapsed().as_millis() as u64;

        let second_started = std::time::Instant::now();
        let second = call_openai_with_options(&api_key, &model, &second_messages, Some(8), false)?;
        let second_latency_ms = second_started.elapsed().as_millis() as u64;
        let status = cache_probe_status(second.usage.cached_input_tokens);

        Ok(serde_json::json!({
            "status": status,
            "network_requests": 2,
            "requested_model": requested_model,
            "actual_models": [first.usage.model, second.usage.model],
            "stable_prefix": stable_prefix,
            "first": {
                "latency_ms": first_latency_ms,
                "input_tokens": first.usage.input_tokens,
                "cached_input_tokens": first.usage.cached_input_tokens,
                "output_tokens": first.usage.output_tokens
            },
            "second": {
                "latency_ms": second_latency_ms,
                "input_tokens": second.usage.input_tokens,
                "cached_input_tokens": second.usage.cached_input_tokens,
                "output_tokens": second.usage.output_tokens
            }
        }).to_string())
    })
    .await
    .map_err(|e| format!("OpenAI cache probe worker failed: {e}"))?
}

fn structural_ab_structural_first(fingerprint: &str, question: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(fingerprint.as_bytes());
    hasher.update(b"\0");
    hasher.update(question.as_bytes());
    hasher.finalize()[0] % 2 == 0
}

fn build_structural_ab_messages(context: &str, question: &str) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "role": "system",
            "content": "Answer only from the supplied project context. Preserve qualified claims and equations. Cite exact source paths or stable IDs when available. If evidence is absent, say insufficient context."
        }),
        serde_json::json!({"role": "system", "content": context}),
        serde_json::json!({"role": "user", "content": question}),
    ]
}

fn structural_ab_context_is_valid(context: &str) -> bool {
    context.matches("@ctx-v1|").count() == 1 && context.chars().count() <= 100_000
}

fn structural_ab_variant(kind: &str, reply: ProviderReply, latency_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "content": reply.content,
        "latency_ms": latency_ms,
        "usage": reply.usage
    })
}

#[tauri::command]
async fn run_structural_ab_probe(
    pane: String,
    question: String,
    legacy_context: String,
    structural_context: String,
    fingerprint: String,
    benchmark_eligible: Option<bool>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let question = question.trim().to_string();
    let legacy_context = legacy_context.trim().to_string();
    let structural_context = structural_context.trim().to_string();
    if question.is_empty() || question.chars().count() > 2_000 {
        return Err("A/B question must contain 1-2,000 characters.".to_string());
    }
    if legacy_context.is_empty() || legacy_context.chars().count() > 100_000 {
        return Err("Legacy A/B context must contain 1-100,000 characters.".to_string());
    }
    if !structural_ab_context_is_valid(&structural_context) {
        return Err("Structural A/B context must contain exactly one @ctx-v1 core and remain under 100,000 characters.".to_string());
    }
    if fingerprint.trim().is_empty() {
        return Err("Structural A/B fingerprint is required.".to_string());
    }

    let config = get_app_config(&app)?;
    let (provider, model) = provider_settings_for_pane(&config, &pane);
    if !provider.eq_ignore_ascii_case("openai") {
        return Err("Structural A/B probe currently requires the selected lane to use OpenAI.".to_string());
    }

    let structural_first = structural_ab_structural_first(&fingerprint, &question);
    let (first_kind, first_context, second_kind, second_context) = if structural_first {
        ("structural", structural_context, "legacy", legacy_context)
    } else {
        ("legacy", legacy_context, "structural", structural_context)
    };
    let first_messages = build_structural_ab_messages(&first_context, &question);
    let second_messages = build_structural_ab_messages(&second_context, &question);
    let api_key = normalize_api_key(&config.openai_api_key);
    let requested_model = model.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let first_started = std::time::Instant::now();
        let first = call_openai_with_options(&api_key, &model, &first_messages, Some(1_200), false)?;
        let first_latency_ms = first_started.elapsed().as_millis() as u64;

        let second_started = std::time::Instant::now();
        let second = call_openai_with_options(&api_key, &model, &second_messages, Some(1_200), false)?;
        let second_latency_ms = second_started.elapsed().as_millis() as u64;
        let actual_models = [first.usage.model.clone(), second.usage.model.clone()];

        Ok(serde_json::json!({
            "status": "complete",
            "network_requests": 2,
            "requested_model": requested_model,
            "actual_models": actual_models,
            "fingerprint": fingerprint,
            "benchmark_eligible": benchmark_eligible.unwrap_or(false),
            "variant_a": structural_ab_variant(first_kind, first, first_latency_ms),
            "variant_b": structural_ab_variant(second_kind, second, second_latency_ms)
        }).to_string())
    })
    .await
    .map_err(|e| format!("Structural A/B worker failed: {e}"))?
}

fn get_app_config(app: &AppHandle) -> Result<AppConfig, String> {
    load_app_config(app)
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
            "gemini" => "gemini-2.5-flash".to_string(),
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
            "gemini-2.5-flash" | "gemini-2.5-flash-lite" | "gemini-2.5-pro" => trimmed.to_string(),
            "gemini-2.0-flash" | "gemini-2.0-flash-lite" | "gemini-1.5-flash" => "gemini-2.5-flash".to_string(),
            "gemini-1.5-pro" | "gemini-3-flash" | "gemini-3.6-flash" => "gemini-2.5-pro".to_string(),
            other if other.is_empty() => "gemini-2.5-flash".to_string(),
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

fn parse_openai_error_details_for_validation(response_text: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<OpenAiApiErrorResponse>(response_text) {
        let message = parsed.error.message.trim();
        if message.is_empty() {
            return "Unknown OpenAI error".to_string();
        }
        return message.to_string();
    }

    let compact = response_text.trim();
    if compact.is_empty() {
        "Unknown OpenAI error".to_string()
    } else {
        compact.to_string()
    }
}

fn validate_openai_model(api_key: &str, model: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key is required for model validation.".to_string());
    }

    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Validation ping"}],
        "max_tokens": 8,
        "temperature": 0
    });

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("OpenAI validation request failed: {e}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("OpenAI validation response parsing failed: {e}"))?;
    let details = parse_openai_error_details_for_validation(&response_text);
    Err(format!(
        "OpenAI model validation failed for '{}': HTTP {} - {}",
        model,
        status,
        details
    ))
}

fn gemini_auth_error_requires_bearer_fallback(status: reqwest::StatusCode, response_text: &str) -> bool {
    if status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::FORBIDDEN {
        return false;
    }

    let lower = response_text.to_ascii_lowercase();
    lower.contains("access_token_type_unsupported")
        || lower.contains("oauth 2 access token")
        || lower.contains("invalid authentication credentials")
        || lower.contains("unauthenticated")
        || lower.contains("oauth")
}

fn send_gemini_post_with_auth_fallback(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    body: &serde_json::Value,
    api_key: &str,
) -> Result<(reqwest::StatusCode, String), String> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err("Gemini API key is required for this request.".to_string());
    }

    let encoded_key = percent_encode_component(trimmed_key);
    let query_url = format!("{endpoint}?key={encoded_key}");
    let response = client
        .post(&query_url)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("Gemini request failed: {e}"))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("Gemini response parsing failed: {e}"))?;

    if status.is_success() || !gemini_auth_error_requires_bearer_fallback(status, &response_text) {
        return Ok((status, response_text));
    }

    let bearer_response = client
        .post(endpoint)
        .header("Authorization", format!("Bearer {}", trimmed_key))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| format!("Gemini bearer-token request failed: {e}"))?;

    let bearer_status = bearer_response.status();
    let bearer_text = bearer_response
        .text()
        .map_err(|e| format!("Gemini bearer-token response parsing failed: {e}"))?;

    Ok((bearer_status, bearer_text))
}

fn send_gemini_get_with_auth_fallback(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    api_key: &str,
) -> Result<(reqwest::StatusCode, String), String> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err("Gemini API key is required for this request.".to_string());
    }

    let encoded_key = percent_encode_component(trimmed_key);
    let query_url = format!("{endpoint}?key={encoded_key}");
    let response = client
        .get(&query_url)
        .send()
        .map_err(|e| format!("Gemini request failed: {e}"))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("Gemini response parsing failed: {e}"))?;

    if status.is_success() || !gemini_auth_error_requires_bearer_fallback(status, &response_text) {
        return Ok((status, response_text));
    }

    let bearer_response = client
        .get(endpoint)
        .header("Authorization", format!("Bearer {}", trimmed_key))
        .send()
        .map_err(|e| format!("Gemini bearer-token request failed: {e}"))?;

    let bearer_status = bearer_response.status();
    let bearer_text = bearer_response
        .text()
        .map_err(|e| format!("Gemini bearer-token response parsing failed: {e}"))?;

    Ok((bearer_status, bearer_text))
}

fn validate_gemini_model(api_key: &str, model: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("Gemini API key is required for model validation.".to_string());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let endpoint = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );
    let body = serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": "Validation ping" }]
        }],
        "generationConfig": {
            "temperature": 0.0,
            "maxOutputTokens": 8
        }
    });

    let (status, response_text) = send_gemini_post_with_auth_fallback(&client, &endpoint, &body, api_key)?;

    if status.is_success() {
        return Ok(());
    }

    Err(format!(
        "Gemini model validation failed for '{}': HTTP {} - {}",
        model,
        status,
        response_text.trim()
    ))
}

#[derive(Debug, Deserialize)]
struct ValidateModelSelectionPayload {
    provider: String,
    model: String,
    #[serde(alias = "apiKey")]
    api_key: String,
}

fn parse_validate_model_selection_payload(value: serde_json::Value) -> Result<ValidateModelSelectionPayload, String> {
    if let Ok(payload) = serde_json::from_value::<ValidateModelSelectionPayload>(value.clone()) {
        return Ok(payload);
    }

    if let Some(inner) = value.get("payload") {
        if let Ok(payload) = serde_json::from_value::<ValidateModelSelectionPayload>(inner.clone()) {
            return Ok(payload);
        }
    }

    Err("Unable to parse model validation payload".to_string())
}

#[tauri::command]
async fn validate_model_selection(payload: serde_json::Value) -> Result<String, String> {
    let payload = parse_validate_model_selection_payload(payload)?;
    let provider_normalized = payload.provider.to_ascii_lowercase();
    let normalized_model = normalize_model_for_provider(&provider_normalized, &payload.model);
    let normalized_api_key = normalize_api_key(&payload.api_key);

    if normalized_model.trim().is_empty() {
        return Err("Model ID is empty. Enter a model ID before validating.".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        match provider_normalized.as_str() {
            "openai" => validate_openai_model(&normalized_api_key, &normalized_model)?,
            "gemini" => validate_gemini_model(&normalized_api_key, &normalized_model)?,
            other => {
                return Err(format!(
                    "Unsupported provider '{}' for validation. Choose OpenAI or Gemini.",
                    other
                ));
            }
        }

        Ok(serde_json::json!({
            "ok": true,
            "provider": provider_normalized,
            "requested_model": payload.model,
            "normalized_model": normalized_model,
            "message": "Model validated for chat/generative requests"
        })
        .to_string())
    })
    .await
    .map_err(|e| format!("Model validation task failed: {e}"))?
}

#[derive(Debug, Deserialize)]
struct ProviderModelCatalogPayload {
    provider: String,
    #[serde(alias = "apiKey")]
    api_key: String,
}

fn parse_provider_model_catalog_payload(value: serde_json::Value) -> Result<ProviderModelCatalogPayload, String> {
    if let Ok(payload) = serde_json::from_value::<ProviderModelCatalogPayload>(value.clone()) {
        return Ok(payload);
    }

    if let Some(inner) = value.get("payload") {
        if let Ok(payload) = serde_json::from_value::<ProviderModelCatalogPayload>(inner.clone()) {
            return Ok(payload);
        }
    }

    Err("Unable to parse provider model catalog payload".to_string())
}

#[tauri::command]
async fn fetch_provider_model_catalog(payload: serde_json::Value) -> Result<String, String> {
    let payload = parse_provider_model_catalog_payload(payload)?;
    let provider_normalized = payload.provider.to_ascii_lowercase();
    let trimmed_key = normalize_api_key(&payload.api_key);

    if trimmed_key.is_empty() {
        return Err(format!("{} API key is required to load model catalog.", provider_normalized));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
        let mut models = match provider_normalized.as_str() {
            "openai" => list_openai_chat_models(&client, &trimmed_key)?,
            "gemini" => list_gemini_generate_content_models(&client, &trimmed_key)?,
            other => {
                return Err(format!(
                    "Unsupported provider '{}' for model catalog. Choose OpenAI or Gemini.",
                    other
                ));
            }
        };

        models.sort();
        models.dedup();

        Ok(serde_json::json!({
            "provider": provider_normalized,
            "models": models
        })
        .to_string())
    })
    .await
    .map_err(|e| format!("Model catalog task failed: {e}"))?
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

struct CanonicalAssembly {
    messages: Vec<serde_json::Value>,
    sections: Vec<serde_json::Value>,
    stable_prefix: serde_json::Value,
}

fn history_entry_text(entry: &serde_json::Value) -> String {
    if let Some(content) = entry.get("content").and_then(|value| value.as_str()) {
        return content.to_string();
    }

    entry
        .get("parts")
        .and_then(|value| value.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn canonical_role(entry: &serde_json::Value) -> &'static str {
    match entry
        .get("role")
        .and_then(|value| value.as_str())
        .unwrap_or("user")
        .to_ascii_lowercase()
        .as_str()
    {
        "system" => "system",
        "assistant" | "model" => "assistant",
        _ => "user",
    }
}

fn canonical_tier(entry: &serde_json::Value, index: usize, last_index: usize) -> (u8, &'static str) {
    let volatility = entry
        .get("context_volatility")
        .and_then(|value| value.as_str())
        .unwrap_or("unspecified");
    let source = entry
        .get("context_source")
        .and_then(|value| value.as_str())
        .unwrap_or_default();

    match volatility {
        "stable" => (0, "stable"),
        "slowly_changing" => (1, "slowly_changing"),
        _ if index == last_index && matches!(source, "current_request" | "context_probe") => (3, "current_request"),
        _ => (2, "thread_history"),
    }
}

fn assemble_canonical_messages(history: &[serde_json::Value]) -> CanonicalAssembly {
    let last_index = history.len().saturating_sub(1);
    let mut latest_slot_index = std::collections::BTreeMap::<String, usize>::new();
    for (index, entry) in history.iter().enumerate() {
        if let Some(slot) = entry
            .get("context_slot")
            .and_then(|value| value.as_str())
            .filter(|slot| !slot.trim().is_empty())
        {
            latest_slot_index.insert(slot.to_string(), index);
        }
    }
    let mut entries = history
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if let Some(slot) = entry.get("context_slot").and_then(|value| value.as_str()) {
                if latest_slot_index.get(slot).is_some_and(|latest| *latest != index) {
                    return None;
                }
            }
            let content = history_entry_text(entry);
            if content.trim().is_empty() {
                return None;
            }
            let (tier_rank, tier) = canonical_tier(entry, index, last_index);
            let role = canonical_role(entry);
            let source = entry
                .get("context_source")
                .and_then(|value| value.as_str())
                .unwrap_or("legacy_or_unspecified");
            let message = serde_json::json!({"role": role, "content": content});
            let section = serde_json::json!({
                "position": 0,
                "original_index": index,
                "tier": tier,
                "role": role,
                "source": source,
                "trigger": entry.get("context_trigger").and_then(|value| value.as_str()).unwrap_or("legacy_or_unspecified"),
                "volatility": entry.get("context_volatility").and_then(|value| value.as_str()).unwrap_or("unspecified"),
                "path": entry.get("context_path").and_then(|value| value.as_str()),
                "characters": content.chars().count(),
                "estimated_tokens": estimate_token_count(content.chars().count())
            });
            Some((tier_rank, index, message, section))
        })
        .collect::<Vec<_>>();

    entries.sort_by_key(|(tier_rank, index, _, _)| (*tier_rank, *index));
    let mut messages = Vec::with_capacity(entries.len());
    let mut sections = Vec::with_capacity(entries.len());
    for (position, (_, _, message, mut section)) in entries.into_iter().enumerate() {
        section["position"] = serde_json::json!(position);
        messages.push(message);
        sections.push(section);
    }

    let stable_message_count = sections
        .iter()
        .take_while(|section| matches!(section["tier"].as_str(), Some("stable" | "slowly_changing")))
        .count();
    let stable_prefix_bytes = serde_json::to_vec(&messages[..stable_message_count]).unwrap_or_default();
    let stable_prefix_characters = String::from_utf8_lossy(&stable_prefix_bytes).chars().count();
    let mut hasher = Sha256::new();
    hasher.update(&stable_prefix_bytes);
    let stable_prefix_hash = format!("{:x}", hasher.finalize());
    let stable_prefix = serde_json::json!({
        "message_count": stable_message_count,
        "bytes": stable_prefix_bytes.len(),
        "estimated_tokens": estimate_token_count(stable_prefix_characters),
        "sha256": stable_prefix_hash
    });

    CanonicalAssembly {
        messages,
        sections,
        stable_prefix,
    }
}

fn estimate_token_count(character_count: usize) -> usize {
    character_count.saturating_add(3) / 4
}

fn estimate_prompt_usage_value(history: &[serde_json::Value]) -> serde_json::Value {
    let assembly = assemble_canonical_messages(history);
    let serialized_prompt = build_prompt_from_history(&assembly.messages);
    let total_characters = serialized_prompt.chars().count();
    let mut role_totals: std::collections::BTreeMap<String, (usize, usize, usize)> = std::collections::BTreeMap::new();
    let mut source_totals: std::collections::BTreeMap<String, (usize, usize, usize)> = std::collections::BTreeMap::new();
    let mut provenance = Vec::new();

    for (index, entry) in history.iter().enumerate() {
        let role = entry
            .get("role")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        let text = if let Some(content) = entry.get("content").and_then(|value| value.as_str()) {
            content.to_string()
        } else {
            entry
                .get("parts")
                .and_then(|value| value.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default()
        };
        let totals = role_totals.entry(role.clone()).or_insert((0, 0, 0));
        totals.0 += 1;
        totals.1 += text.len();
        totals.2 += text.chars().count();

        let default_source = entry
            .get("context_source")
            .and_then(|value| value.as_str())
            .filter(|source| !source.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}_history", role));
        let context_parts = entry.get("context_parts").and_then(|value| value.as_array());
        if let Some(parts) = context_parts.filter(|parts| !parts.is_empty()) {
            for part in parts {
                let source = part
                    .get("source")
                    .and_then(|value| value.as_str())
                    .filter(|source| !source.trim().is_empty())
                    .unwrap_or(&default_source)
                    .to_string();
                let content = part.get("content").and_then(|value| value.as_str()).unwrap_or_default();
                let source_total = source_totals.entry(source).or_insert((0, 0, 0));
                source_total.0 += 1;
                source_total.1 += content.len();
                source_total.2 += content.chars().count();
            }
        } else {
            let source_total = source_totals.entry(default_source.clone()).or_insert((0, 0, 0));
            source_total.0 += 1;
            source_total.1 += text.len();
            source_total.2 += text.chars().count();
        }

        provenance.push(serde_json::json!({
            "message_index": index,
            "role": role,
            "source": default_source,
            "trigger": entry.get("context_trigger").and_then(|value| value.as_str()).unwrap_or("legacy_or_unspecified"),
            "volatility": entry.get("context_volatility").and_then(|value| value.as_str()).unwrap_or("unspecified"),
            "path": entry.get("context_path").and_then(|value| value.as_str()),
            "content_characters": text.chars().count()
        }));
    }

    let by_role = role_totals
        .into_iter()
        .map(|(role, (messages, bytes, characters))| {
            (
                role,
                serde_json::json!({
                    "messages": messages,
                    "content_bytes": bytes,
                    "content_characters": characters,
                    "estimated_tokens": estimate_token_count(characters)
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let by_source = source_totals
        .into_iter()
        .map(|(source, (segments, bytes, characters))| {
            (
                source,
                serde_json::json!({
                    "segments": segments,
                    "content_bytes": bytes,
                    "content_characters": characters,
                    "estimated_tokens": estimate_token_count(characters)
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();

    serde_json::json!({
        "method": "character_heuristic_4_chars_per_token",
        "estimate_only": true,
        "message_count": history.len(),
        "total": {
            "bytes": serialized_prompt.len(),
            "characters": total_characters,
            "estimated_tokens": estimate_token_count(total_characters)
        },
        "by_role": by_role,
        "by_source": by_source,
        "provenance": provenance,
        "assembly": assembly.sections,
        "stable_prefix": assembly.stable_prefix
    })
}

#[tauri::command]
fn estimate_prompt_usage(history: Vec<serde_json::Value>) -> String {
    estimate_prompt_usage_value(&history).to_string()
}

fn build_openai_request_body(model: &str, messages: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": 0.7
    })
}

fn parse_openai_usage(response: &serde_json::Value, model: &str) -> ProviderUsage {
    let usage = response.get("usage").unwrap_or(&serde_json::Value::Null);
    ProviderUsage {
        provider: "openai".to_string(),
        model: model.to_string(),
        input_tokens: usage.get("prompt_tokens").and_then(|value| value.as_u64()),
        cached_input_tokens: usage
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(|value| value.as_u64()),
        output_tokens: usage.get("completion_tokens").and_then(|value| value.as_u64()),
        reasoning_tokens: usage
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(|value| value.as_u64()),
        total_tokens: usage.get("total_tokens").and_then(|value| value.as_u64()),
    }
}

fn parse_openai_visible_content(response: &serde_json::Value) -> Result<String, String> {
    let choice = response
        .get("choices")
        .and_then(|choices| choices.get(0))
        .ok_or_else(|| "OpenAI returned no response choice".to_string())?;
    let content = choice
        .get("message")
        .and_then(|message| message.get("content"));
    let visible = match content {
        Some(serde_json::Value::String(text)) => text.trim().to_string(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    if !visible.is_empty() {
        return Ok(visible);
    }

    let finish_reason = choice.get("finish_reason").and_then(|value| value.as_str()).unwrap_or("unavailable");
    let usage = response.get("usage").unwrap_or(&serde_json::Value::Null);
    let completion_tokens = usage.get("completion_tokens").and_then(|value| value.as_u64());
    let reasoning_tokens = usage
        .get("completion_tokens_details")
        .and_then(|details| details.get("reasoning_tokens"))
        .and_then(|value| value.as_u64());
    let refusal = choice
        .get("message")
        .and_then(|message| message.get("refusal"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty());

    Err(format!(
        "OpenAI returned no visible response content (finish_reason={finish_reason}, completion_tokens={}, reasoning_tokens={}, refusal={}). Increase the output budget or inspect the model response mode.",
        completion_tokens.map(|value| value.to_string()).unwrap_or_else(|| "unavailable".to_string()),
        reasoning_tokens.map(|value| value.to_string()).unwrap_or_else(|| "unavailable".to_string()),
        refusal.unwrap_or("none")
    ))
}

fn call_openai(api_key: &str, model: &str, messages: &[serde_json::Value]) -> Result<ProviderReply, String> {
    call_openai_with_options(api_key, model, messages, None, true)
}

fn call_openai_with_options(
    api_key: &str,
    model: &str,
    messages: &[serde_json::Value],
    max_tokens: Option<u64>,
    allow_model_fallback: bool,
) -> Result<ProviderReply, String> {
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

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let mut attempted_models = Vec::new();
    let mut last_error = String::new();

    let candidate_models = if allow_model_fallback {
        openai_model_candidates(model)
    } else {
        vec![normalize_model_for_provider("openai", model)]
    };

    for candidate_model in candidate_models {
        attempted_models.push(candidate_model.clone());

        let mut body = build_openai_request_body(&candidate_model, messages);
        if let Some(max_tokens) = max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        let response = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("OpenAI request failed: {e}"))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| format!("OpenAI response parsing failed: {e}"))?;

        if status.is_success() {
            let parsed: serde_json::Value = serde_json::from_str(&response_text)
                .map_err(|e| format!("OpenAI response parsing failed: {e}"))?;

            let content = parse_openai_visible_content(&parsed)?;
            return Ok(ProviderReply {
                content,
                usage: parse_openai_usage(&parsed, &candidate_model),
            });
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
    let endpoint = "https://generativelanguage.googleapis.com/v1beta/models";
    let (status, response_text) = send_gemini_get_with_auth_fallback(client, endpoint, api_key)?;

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

fn is_chat_capable_openai_model_name(model_name: &str) -> bool {
    let lower = model_name.to_ascii_lowercase();

    if !(lower.starts_with("gpt-") || lower.starts_with("o1") || lower.starts_with("o3") || lower.starts_with("o4")) {
        return false;
    }

    let disallowed_markers = [
        "audio",
        "tts",
        "realtime",
        "transcribe",
        "whisper",
        "image",
        "vision",
        "embedding",
        "moderation",
        "search",
    ];

    !disallowed_markers.iter().any(|marker| lower.contains(marker))
}

fn list_openai_chat_models(client: &reqwest::blocking::Client, api_key: &str) -> Result<Vec<String>, String> {
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .map_err(|e| format!("OpenAI model discovery failed: {e}"))?;

    let status = response.status();
    let response_text = response
        .text()
        .map_err(|e| format!("OpenAI model discovery response parsing failed: {e}"))?;

    if !status.is_success() {
        let parsed_error = parse_openai_error_details_for_validation(&response_text);
        return Err(format!(
            "OpenAI model discovery failed with status {}: {}",
            status, parsed_error
        ));
    }

    let parsed: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("OpenAI model discovery response parsing failed: {e}"))?;

    let mut models = parsed
        .get("data")
        .and_then(|value| value.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("id").and_then(|id| id.as_str()))
                .filter(|id| is_chat_capable_openai_model_name(id))
                .map(|id| id.to_string())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    models.sort();
    models.dedup();
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
        "gemini-2.5-flash",
        "gemini-2.5-flash-lite",
        "gemini-2.5-pro",
        "gemini-2.0-flash",
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

fn parse_gemini_usage(response: &serde_json::Value, model: &str) -> ProviderUsage {
    let usage = response.get("usageMetadata").unwrap_or(&serde_json::Value::Null);
    ProviderUsage {
        provider: "gemini".to_string(),
        model: model.to_string(),
        input_tokens: usage.get("promptTokenCount").and_then(|value| value.as_u64()),
        cached_input_tokens: usage.get("cachedContentTokenCount").and_then(|value| value.as_u64()),
        output_tokens: usage.get("candidatesTokenCount").and_then(|value| value.as_u64()),
        reasoning_tokens: usage.get("thoughtsTokenCount").and_then(|value| value.as_u64()),
        total_tokens: usage.get("totalTokenCount").and_then(|value| value.as_u64()),
    }
}

fn call_gemini(api_key: &str, model: &str, history: &[serde_json::Value]) -> Result<ProviderReply, String> {
    if api_key.trim().is_empty() {
        return Err("Gemini API key is not configured. Please add one in Settings.".to_string());
    }

    let normalized_model = normalize_model_for_provider("gemini", model);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let body = build_gemini_request_body(history).map_err(|e| format!("Gemini request body error: {e}"))?;

    let mut last_error = "Gemini request failed before receiving a response".to_string();
    let mut attempted_models = Vec::new();

    for candidate_model in gemini_model_candidates(&client, api_key, &normalized_model) {
        attempted_models.push(candidate_model.clone());
        let endpoint = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{candidate_model}:generateContent"
        );
        let (status, response_text) = send_gemini_post_with_auth_fallback(&client, &endpoint, &body, api_key)?;

        if status.is_success() {
            let parsed: serde_json::Value = serde_json::from_str(&response_text)
                .map_err(|e| format!("Gemini response parsing failed: {e}"))?;

            let content = parsed
                .get("candidates")
                .and_then(|candidates| candidates.get(0))
                .and_then(|candidate| candidate.get("content"))
                .and_then(|content| content.get("parts"))
                .and_then(|parts| parts.get(0))
                .and_then(|part| part.get("text"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .ok_or_else(|| "Gemini returned no response content".to_string())?;
            return Ok(ProviderReply {
                content,
                usage: parse_gemini_usage(&parsed, &candidate_model),
            });
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

fn retrieval_index_path(app: &AppHandle, root: &Path) -> Result<PathBuf, String> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let mut path = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    path.push("retrieval");
    path.push(&digest[..20]);
    path.push("retrieval.sqlite3");
    Ok(path)
}

const RETRIEVAL_EMBEDDING_MODEL_ID: &str = "Qdrant/all-MiniLM-L6-v2-onnx";
const RETRIEVAL_EMBEDDING_MODEL_REVISION: &str = "5f1b8cd78bc4fb444dd171e59b18f3a3af89a079";
const RETRIEVAL_EMBEDDING_DIMENSIONS: usize = 384;
const RETRIEVAL_EMBEDDING_BATCH_SIZE: usize = 64;
static REGISTER_SQLITE_VEC: Once = Once::new();

struct EmbeddingAsset {
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

const RETRIEVAL_EMBEDDING_ASSETS: &[EmbeddingAsset] = &[
    EmbeddingAsset {
        name: "model.onnx",
        size: 90_387_630,
        sha256: "bbd7b466f6d58e646fdc2bd5fd67b2f5e93c0b687011bd4548c420f7bd46f0c5",
    },
    EmbeddingAsset {
        name: "tokenizer.json",
        size: 711_661,
        sha256: "da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0",
    },
    EmbeddingAsset {
        name: "config.json",
        size: 650,
        sha256: "1b4d8e2a3988377ed8b519a31d8d31025a25f1c5f8606998e8014111438efcd7",
    },
    EmbeddingAsset {
        name: "special_tokens_map.json",
        size: 695,
        sha256: "5d5b662e421ea9fac075174bb0688ee0d9431699900b90662acd44b2a350503a",
    },
    EmbeddingAsset {
        name: "tokenizer_config.json",
        size: 1_433,
        sha256: "bd2e06a5b20fd1b13ca988bedc8763d332d242381b4fbc98f8fead4524158f79",
    },
];

fn embedding_model_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    path.push("embedding-models");
    path.push("all-MiniLM-L6-v2-onnx");
    Ok(path)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_embedding_asset_path(path: &Path, asset: &EmbeddingAsset) -> Result<(), String> {
    let metadata =
        fs::metadata(&path).map_err(|_| format!("Missing embedding asset: {}", asset.name))?;
    if metadata.len() != asset.size {
        return Err(format!(
            "Embedding asset has the wrong size: {}",
            asset.name
        ));
    }
    if sha256_file(&path)? != asset.sha256 {
        return Err(format!(
            "Embedding asset failed SHA-256 verification: {}",
            asset.name
        ));
    }
    Ok(())
}

fn validate_embedding_asset(directory: &Path, asset: &EmbeddingAsset) -> Result<(), String> {
    validate_embedding_asset_path(&directory.join(asset.name), asset)
}

fn embedding_model_status_value(directory: &Path) -> serde_json::Value {
    let errors = RETRIEVAL_EMBEDDING_ASSETS
        .iter()
        .filter_map(|asset| validate_embedding_asset(directory, asset).err())
        .collect::<Vec<_>>();
    serde_json::json!({
        "status": if errors.is_empty() { "ready" } else { "model_assets_required" },
        "model": RETRIEVAL_EMBEDDING_MODEL_ID,
        "dimensions": RETRIEVAL_EMBEDDING_DIMENSIONS,
        "directory": directory.to_string_lossy().to_string(),
        "asset_count": RETRIEVAL_EMBEDDING_ASSETS.len(),
        "total_bytes": RETRIEVAL_EMBEDDING_ASSETS.iter().map(|asset| asset.size).sum::<u64>(),
        "errors": errors
    })
}

fn install_embedding_model_value(directory: &Path) -> Result<serde_json::Value, String> {
    fs::create_dir_all(directory)
        .map_err(|e| format!("Failed to create embedding model directory: {e}"))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .user_agent("physics-ide/9 local-embedding-installer")
        .build()
        .map_err(|e| format!("Failed to initialize embedding model installer: {e}"))?;

    for asset in RETRIEVAL_EMBEDDING_ASSETS {
        if validate_embedding_asset(directory, asset).is_ok() {
            continue;
        }
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            RETRIEVAL_EMBEDDING_MODEL_ID, RETRIEVAL_EMBEDDING_MODEL_REVISION, asset.name
        );
        let mut response = client
            .get(&url)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|e| format!("Failed to download {}: {e}", asset.name))?;
        let partial_path = directory.join(format!("{}.part", asset.name));
        let mut output = fs::File::create(&partial_path)
            .map_err(|e| format!("Failed to create temporary model asset {}: {e}", asset.name))?;
        std::io::copy(&mut response, &mut output)
            .map_err(|e| format!("Failed to write model asset {}: {e}", asset.name))?;
        output
            .flush()
            .map_err(|e| format!("Failed to flush model asset {}: {e}", asset.name))?;
        validate_embedding_asset_path(&partial_path, asset)?;
        let final_path = directory.join(asset.name);
        if final_path.exists() {
            fs::remove_file(&final_path)
                .map_err(|e| format!("Failed to replace model asset {}: {e}", asset.name))?;
        }
        fs::rename(&partial_path, &final_path)
            .map_err(|e| format!("Failed to activate model asset {}: {e}", asset.name))?;
    }

    let status = embedding_model_status_value(directory);
    if status["status"] != "ready" {
        return Err(
            "Embedding model installation did not pass integrity verification.".to_string(),
        );
    }
    let mut model = load_local_embedding_model(directory)?;
    let embeddings = model
        .embed(
            vec!["physics-ide local vector model activation probe"],
            Some(1),
        )
        .map_err(|e| format!("Local embedding model activation probe failed: {e}"))?;
    if embeddings.len() != 1 || embeddings[0].len() != RETRIEVAL_EMBEDDING_DIMENSIONS {
        return Err("Local embedding model returned an unexpected vector shape.".to_string());
    }
    let mut verified_status = status;
    verified_status["inference_status"] = serde_json::Value::String("ready".to_string());
    Ok(verified_status)
}

fn load_local_embedding_model(directory: &Path) -> Result<fastembed::TextEmbedding, String> {
    for asset in RETRIEVAL_EMBEDDING_ASSETS {
        validate_embedding_asset(directory, asset)?;
    }
    let read = |name: &str| {
        fs::read(directory.join(name))
            .map_err(|e| format!("Failed to load local embedding asset {name}: {e}"))
    };
    let tokenizer_files = fastembed::TokenizerFiles {
        tokenizer_file: read("tokenizer.json")?,
        config_file: read("config.json")?,
        special_tokens_map_file: read("special_tokens_map.json")?,
        tokenizer_config_file: read("tokenizer_config.json")?,
    };
    let model = fastembed::UserDefinedEmbeddingModel::new(read("model.onnx")?, tokenizer_files)
        .with_pooling(fastembed::Pooling::Mean);
    fastembed::TextEmbedding::try_new_from_user_defined(
        model,
        fastembed::InitOptionsUserDefined::new().with_intra_threads(2),
    )
    .map_err(|e| format!("Failed to initialize local embedding model: {e}"))
}

fn with_local_embedding_model<T, F>(
    state: &AppState,
    directory: &Path,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce(&mut fastembed::TextEmbedding) -> Result<T, String>,
{
    let mut cached = state
        .embedding_model
        .lock()
        .map_err(|_| "Local embedding model cache is unavailable.".to_string())?;
    if cached.is_none() {
        *cached = Some(load_local_embedding_model(directory)?);
    }
    operation(cached.as_mut().expect("embedding model initialized"))
}

fn register_sqlite_vec() {
    REGISTER_SQLITE_VEC.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

fn initialize_retrieval_index(connection: &rusqlite::Connection) -> Result<(), String> {
    let existing_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("Failed to read retrieval schema version: {e}"))?;
    if existing_version > 1 {
        return Err(format!(
            "Retrieval schema version {existing_version} is newer than supported version 1."
        ));
    }
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS retrieval_files (
                 path TEXT PRIMARY KEY,
                 size_bytes INTEGER NOT NULL,
                 modified_ns INTEGER NOT NULL,
                 content_hash TEXT NOT NULL,
                 chunk_count INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS retrieval_chunks USING fts5(
                 chunk_id UNINDEXED,
                 path UNINDEXED,
                 chunk_index UNINDEXED,
                 line_start UNINDEXED,
                 line_end UNINDEXED,
                 heading,
                 content,
                 tokenize='unicode61'
             );
             CREATE TABLE IF NOT EXISTS retrieval_vector_records (
                 vector_rowid INTEGER PRIMARY KEY,
                 chunk_id TEXT UNIQUE NOT NULL,
                 model_id TEXT NOT NULL,
                 dimensions INTEGER NOT NULL,
                 content_hash TEXT NOT NULL
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS retrieval_vectors USING vec0(
                 embedding float[384] distance_metric=cosine
             );
             CREATE TABLE IF NOT EXISTS retrieval_graph_edges (
                 from_chunk_id TEXT NOT NULL,
                 to_chunk_id TEXT NOT NULL,
                 relation_type TEXT NOT NULL,
                 evidence TEXT NOT NULL,
                 PRIMARY KEY (from_chunk_id, to_chunk_id, relation_type)
             );
             PRAGMA user_version=1;",
        )
        .map_err(|e| format!("Failed to initialize retrieval index: {e}"))
}

fn validate_retrieval_index_schema(connection: &rusqlite::Connection) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("Failed to read retrieval schema version: {e}"))?;
    if version != 1 {
        return Err(format!("Unsupported retrieval schema version: {version}"));
    }
    for query in [
        "SELECT path, size_bytes, modified_ns, content_hash, chunk_count FROM retrieval_files LIMIT 0",
        "SELECT chunk_id, path, chunk_index, line_start, line_end, heading, content FROM retrieval_chunks LIMIT 0",
        "SELECT vector_rowid, chunk_id, model_id, dimensions, content_hash FROM retrieval_vector_records LIMIT 0",
        "SELECT from_chunk_id, to_chunk_id, relation_type, evidence FROM retrieval_graph_edges LIMIT 0",
    ] {
        connection
            .prepare(query)
            .map_err(|e| format!("Retrieval index schema is incompatible: {e}"))?;
    }
    Ok(())
}

fn validate_retrieval_index_integrity(connection: &rusqlite::Connection) -> Result<(), String> {
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| format!("Failed to check retrieval index integrity: {e}"))?;
    if integrity != "ok" {
        return Err(format!("Retrieval index integrity check failed: {integrity}"));
    }
    validate_retrieval_index_schema(connection)
}

fn retrieval_index_sidecars(index_path: &Path) -> [PathBuf; 3] {
    [
        index_path.to_path_buf(),
        PathBuf::from(format!("{}-wal", index_path.to_string_lossy())),
        PathBuf::from(format!("{}-shm", index_path.to_string_lossy())),
    ]
}

fn quarantine_retrieval_index(index_path: &Path) -> Result<Vec<String>, String> {
    let parent = index_path.parent().unwrap_or_else(|| Path::new("."));
    let mut suffix = 0usize;
    let quarantine_dir = loop {
        let candidate = parent.join(if suffix == 0 {
            "corrupt-index".to_string()
        } else {
            format!("corrupt-index-{suffix}")
        });
        if !candidate.exists() {
            break candidate;
        }
        suffix += 1;
    };
    fs::create_dir_all(&quarantine_dir)
        .map_err(|e| format!("Failed to create retrieval quarantine directory: {e}"))?;
    let mut quarantined = Vec::new();
    for path in retrieval_index_sidecars(index_path) {
        if !path.exists() {
            continue;
        }
        let file_name = path.file_name().unwrap_or_default();
        let target = quarantine_dir.join(file_name);
        fs::rename(&path, &target).map_err(|e| {
            format!("Failed to quarantine retrieval index {}: {e}", path.display())
        })?;
        quarantined.push(target.to_string_lossy().to_string());
    }
    Ok(quarantined)
}

fn open_retrieval_index(index_path: &Path) -> Result<rusqlite::Connection, String> {
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create retrieval index directory: {e}"))?;
    }
    register_sqlite_vec();
    let connection = rusqlite::Connection::open(index_path)
        .map_err(|e| format!("Failed to open retrieval index: {e}"))?;
    if initialize_retrieval_index(&connection).is_ok()
        && validate_retrieval_index_integrity(&connection).is_ok()
    {
        return Ok(connection);
    }
    drop(connection);
    quarantine_retrieval_index(index_path)?;
    let recovered = rusqlite::Connection::open(index_path)
        .map_err(|e| format!("Failed to create recovered retrieval index: {e}"))?;
    initialize_retrieval_index(&recovered)?;
    validate_retrieval_index_integrity(&recovered)?;
    Ok(recovered)
}

fn normalize_graph_phrase(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn rebuild_retrieval_graph(connection: &mut rusqlite::Connection) -> Result<usize, String> {
    let chunks = {
        let mut statement = connection
            .prepare(
                "SELECT chunk_id, heading, content
                 FROM retrieval_chunks
                 ORDER BY path ASC, chunk_index ASC",
            )
            .map_err(|e| format!("Failed to prepare retrieval graph source query: {e}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to query retrieval graph sources: {e}"))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows.into_iter()
            .map(|(chunk_id, heading, content)| (chunk_id, (heading, content)))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let mut headings_by_first_word = std::collections::BTreeMap::<String, Vec<(String, String)>>::new();
    for (chunk_id, (heading, _)) in &chunks {
        let normalized = normalize_graph_phrase(heading);
        if normalized.len() < 4 {
            continue;
        }
        if let Some(first_word) = normalized.split_whitespace().next() {
            headings_by_first_word
                .entry(first_word.to_string())
                .or_default()
                .push((chunk_id.clone(), normalized));
        }
    }

    let relation_cues = [
        ("depends on", "depends_on"),
        ("derived from", "derived_from"),
        ("defines", "defines"),
        ("constrains", "constrains"),
        ("predicts", "predicts"),
        ("measured by", "measured_by"),
        ("alias for", "alias_for"),
        ("contradicts", "contradicts"),
        ("in conflict with", "contradicts"),
    ];
    let mut edges = std::collections::BTreeMap::<(String, String, String), String>::new();
    for (from_chunk_id, (_, content)) in &chunks {
        for line in content.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let normalized_line = normalize_graph_phrase(line);
            let line_words = normalized_line
                .split_whitespace()
                .collect::<std::collections::BTreeSet<_>>();
            for (cue, relation_type) in relation_cues {
                if !normalized_line.contains(cue) {
                    continue;
                }
                for word in &line_words {
                    let Some(candidates) = headings_by_first_word.get(*word) else {
                        continue;
                    };
                    for (to_chunk_id, heading) in candidates {
                        if from_chunk_id == to_chunk_id || !normalized_line.contains(heading) {
                            continue;
                        }
                        edges.insert(
                            (
                                from_chunk_id.clone(),
                                to_chunk_id.clone(),
                                relation_type.to_string(),
                            ),
                            line.chars().take(300).collect(),
                        );
                    }
                }
            }
        }
    }

    let transaction = connection
        .transaction()
        .map_err(|e| format!("Failed to start retrieval graph refresh: {e}"))?;
    transaction
        .execute("DELETE FROM retrieval_graph_edges", [])
        .map_err(|e| format!("Failed to clear retrieval graph: {e}"))?;
    for ((from_chunk_id, to_chunk_id, relation_type), evidence) in &edges {
        transaction
            .execute(
                "INSERT INTO retrieval_graph_edges (from_chunk_id, to_chunk_id, relation_type, evidence)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![from_chunk_id, to_chunk_id, relation_type, evidence],
            )
            .map_err(|e| format!("Failed to insert retrieval graph edge: {e}"))?;
    }
    transaction
        .commit()
        .map_err(|e| format!("Failed to commit retrieval graph refresh: {e}"))?;
    Ok(edges.len())
}

fn load_retrieval_graph_neighbors(
    connection: &rusqlite::Connection,
    chunk_id: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT edges.relation_type, 'outgoing', edges.to_chunk_id, edges.evidence,
                    chunks.path, chunks.chunk_index, chunks.line_start, chunks.line_end, chunks.heading, chunks.content
             FROM retrieval_graph_edges AS edges
             JOIN retrieval_chunks AS chunks ON chunks.chunk_id = edges.to_chunk_id
             WHERE edges.from_chunk_id = ?1
             UNION ALL
             SELECT edges.relation_type, 'incoming', edges.from_chunk_id, edges.evidence,
                    chunks.path, chunks.chunk_index, chunks.line_start, chunks.line_end, chunks.heading, chunks.content
             FROM retrieval_graph_edges AS edges
             JOIN retrieval_chunks AS chunks ON chunks.chunk_id = edges.from_chunk_id
             WHERE edges.to_chunk_id = ?1
             ORDER BY 1 ASC, 2 ASC, 5 ASC, 6 ASC
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare retrieval graph expansion: {e}"))?;
    let rows = statement
        .query_map(rusqlite::params![chunk_id, limit.clamp(1, 8) as i64], |row| {
            Ok(serde_json::json!({
                "relation_type": row.get::<_, String>(0)?,
                "direction": row.get::<_, String>(1)?,
                "chunk_id": row.get::<_, String>(2)?,
                "evidence": row.get::<_, String>(3)?,
                "relative_path": row.get::<_, String>(4)?,
                "chunk_index": row.get::<_, i64>(5)?,
                "line_start": row.get::<_, i64>(6)?,
                "line_end": row.get::<_, i64>(7)?,
                "heading": row.get::<_, String>(8)?,
                "content": row.get::<_, String>(9)?
            }))
        })
        .map_err(|e| format!("Failed to load retrieval graph expansion: {e}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}

fn retrieval_index_diagnostics(connection: &rusqlite::Connection) -> Result<serde_json::Value, String> {
    let file_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM retrieval_files", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count indexed files: {e}"))?;
    let chunk_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM retrieval_chunks", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count indexed chunks: {e}"))?;
    let vector_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM retrieval_vector_records", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count indexed vectors: {e}"))?;
    let graph_edge_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM retrieval_graph_edges", [], |row| row.get(0))
        .map_err(|e| format!("Failed to count retrieval graph edges: {e}"))?;
    let latest_source_modified_ns: i64 = connection
        .query_row("SELECT COALESCE(MAX(modified_ns), 0) FROM retrieval_files", [], |row| row.get(0))
        .map_err(|e| format!("Failed to inspect retrieval index freshness: {e}"))?;
    let active_model_fingerprint = format!("{}@{}", RETRIEVAL_EMBEDDING_MODEL_ID, RETRIEVAL_EMBEDDING_MODEL_REVISION);
    let stale_vector_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM retrieval_vector_records
             WHERE model_id != ?1 OR dimensions != ?2",
            rusqlite::params![active_model_fingerprint, RETRIEVAL_EMBEDDING_DIMENSIONS as i64],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to inspect vector freshness: {e}"))?;
    Ok(serde_json::json!({
        "indexed_files": file_count,
        "indexed_chunks": chunk_count,
        "indexed_vectors": vector_count,
        "graph_edges": graph_edge_count,
        "latest_source_modified_ns": latest_source_modified_ns,
        "active_model": RETRIEVAL_EMBEDDING_MODEL_ID,
        "active_model_revision": RETRIEVAL_EMBEDDING_MODEL_REVISION,
        "embedding_dimensions": RETRIEVAL_EMBEDDING_DIMENSIONS,
        "stale_vectors": stale_vector_count,
        "freshness_status": if stale_vector_count == 0 { "current" } else { "stale_vectors" }
    }))
}

fn encode_embedding(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn sync_retrieval_vectors_value<F>(
    index_path: &Path,
    mut embed: F,
) -> Result<serde_json::Value, String>
where
    F: FnMut(&[String]) -> Result<Vec<Vec<f32>>, String>,
{
    let mut connection = open_retrieval_index(index_path)?;
    let model_fingerprint = format!("{}@{}", RETRIEVAL_EMBEDDING_MODEL_ID, RETRIEVAL_EMBEDDING_MODEL_REVISION);
    let stale_rowids = {
        let mut statement = connection
            .prepare(
                "SELECT records.vector_rowid
                 FROM retrieval_vector_records AS records
                 LEFT JOIN retrieval_chunks AS chunks ON chunks.chunk_id = records.chunk_id
                 WHERE chunks.chunk_id IS NULL
                    OR records.model_id != ?1
                    OR records.dimensions != ?2
                    OR records.content_hash != records.chunk_id",
            )
            .map_err(|e| format!("Failed to prepare stale vector query: {e}"))?;
        let rows = statement
            .query_map(
                rusqlite::params![model_fingerprint, RETRIEVAL_EMBEDDING_DIMENSIONS as i64],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| format!("Failed to query stale vectors: {e}"))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows
    };
    let missing_chunks = {
        let mut statement = connection
            .prepare(
                "SELECT chunks.chunk_id, chunks.content
                 FROM retrieval_chunks AS chunks
                 LEFT JOIN retrieval_vector_records AS records
                   ON records.chunk_id = chunks.chunk_id
                  AND records.model_id = ?1
                  AND records.dimensions = ?2
                  AND records.content_hash = chunks.chunk_id
                 WHERE records.chunk_id IS NULL
                 ORDER BY chunks.path ASC, chunks.chunk_index ASC",
            )
            .map_err(|e| format!("Failed to prepare missing vector query: {e}"))?;
        let rows = statement
            .query_map(
                rusqlite::params![model_fingerprint, RETRIEVAL_EMBEDDING_DIMENSIONS as i64],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(|e| format!("Failed to query missing vectors: {e}"))?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        rows.into_iter().collect::<std::collections::BTreeMap<_, _>>().into_iter().collect::<Vec<_>>()
    };

    let mut pending = Vec::with_capacity(missing_chunks.len());
    for batch in missing_chunks.chunks(RETRIEVAL_EMBEDDING_BATCH_SIZE) {
        let texts = batch.iter().map(|(_, content)| content.clone()).collect::<Vec<_>>();
        let embeddings = embed(&texts)?;
        if embeddings.len() != batch.len()
            || embeddings.iter().any(|embedding| embedding.len() != RETRIEVAL_EMBEDDING_DIMENSIONS)
        {
            return Err("Local embedding model returned an unexpected vector batch shape.".to_string());
        }
        pending.extend(batch.iter().zip(embeddings).map(|((chunk_id, _), embedding)| {
            (chunk_id.clone(), embedding)
        }));
    }

    let transaction = connection
        .transaction()
        .map_err(|e| format!("Failed to start vector synchronization: {e}"))?;
    for rowid in &stale_rowids {
        transaction
            .execute("DELETE FROM retrieval_vectors WHERE rowid = ?1", [rowid])
            .map_err(|e| format!("Failed to delete stale vector: {e}"))?;
        transaction
            .execute("DELETE FROM retrieval_vector_records WHERE vector_rowid = ?1", [rowid])
            .map_err(|e| format!("Failed to delete stale vector metadata: {e}"))?;
    }
    for (chunk_id, embedding) in &pending {
        transaction
            .execute(
                "INSERT INTO retrieval_vector_records (chunk_id, model_id, dimensions, content_hash)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    chunk_id,
                    model_fingerprint,
                    RETRIEVAL_EMBEDDING_DIMENSIONS as i64,
                    chunk_id
                ],
            )
            .map_err(|e| format!("Failed to insert vector metadata: {e}"))?;
        let rowid = transaction.last_insert_rowid();
        transaction
            .execute(
                "INSERT INTO retrieval_vectors (rowid, embedding) VALUES (?1, ?2)",
                rusqlite::params![rowid, encode_embedding(embedding)],
            )
            .map_err(|e| format!("Failed to insert retrieval vector: {e}"))?;
    }
    transaction
        .commit()
        .map_err(|e| format!("Failed to commit vector synchronization: {e}"))?;

    Ok(serde_json::json!({
        "status": "ready",
        "embedded_chunks": pending.len(),
        "removed_vectors": stale_rowids.len(),
        "model": RETRIEVAL_EMBEDDING_MODEL_ID,
        "model_revision": RETRIEVAL_EMBEDDING_MODEL_REVISION,
        "dimensions": RETRIEVAL_EMBEDDING_DIMENSIONS
    }))
}

fn file_modified_ns(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn chunk_markdown(relative_path: &str, content: &str) -> Vec<RetrievalChunk> {
    const MAX_CHUNK_CHARACTERS: usize = 4_000;
    let lines = content.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut boundaries = vec![0usize];
    for (index, line) in lines.iter().enumerate().skip(1) {
        if line.trim_start().starts_with('#') {
            boundaries.push(index);
        }
    }
    boundaries.push(lines.len());

    let mut chunks = Vec::new();
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        let heading = lines[start]
            .trim()
            .trim_start_matches('#')
            .trim()
            .to_string();
        let mut part_start = start;
        let mut part_lines = Vec::new();
        let mut part_characters = 0usize;
        for line_index in start..end {
            if lines[line_index].chars().count() > MAX_CHUNK_CHARACTERS {
                if !part_lines.is_empty() {
                    let section = part_lines.join("\n").trim().to_string();
                    if !section.is_empty() {
                        let chunk_index = chunks.len();
                        chunks.push(RetrievalChunk {
                            id: stable_structural_id("chunk", relative_path, &section),
                            path: relative_path.to_string(),
                            chunk_index,
                            line_start: part_start + 1,
                            line_end: line_index,
                            heading: heading.clone(),
                            content: section,
                        });
                    }
                    part_lines.clear();
                    part_characters = 0;
                }
                let line_chars = lines[line_index].chars().collect::<Vec<_>>();
                for character_slice in line_chars.chunks(MAX_CHUNK_CHARACTERS) {
                    let section = character_slice.iter().collect::<String>();
                    let chunk_index = chunks.len();
                    chunks.push(RetrievalChunk {
                        id: stable_structural_id("chunk", relative_path, &section),
                        path: relative_path.to_string(),
                        chunk_index,
                        line_start: line_index + 1,
                        line_end: line_index + 1,
                        heading: heading.clone(),
                        content: section,
                    });
                }
                part_start = line_index + 1;
                continue;
            }
            let line_characters = lines[line_index].chars().count() + usize::from(!part_lines.is_empty());
            if !part_lines.is_empty() && part_characters + line_characters > MAX_CHUNK_CHARACTERS {
                let section = part_lines.join("\n").trim().to_string();
                if !section.is_empty() {
                    let chunk_index = chunks.len();
                    chunks.push(RetrievalChunk {
                        id: stable_structural_id("chunk", relative_path, &section),
                        path: relative_path.to_string(),
                        chunk_index,
                        line_start: part_start + 1,
                        line_end: line_index,
                        heading: heading.clone(),
                        content: section,
                    });
                }
                part_start = line_index;
                part_lines.clear();
                part_characters = 0;
            }
            part_lines.push(lines[line_index]);
            part_characters += line_characters;
        }
        let section = part_lines.join("\n").trim().to_string();
        if !section.is_empty() {
            let chunk_index = chunks.len();
            chunks.push(RetrievalChunk {
                id: stable_structural_id("chunk", relative_path, &section),
                path: relative_path.to_string(),
                chunk_index,
                line_start: part_start + 1,
                line_end: end,
                heading,
                content: section,
            });
        }
    }
    chunks
}

fn refresh_retrieval_index_value(root: &Path, index_path: &Path) -> Result<serde_json::Value, String> {
    let files = collect_markdown_files_recursive(root)?;
    let mut connection = open_retrieval_index(index_path)?;
    let existing = {
        let mut statement = connection
            .prepare("SELECT path, size_bytes, modified_ns FROM retrieval_files")
            .map_err(|e| format!("Failed to read retrieval metadata: {e}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to query retrieval metadata: {e}"))?
            .filter_map(Result::ok)
            .map(|(path, size, modified)| (path, (size, modified)))
            .collect::<std::collections::BTreeMap<_, _>>();
        rows
    };

    let transaction = connection.transaction().map_err(|e| format!("Failed to start retrieval refresh: {e}"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut indexed_files = 0usize;
    let mut unchanged_files = 0usize;
    let mut indexed_chunks = 0usize;

    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = file.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        if matches!(
            file_name,
            "ai_briefing.md"
                | "project_awareness.md"
                | "session_recap.md"
                | "workspace_tree.md"
                | "next_session_notes.md"
                | "first_session_startup_guide.md"
        ) {
            continue;
        }
        let metadata = match fs::metadata(&file) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let size = metadata.len();
        let modified = file_modified_ns(&metadata);
        seen.insert(relative.clone());
        if existing.get(&relative).is_some_and(|value| *value == (size, modified)) {
            unchanged_files += 1;
            continue;
        }

        let content = match fs::read_to_string(&file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let chunks = chunk_markdown(&relative, &content);
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());
        transaction
            .execute("DELETE FROM retrieval_chunks WHERE path = ?1", [&relative])
            .map_err(|e| format!("Failed to replace retrieval chunks: {e}"))?;
        for chunk in &chunks {
            transaction
                .execute(
                    "INSERT INTO retrieval_chunks (chunk_id, path, chunk_index, line_start, line_end, heading, content)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        chunk.id,
                        chunk.path,
                        chunk.chunk_index as i64,
                        chunk.line_start as i64,
                        chunk.line_end as i64,
                        chunk.heading,
                        chunk.content
                    ],
                )
                .map_err(|e| format!("Failed to insert retrieval chunk: {e}"))?;
        }
        transaction
            .execute(
                "INSERT INTO retrieval_files (path, size_bytes, modified_ns, content_hash, chunk_count)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path) DO UPDATE SET
                   size_bytes=excluded.size_bytes,
                   modified_ns=excluded.modified_ns,
                   content_hash=excluded.content_hash,
                   chunk_count=excluded.chunk_count",
                rusqlite::params![relative, size, modified, content_hash, chunks.len() as i64],
            )
            .map_err(|e| format!("Failed to update retrieval metadata: {e}"))?;
        indexed_files += 1;
        indexed_chunks += chunks.len();
    }

    let deleted = existing
        .keys()
        .filter(|path| !seen.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    for path in &deleted {
        transaction.execute("DELETE FROM retrieval_chunks WHERE path = ?1", [path])
            .map_err(|e| format!("Failed to delete stale retrieval chunks: {e}"))?;
        transaction.execute("DELETE FROM retrieval_files WHERE path = ?1", [path])
            .map_err(|e| format!("Failed to delete stale retrieval metadata: {e}"))?;
    }
    transaction.commit().map_err(|e| format!("Failed to commit retrieval refresh: {e}"))?;
    let graph_edges = rebuild_retrieval_graph(&mut connection)?;

    Ok(serde_json::json!({
        "status": "ok",
        "index_path": index_path.to_string_lossy().to_string(),
        "indexed_files": indexed_files,
        "unchanged_files": unchanged_files,
        "deleted_files": deleted.len(),
        "indexed_chunks": indexed_chunks,
        "graph_edges": graph_edges,
        "embedding_status": "model_assets_required",
        "embedding_model": RETRIEVAL_EMBEDDING_MODEL_ID,
        "embedding_dimensions": RETRIEVAL_EMBEDDING_DIMENSIONS,
        "vector_extension": "sqlite-vec",
        "search_mode": "fts5_lexical_with_neighbors"
    }))
}

fn fts_query(query: &str) -> String {
    build_probe_terms(query)
        .into_iter()
        .take(12)
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[derive(Default)]
struct RetrievalFusionCandidate {
    score: f64,
    lexical_rank: Option<usize>,
    lexical_score: Option<f64>,
    vector_rank: Option<usize>,
    vector_distance: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RetrievalQueryMode {
    Lexical,
    Vector,
    Hybrid,
}

fn query_retrieval_index_hybrid_value(
    index_path: &Path,
    query: &str,
    limit: usize,
    query_embedding: Option<&[f32]>,
    mode: RetrievalQueryMode,
) -> Result<serde_json::Value, String> {
    const RRF_K: f64 = 60.0;
    let match_query = fts_query(query);
    let connection = open_retrieval_index(index_path)?;
    let result_limit = limit.clamp(1, 20);
    let candidate_limit = (result_limit * 4).clamp(20, 80);
    let mut candidates = std::collections::BTreeMap::<String, RetrievalFusionCandidate>::new();
    let mut lexical_candidate_count = 0usize;
    if mode != RetrievalQueryMode::Vector && !match_query.is_empty() {
        let lexical_matches = {
            let mut statement = connection
                .prepare(
                    "SELECT chunk_id, bm25(retrieval_chunks) AS rank
                     FROM retrieval_chunks
                     WHERE retrieval_chunks MATCH ?1
                     ORDER BY rank ASC, path ASC, chunk_index ASC
                     LIMIT ?2",
                )
                .map_err(|e| format!("Failed to prepare lexical retrieval query: {e}"))?;
            let rows = statement
                .query_map(rusqlite::params![match_query, candidate_limit as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })
                .map_err(|e| format!("Failed to execute lexical retrieval query: {e}"))?
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
            rows
        };
        let mut seen = std::collections::BTreeSet::new();
        for (chunk_id, lexical_score) in lexical_matches {
            if !seen.insert(chunk_id.clone()) {
                continue;
            }
            lexical_candidate_count += 1;
            let candidate = candidates.entry(chunk_id).or_default();
            candidate.lexical_rank = Some(lexical_candidate_count);
            candidate.lexical_score = Some(lexical_score);
            candidate.score += 1.0 / (RRF_K + lexical_candidate_count as f64);
        }
    }

    let mut vector_candidate_count = 0usize;
    if mode != RetrievalQueryMode::Lexical {
        if let Some(embedding) = query_embedding.filter(|values| values.len() == RETRIEVAL_EMBEDDING_DIMENSIONS) {
        let model_fingerprint = format!("{}@{}", RETRIEVAL_EMBEDDING_MODEL_ID, RETRIEVAL_EMBEDDING_MODEL_REVISION);
        let vector_matches = {
            let mut statement = connection
                .prepare(
                    "SELECT records.chunk_id, vectors.distance
                     FROM retrieval_vectors AS vectors
                     JOIN retrieval_vector_records AS records ON records.vector_rowid = vectors.rowid
                                         WHERE vectors.embedding MATCH ?1 AND k = ?2
                                             AND records.model_id = ?3
                                             AND records.dimensions = ?4
                     ORDER BY vectors.distance ASC, records.chunk_id ASC",
                )
                .map_err(|e| format!("Failed to prepare vector retrieval query: {e}"))?;
            let rows = statement
                .query_map(
                    rusqlite::params![
                        encode_embedding(embedding),
                        candidate_limit as i64,
                        model_fingerprint,
                        RETRIEVAL_EMBEDDING_DIMENSIONS as i64
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
                )
                .map_err(|e| format!("Failed to execute vector retrieval query: {e}"))?
                .filter_map(Result::ok)
                .collect::<Vec<_>>();
            rows
        };
        for (index, (chunk_id, distance)) in vector_matches.into_iter().enumerate() {
            let rank = index + 1;
            vector_candidate_count += 1;
            let candidate = candidates.entry(chunk_id).or_default();
            candidate.vector_rank = Some(rank);
            candidate.vector_distance = Some(distance);
            candidate.score += 1.0 / (RRF_K + rank as f64);
        }
        }
    }

    let mut ranked = candidates.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_id, left), (right_id, right)| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left_id.cmp(right_id))
    });
    if mode == RetrievalQueryMode::Hybrid && result_limit >= 2 {
        let top_lexical = ranked
            .iter()
            .filter_map(|(chunk_id, candidate)| candidate.lexical_rank.map(|rank| (rank, chunk_id.clone())))
            .min();
        let top_vector = ranked
            .iter()
            .filter_map(|(chunk_id, candidate)| candidate.vector_rank.map(|rank| (rank, chunk_id.clone())))
            .min();
        let mut selected = std::collections::BTreeSet::new();
        if let Some((_, chunk_id)) = top_lexical {
            selected.insert(chunk_id);
        }
        if let Some((_, chunk_id)) = top_vector {
            selected.insert(chunk_id);
        }
        for (chunk_id, _) in &ranked {
            if selected.len() >= result_limit {
                break;
            }
            selected.insert(chunk_id.clone());
        }
        ranked.retain(|(chunk_id, _)| selected.contains(chunk_id));
    } else {
        ranked.truncate(result_limit);
    }

    let mut results = Vec::new();
    for (chunk_id, fusion) in ranked {
        let (path, chunk_index, line_start, line_end, heading, content) = connection
            .query_row(
                "SELECT path, chunk_index, line_start, line_end, heading, content
                 FROM retrieval_chunks
                 WHERE chunk_id = ?1
                 ORDER BY path ASC, chunk_index ASC
                 LIMIT 1",
                [&chunk_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .map_err(|e| format!("Failed to load fused retrieval result: {e}"))?;
        let mut neighbors = Vec::new();
        let mut neighbor_statement = connection
            .prepare(
                "SELECT chunk_index, line_start, line_end, heading, content
                 FROM retrieval_chunks
                 WHERE path = ?1 AND chunk_index IN (?2, ?3)
                 ORDER BY chunk_index ASC",
            )
            .map_err(|e| format!("Failed to prepare neighbor query: {e}"))?;
        let neighbor_rows = neighbor_statement
            .query_map(
                rusqlite::params![path, chunk_index - 1, chunk_index + 1],
                |row| {
                    Ok(serde_json::json!({
                        "chunk_index": row.get::<_, i64>(0)?,
                        "line_start": row.get::<_, i64>(1)?,
                        "line_end": row.get::<_, i64>(2)?,
                        "heading": row.get::<_, String>(3)?,
                        "content": row.get::<_, String>(4)?
                    }))
                },
            )
            .map_err(|e| format!("Failed to load retrieval neighbors: {e}"))?;
        neighbors.extend(neighbor_rows.filter_map(Result::ok));
        let graph_neighbors = load_retrieval_graph_neighbors(&connection, &chunk_id, 4)?;
        results.push(serde_json::json!({
            "chunk_id": chunk_id,
            "relative_path": path,
            "chunk_index": chunk_index,
            "line_start": line_start,
            "line_end": line_end,
            "heading": heading,
            "content": content,
            "rank": fusion.score,
            "fusion_score": fusion.score,
            "lexical_rank": fusion.lexical_rank,
            "lexical_score": fusion.lexical_score,
            "vector_rank": fusion.vector_rank,
            "vector_distance": fusion.vector_distance,
            "neighbors": neighbors,
            "graph_neighbors": graph_neighbors
        }));
    }

    let vector_used = query_embedding.is_some() && vector_candidate_count > 0;
    let index_diagnostics = retrieval_index_diagnostics(&connection)?;
    Ok(serde_json::json!({
        "status": "ok",
        "search_mode": match mode {
            RetrievalQueryMode::Hybrid if vector_used => "hybrid_rrf_fts5_vector_with_neighbors",
            RetrievalQueryMode::Vector if vector_used => "vector_only_with_neighbors",
            _ => "fts5_lexical_with_neighbors"
        },
        "fusion": if mode == RetrievalQueryMode::Hybrid && vector_used { "reciprocal_rank_fusion" } else if mode == RetrievalQueryMode::Vector && vector_used { "vector_only" } else { "lexical_only" },
        "rrf_k": if mode == RetrievalQueryMode::Hybrid && vector_used { Some(RRF_K) } else { None },
        "lexical_candidates": lexical_candidate_count,
        "vector_candidates": vector_candidate_count,
        "modality_coverage_reserved": mode == RetrievalQueryMode::Hybrid && result_limit >= 2 && lexical_candidate_count > 0 && vector_candidate_count > 0,
        "vector_status": if vector_used { "used" } else if query_embedding.is_some() { "index_empty" } else { "unavailable" },
        "index": index_diagnostics,
        "query": query,
        "results": results
    }))
}

fn query_retrieval_index_value(index_path: &Path, query: &str, limit: usize) -> Result<serde_json::Value, String> {
    query_retrieval_index_hybrid_value(index_path, query, limit, None, RetrievalQueryMode::Lexical)
}

const DEFAULT_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS: usize = 6_000;
const MIN_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS: usize = 500;
const MAX_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS: usize = 24_000;

fn take_budgeted_text(value: &str, maximum: usize) -> (String, bool) {
    let character_count = value.chars().count();
    if character_count <= maximum {
        return (value.to_string(), false);
    }
    (value.chars().take(maximum).collect(), true)
}

fn build_retrieval_evidence_packet(
    retrieval: &serde_json::Value,
    requested_budget: Option<usize>,
) -> serde_json::Value {
    let budget = requested_budget
        .unwrap_or(DEFAULT_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS)
        .clamp(
            MIN_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS,
            MAX_RETRIEVAL_EVIDENCE_BUDGET_CHARACTERS,
        );
    let rows = retrieval["results"].as_array().cloned().unwrap_or_default();
    let mut remaining = budget;
    let mut included = Vec::new();
    let mut included_sources = std::collections::BTreeSet::new();
    let mut excluded_sources = std::collections::BTreeSet::new();
    let mut included_snippets = 0usize;
    let mut excluded_snippets = 0usize;
    let mut truncated_snippets = 0usize;

    for row in &rows {
        let relative_path = row["relative_path"].as_str().unwrap_or_default();
        if remaining == 0 {
            if !relative_path.is_empty() {
                excluded_sources.insert(relative_path.to_string());
            }
            excluded_snippets += 1
                + row["neighbors"].as_array().map(|items| items.len().min(2)).unwrap_or(0)
                + row["graph_neighbors"].as_array().map(|items| items.len().min(2)).unwrap_or(0);
            continue;
        }

        let mut snippets = Vec::new();
        let mut append_snippet = |kind: &str, label: String, content: &str, source_path: &str, maximum: usize| {
            if content.trim().is_empty() || remaining == 0 {
                excluded_snippets += usize::from(!content.trim().is_empty());
                return;
            }
            let allowance = remaining.min(maximum);
            let (selected, truncated) = take_budgeted_text(content.trim(), allowance);
            if selected.is_empty() {
                excluded_snippets += 1;
                return;
            }
            remaining = remaining.saturating_sub(selected.chars().count());
            included_snippets += 1;
            truncated_snippets += usize::from(truncated);
            if !source_path.is_empty() {
                included_sources.insert(source_path.to_string());
            }
            snippets.push(serde_json::json!({
                "kind": kind,
                "label": label,
                "content": selected,
                "source_path": source_path,
                "truncated": truncated
            }));
        };

        append_snippet(
            "primary",
            format!("L{}-{}", row["line_start"], row["line_end"]),
            row["content"].as_str().unwrap_or_default(),
            relative_path,
            700,
        );
        for neighbor in row["neighbors"].as_array().into_iter().flatten().take(2) {
            append_snippet(
                "adjacent",
                format!("L{}-{}", neighbor["line_start"], neighbor["line_end"]),
                neighbor["content"].as_str().unwrap_or_default(),
                relative_path,
                450,
            );
        }
        for neighbor in row["graph_neighbors"].as_array().into_iter().flatten().take(2) {
            let graph_path = neighbor["relative_path"].as_str().unwrap_or(relative_path);
            append_snippet(
                "graph",
                format!(
                    "{} {} L{}-{}",
                    neighbor["direction"].as_str().unwrap_or("related"),
                    neighbor["relation_type"].as_str().unwrap_or("relation"),
                    neighbor["line_start"],
                    neighbor["line_end"]
                ),
                neighbor["content"].as_str().unwrap_or_default(),
                graph_path,
                450,
            );
        }
        if snippets.is_empty() {
            if !relative_path.is_empty() {
                excluded_sources.insert(relative_path.to_string());
            }
            continue;
        }
        included.push(serde_json::json!({
            "relative_path": relative_path,
            "chunk_id": row["chunk_id"],
            "fusion_score": row["fusion_score"],
            "lexical_rank": row["lexical_rank"],
            "vector_rank": row["vector_rank"],
            "snippets": snippets
        }));
    }
    for row in &rows {
        if let Some(path) = row["relative_path"].as_str() {
            if !included_sources.contains(path) {
                excluded_sources.insert(path.to_string());
            }
        }
    }
    let mut provider_lines = vec!["@retrieval-evidence-v1".to_string()];
    for row in &included {
        provider_lines.push(format!(
            "S|{}|{}|score={}",
            row["relative_path"].as_str().unwrap_or_default(),
            row["chunk_id"].as_str().unwrap_or_default(),
            row["fusion_score"]
        ));
        for snippet in row["snippets"].as_array().into_iter().flatten() {
            provider_lines.push(format!(
                "{}|{}|{}",
                snippet["kind"].as_str().unwrap_or("evidence"),
                snippet["label"].as_str().unwrap_or_default(),
                snippet["content"].as_str().unwrap_or_default()
            ));
        }
    }
    let provider_text_full = provider_lines.join("\n");
    let (provider_text, provider_text_truncated) = take_budgeted_text(&provider_text_full, budget);
    let used = provider_text.chars().count();
    remaining = budget.saturating_sub(used);
    serde_json::json!({
        "evidence": included,
        "provider_text": provider_text,
        "diagnostics": {
            "budget_characters": budget,
            "budget_estimated_tokens": estimate_token_count(budget),
            "used_characters": used,
            "used_estimated_tokens": estimate_token_count(used),
            "remaining_characters": remaining,
            "candidate_rows": rows.len(),
            "included_rows": included.len(),
            "excluded_rows": rows.len().saturating_sub(included.len()),
            "included_snippets": included_snippets,
            "excluded_snippets": excluded_snippets,
            "truncated_snippets": truncated_snippets,
            "provider_text_truncated": provider_text_truncated,
            "included_sources": included_sources,
            "excluded_sources": excluded_sources,
            "method": "character_budget_4_chars_per_estimated_token",
            "truncation_policy": "rank_order_primary_then_adjacent_then_graph"
        }
    })
}

struct RetrievalBenchmarkCase {
    id: &'static str,
    family: &'static str,
    category: &'static str,
    query: &'static str,
    expected_heading: &'static str,
    expected_graph_heading: Option<&'static str>,
}

const RETRIEVAL_BENCHMARK_CASES: &[RetrievalBenchmarkCase] = &[
    RetrievalBenchmarkCase {
        id: "lambda-equation-symbol",
        family: "lambda_cdm_style",
        category: "equation_and_uncommon_symbol",
        query: "Where does Omega_m enter the expansion equation?",
        expected_heading: "Expansion equation",
        expected_graph_heading: None,
    },
    RetrievalBenchmarkCase {
        id: "bmi-mechanism-alias",
        family: "bimodal_interaction_style",
        category: "mechanism_alias",
        query: "What establishes particle inertia?",
        expected_heading: "Neutrino mechanism",
        expected_graph_heading: None,
    },
    RetrievalBenchmarkCase {
        id: "geocentric-contradiction",
        family: "geocentric_style",
        category: "contradiction_and_observation",
        query: "Which observation contradicts stationary Earth?",
        expected_heading: "Parallax contradiction",
        expected_graph_heading: None,
    },
    RetrievalBenchmarkCase {
        id: "qft-interaction-alias",
        family: "qft_style",
        category: "equation_alias",
        query: "What governs bosonic field feedback?",
        expected_heading: "Bosonic glossary",
        expected_graph_heading: Some("Scalar interaction"),
    },
    RetrievalBenchmarkCase {
        id: "cross-section-dependency",
        family: "lambda_cdm_style",
        category: "cross_section_relation",
        query: "What does luminosity distance depend on?",
        expected_heading: "Luminosity mapping",
        expected_graph_heading: Some("Expansion equation"),
    },
    RetrievalBenchmarkCase {
        id: "bmi-mode-locking-experiment",
        family: "bimodal_interaction_style",
        category: "experiment_and_graph_relation",
        query: "Which experiment uses protocol EX_27 to test boundary mode locking?",
        expected_heading: "Mode locking experiment",
        expected_graph_heading: Some("Boundary condition"),
    },
    RetrievalBenchmarkCase {
        id: "qft-uncommon-symbol",
        family: "qft_style",
        category: "uncommon_symbol_definition",
        query: "Where is chi_perp defined?",
        expected_heading: "Transverse invariant",
        expected_graph_heading: None,
    },
    RetrievalBenchmarkCase {
        id: "geocentric-instrument-alias",
        family: "geocentric_style",
        category: "experiment_alias",
        query: "Which instrument tests the fixed-frame prediction?",
        expected_heading: "Fixed-frame prediction",
        expected_graph_heading: Some("Gyroscope experiment"),
    },
];

fn write_retrieval_benchmark_fixture(root: &Path) -> Result<String, String> {
    let documents = [
        (
            "lambda_cdm.md",
            "# Expansion equation\nThe relation E(z)^2 = Omega_m(1+z)^3 + Omega_lambda specifies background evolution.\n\n## Luminosity mapping\nLuminosity distance depends on Expansion equation.\n\n## Distance catalog\nA catalog entry stores unrelated distance metadata.\n",
        ),
        (
            "bimodal_interaction.md",
            "# Neutrino mechanism\nThe effective mass emerges from a bimodal interaction eigenvalue at the boundary.\n\n## Boundary condition\nThe paired modes lock when the outer coupling reaches the critical surface.\n\n## Mode locking experiment\nProtocol EX_27 tests boundary mode locking and depends on Boundary condition.\n\n## Inertia index\nParticle inertia is an index topic only and supplies no mechanism.\n",
        ),
        (
            "geocentric.md",
            "# Parallax contradiction\nAnnual stellar parallax contradicts a strictly stationary Earth construction.\n\n## Gyroscope experiment\nA ring-laser apparatus measures rotational drift against the proposed immobile terrestrial frame.\n\n## Fixed-frame prediction\nThe fixed-frame prediction is measured by Gyroscope experiment.\n\n## Instrument log\nThe telescope log records an unrelated calibration sequence.\n",
        ),
        (
            "qft.md",
            "# Scalar interaction\nThe quartic lambda phi^4 contribution controls self-coupling in the Lagrangian.\n\n## Transverse invariant\nThe uncommon symbol chi_perp := chi - (chi dot n)n denotes the projected field component.\n\n## Bosonic glossary\nBosonic field feedback is an alias for Scalar interaction.\n",
        ),
        (
            "vector_decoys.md",
            "# Matter formula decoy one\nA matter fraction enters a cosmic expansion formula in this unrelated teaching example.\n\n## Matter formula decoy two\nThe density contribution appears inside a background evolution equation without project provenance.\n\n## Matter formula decoy three\nA cosmological matter term participates in an expansion relation used only as a distractor.\n\n## Parallax decoy one\nAn observation challenges a fixed terrestrial frame in a generic astronomy exercise.\n\n## Parallax decoy two\nA measured stellar shift conflicts with an unmoving observer assumption outside the loaded theory.\n\n## Parallax decoy three\nA yearly angular displacement tests whether the terrestrial platform remains fixed.\n\n## Distance decoy one\nA brightness-based distance depends on cosmic expansion in an unrelated reference model.\n\n## Distance decoy two\nThe inferred source distance follows the background expansion history in a generic example.\n\n## Distance decoy three\nAn observational distance mapping relies on the universal growth relation outside this theory.\n\n## Protocol decoy one\nAn experimental protocol tests mode synchronization at a generic boundary.\n\n## Protocol decoy two\nA laboratory procedure examines coupled mode locking near an outer surface.\n\n## Protocol decoy three\nAn unrelated trial measures whether paired oscillations lock at a threshold.\n\n## Symbol decoy one\nA projected transverse component is defined for a generic field.\n\n## Symbol decoy two\nThe orthogonal invariant removes the normal contribution from a vector.\n\n## Symbol decoy three\nA field projection onto the tangent plane supplies an auxiliary definition.\n",
        ),
    ];
    fs::create_dir_all(root).map_err(|e| format!("Failed to create retrieval benchmark fixture: {e}"))?;
    let mut fingerprint_source = String::new();
    for (path, content) in documents {
        fs::write(root.join(path), content)
            .map_err(|e| format!("Failed to write retrieval benchmark fixture {path}: {e}"))?;
        fingerprint_source.push_str(path);
        fingerprint_source.push_str(content);
    }
    for case in RETRIEVAL_BENCHMARK_CASES {
        fingerprint_source.push_str(case.id);
        fingerprint_source.push_str(case.query);
        fingerprint_source.push_str(case.expected_heading);
    }
    let mut hasher = Sha256::new();
    hasher.update(fingerprint_source.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn benchmark_heading_rank(result: &serde_json::Value, expected_heading: &str) -> Option<usize> {
    result["results"]
        .as_array()?
        .iter()
        .position(|row| row["heading"] == expected_heading)
        .map(|index| index + 1)
}

fn benchmark_mrr(rank_sum: f64, case_count: usize) -> f64 {
    if case_count == 0 { 0.0 } else { rank_sum / case_count as f64 }
}

fn run_retrieval_benchmark_value<F>(
    benchmark_root: &Path,
    mut embed: F,
) -> Result<serde_json::Value, String>
where
    F: FnMut(&[String]) -> Result<Vec<Vec<f32>>, String>,
{
    let _ = fs::remove_dir_all(benchmark_root);
    let result = (|| {
        let fixture_fingerprint = write_retrieval_benchmark_fixture(benchmark_root)?;
        let index_path = benchmark_root.join("retrieval.sqlite3");
        refresh_retrieval_index_value(benchmark_root, &index_path)?;
        sync_retrieval_vectors_value(&index_path, |texts| embed(texts))?;

        let mut lexical_hits = 0usize;
        let mut vector_hits = 0usize;
        let mut hybrid_hits = 0usize;
        let mut graph_hits = 0usize;
        let mut graph_cases = 0usize;
        let mut lexical_reciprocal_rank = 0.0f64;
        let mut vector_reciprocal_rank = 0.0f64;
        let mut hybrid_reciprocal_rank = 0.0f64;
        let mut cases = Vec::new();
        for case in RETRIEVAL_BENCHMARK_CASES {
            let mut query_vectors = embed(&[case.query.to_string()])?;
            let query_vector = query_vectors
                .pop()
                .ok_or_else(|| format!("Benchmark embedding was missing for {}.", case.id))?;
            let lexical = query_retrieval_index_hybrid_value(
                &index_path,
                case.query,
                3,
                None,
                RetrievalQueryMode::Lexical,
            )?;
            let vector = query_retrieval_index_hybrid_value(
                &index_path,
                case.query,
                3,
                Some(&query_vector),
                RetrievalQueryMode::Vector,
            )?;
            let hybrid = query_retrieval_index_hybrid_value(
                &index_path,
                case.query,
                3,
                Some(&query_vector),
                RetrievalQueryMode::Hybrid,
            )?;
            let lexical_rank = benchmark_heading_rank(&lexical, case.expected_heading);
            let vector_rank = benchmark_heading_rank(&vector, case.expected_heading);
            let hybrid_rank = benchmark_heading_rank(&hybrid, case.expected_heading);
            lexical_hits += usize::from(lexical_rank.is_some());
            vector_hits += usize::from(vector_rank.is_some());
            hybrid_hits += usize::from(hybrid_rank.is_some());
            lexical_reciprocal_rank += lexical_rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0);
            vector_reciprocal_rank += vector_rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0);
            hybrid_reciprocal_rank += hybrid_rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0);

            let graph_hit = case.expected_graph_heading.map(|expected| {
                hybrid["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|row| row["heading"] == case.expected_heading)
                    .flat_map(|row| row["graph_neighbors"].as_array().into_iter().flatten())
                    .any(|neighbor| neighbor["heading"] == expected)
            });
            if let Some(hit) = graph_hit {
                graph_cases += 1;
                graph_hits += usize::from(hit);
            }
            cases.push(serde_json::json!({
                "id": case.id,
                "family": case.family,
                "category": case.category,
                "query": case.query,
                "expected_heading": case.expected_heading,
                "lexical_rank": lexical_rank,
                "vector_rank": vector_rank,
                "hybrid_rank": hybrid_rank,
                "graph_hit": graph_hit
            }));
        }

        let case_count = RETRIEVAL_BENCHMARK_CASES.len();
        let hybrid_outperforms_baselines = hybrid_hits > lexical_hits && hybrid_hits > vector_hits;
        let hybrid_non_inferior = hybrid_hits >= lexical_hits && hybrid_hits >= vector_hits;
        let graph_complete = graph_hits == graph_cases;
        let lexical_mrr = benchmark_mrr(lexical_reciprocal_rank, case_count);
        let vector_mrr = benchmark_mrr(vector_reciprocal_rank, case_count);
        let hybrid_mrr = benchmark_mrr(hybrid_reciprocal_rank, case_count);
        let hybrid_recall = hybrid_hits as f64 / case_count as f64;
        let acceptance_passed = hybrid_recall == 1.0
            && graph_complete
            && hybrid_non_inferior
            && hybrid_mrr >= lexical_mrr;
        Ok(serde_json::json!({
            "status": if acceptance_passed { "pass" } else { "fail" },
            "fixture": "physics-ide.retrieval-benchmark/v2",
            "fixture_fingerprint": fixture_fingerprint,
            "case_count": case_count,
            "family_count": 4,
            "recall_at_3": {
                "lexical": lexical_hits as f64 / case_count as f64,
                "vector": vector_hits as f64 / case_count as f64,
                "hybrid": hybrid_recall
            },
            "mrr_at_3": {
                "lexical": lexical_mrr,
                "vector": vector_mrr,
                "hybrid": hybrid_mrr
            },
            "hits_at_3": {
                "lexical": lexical_hits,
                "vector": vector_hits,
                "hybrid": hybrid_hits
            },
            "graph": {
                "hits": graph_hits,
                "cases": graph_cases,
                "complete": graph_complete
            },
            "hybrid_outperforms_baselines": hybrid_outperforms_baselines,
            "hybrid_non_inferior": hybrid_non_inferior,
            "acceptance": {
                "passed": acceptance_passed,
                "required_hybrid_recall_at_3": 1.0,
                "require_complete_graph_coverage": true,
                "require_non_inferiority": true,
                "require_hybrid_mrr_not_below_lexical": true
            },
            "strict_superiority_status": if hybrid_outperforms_baselines { "pass" } else { "open" },
            "cases": cases
        }))
    })();
    let _ = fs::remove_dir_all(benchmark_root);
    result
}

fn delete_retrieval_index_value(index_path: &Path) -> Result<serde_json::Value, String> {
    let mut deleted_files = 0usize;
    for path in retrieval_index_sidecars(index_path) {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete local retrieval index {}: {e}", path.display()))?;
            deleted_files += 1;
        }
    }
    if let Some(parent) = index_path.parent() {
        let _ = fs::remove_dir(parent);
    }

    Ok(serde_json::json!({
        "status": if deleted_files > 0 { "deleted" } else { "not_found" },
        "deleted_files": deleted_files,
        "index_path": index_path.to_string_lossy().to_string()
    }))
}

fn inspect_retrieval_index_value(index_path: &Path) -> serde_json::Value {
    let quarantine_count = index_path
        .parent()
        .and_then(|parent| fs::read_dir(parent).ok())
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("corrupt-index"))
                .count()
        })
        .unwrap_or(0);
    if !index_path.exists() {
        return serde_json::json!({
            "status": "not_built",
            "index_path": index_path.to_string_lossy().to_string(),
            "quarantined_indexes": quarantine_count
        });
    }

    register_sqlite_vec();
    let connection = match rusqlite::Connection::open_with_flags(
        index_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            return serde_json::json!({
                "status": "corrupt",
                "index_path": index_path.to_string_lossy().to_string(),
                "error": error.to_string(),
                "quarantined_indexes": quarantine_count
            });
        }
    };
    let integrity = connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .unwrap_or_else(|error| format!("error:{error}"));
    if integrity != "ok" {
        return serde_json::json!({
            "status": "corrupt",
            "index_path": index_path.to_string_lossy().to_string(),
            "integrity": integrity,
            "quarantined_indexes": quarantine_count
        });
    }
    if let Err(error) = validate_retrieval_index_schema(&connection) {
        return serde_json::json!({
            "status": "incompatible",
            "index_path": index_path.to_string_lossy().to_string(),
            "integrity": integrity,
            "error": error,
            "quarantined_indexes": quarantine_count
        });
    }
    match retrieval_index_diagnostics(&connection) {
        Ok(diagnostics) => serde_json::json!({
            "status": "ready",
            "index_path": index_path.to_string_lossy().to_string(),
            "database_bytes": fs::metadata(index_path).map(|metadata| metadata.len()).unwrap_or(0),
            "integrity": integrity,
            "quarantined_indexes": quarantine_count,
            "index": diagnostics
        }),
        Err(error) => serde_json::json!({
            "status": "incompatible",
            "index_path": index_path.to_string_lossy().to_string(),
            "integrity": integrity,
            "error": error,
            "quarantined_indexes": quarantine_count
        }),
    }
}

fn refresh_retrieval_index_command_value(
    root: &Path,
    index_path: &Path,
    model_directory: &Path,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let mut result = refresh_retrieval_index_value(root, index_path)?;
    let model_status = embedding_model_status_value(model_directory);
    result["embedding_status"] = model_status["status"].clone();
    if model_status["status"] == "ready" {
        let vector_sync = with_local_embedding_model(state, model_directory, |model| {
            sync_retrieval_vectors_value(index_path, |texts| {
                model
                    .embed(texts, Some(RETRIEVAL_EMBEDDING_BATCH_SIZE))
                    .map_err(|e| format!("Failed to embed retrieval chunks: {e}"))
            })
        })?;
        result["vector_sync"] = vector_sync;
        result["search_mode"] = serde_json::Value::String(
            "fts5_lexical_with_neighbors_vector_index_ready".to_string(),
        );
    }
    result["inspection"] = inspect_retrieval_index_value(index_path);
    Ok(result)
}

#[tauri::command]
fn refresh_retrieval_index(
    workspace_path: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for retrieval indexing.".to_string());
    }
    let index_path = retrieval_index_path(&app, &root)?;
    let model_directory = embedding_model_dir(&app)?;
    Ok(refresh_retrieval_index_command_value(&root, &index_path, &model_directory, &state)?.to_string())
}

#[tauri::command]
fn inspect_retrieval_index(
    workspace_path: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for retrieval inspection.".to_string());
    }
    Ok(inspect_retrieval_index_value(&retrieval_index_path(&app, &root)?).to_string())
}

#[tauri::command]
fn rebuild_retrieval_index(
    workspace_path: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for retrieval rebuild.".to_string());
    }
    let index_path = retrieval_index_path(&app, &root)?;
    let deleted = delete_retrieval_index_value(&index_path)?;
    let mut rebuilt = refresh_retrieval_index_command_value(
        &root,
        &index_path,
        &embedding_model_dir(&app)?,
        &state,
    )?;
    rebuilt["rebuild"] = serde_json::json!({
        "deleted_database_files": deleted["deleted_files"],
        "status": "rebuilt"
    });
    Ok(rebuilt.to_string())
}

#[tauri::command]
fn query_retrieval_index(
    workspace_path: String,
    query: String,
    limit: Option<usize>,
    evidence_budget_characters: Option<usize>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for retrieval query.".to_string());
    }
    let index_path = retrieval_index_path(&app, &root)?;
    let model_directory = embedding_model_dir(&app)?;
    let query_embedding = with_local_embedding_model(&state, &model_directory, |model| {
            model
                .embed(vec![query.as_str()], Some(1))
                .map_err(|e| format!("Failed to embed retrieval query: {e}"))
        })
        .ok()
        .and_then(|mut embeddings| embeddings.pop());
    let result = if let Some(embedding) = query_embedding.as_deref() {
        query_retrieval_index_hybrid_value(
            &index_path,
            &query,
            limit.unwrap_or(8),
            Some(embedding),
            RetrievalQueryMode::Hybrid,
        )?
    } else {
        query_retrieval_index_value(&index_path, &query, limit.unwrap_or(8))?
    };
    let mut result = result;
    result["evidence_packet"] = build_retrieval_evidence_packet(&result, evidence_budget_characters);
    Ok(result.to_string())
}

#[tauri::command]
fn delete_retrieval_index(workspace_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let root = PathBuf::from(workspace_path.trim());
    if !root.exists() || !root.is_dir() {
        return Err("Workspace path is missing or invalid for retrieval index deletion.".to_string());
    }
    let index_path = retrieval_index_path(&app, &root)?;
    Ok(delete_retrieval_index_value(&index_path)?.to_string())
}

#[tauri::command]
fn get_embedding_model_status(app: tauri::AppHandle) -> Result<String, String> {
    Ok(embedding_model_status_value(&embedding_model_dir(&app)?).to_string())
}

#[tauri::command]
fn install_embedding_model(
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let result = install_embedding_model_value(&embedding_model_dir(&app)?)?;
    *state
        .embedding_model
        .lock()
        .map_err(|_| "Local embedding model cache is unavailable.".to_string())? = None;
    Ok(result.to_string())
}

#[tauri::command]
fn run_retrieval_benchmark(
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let model_directory = embedding_model_dir(&app)?;
    let benchmark_root = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("retrieval-benchmarks")
        .join("v1");
    let result = with_local_embedding_model(&state, &model_directory, |model| {
        run_retrieval_benchmark_value(&benchmark_root, |texts| {
            model
                .embed(texts, Some(RETRIEVAL_EMBEDDING_BATCH_SIZE))
                .map_err(|e| format!("Failed to embed retrieval benchmark text: {e}"))
        })
    })?;
    Ok(result.to_string())
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

    if output_ext == "md" {
        fs::write(&rendered_path, &content).map_err(|e| format!("Failed to write rendered artifact: {e}"))?;
    } else {
        let temp_markdown_path = output_path.join(format!("{}.render_tmp.md", output_name));
        fs::write(&temp_markdown_path, &content)
            .map_err(|e| format!("Failed to stage markdown for conversion: {e}"))?;

        let conversion = std::process::Command::new("pandoc")
            .arg(&temp_markdown_path)
            .arg("-o")
            .arg(&rendered_path)
            .output();

        let _ = fs::remove_file(&temp_markdown_path);

        match conversion {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let reason = if stderr.is_empty() {
                        "Pandoc conversion failed with no error output.".to_string()
                    } else {
                        stderr
                    };
                    return Err(format!(
                        "Failed to convert markdown to {}. {}",
                        output_ext, reason
                    ));
                }
            }
            Err(err) => {
                return Err(format!(
                    "Failed to run pandoc for {} export: {}. Install pandoc to enable this format.",
                    output_ext, err
                ));
            }
        }
    }

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
    files.sort();

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StructuralSource {
    id: String,
    path: String,
    kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StructuralRecord {
    id: String,
    text: String,
    source_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StructuralSymbol {
    id: String,
    notation: String,
    symbol_type: Option<String>,
    units: Option<String>,
    domain: Option<String>,
    source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StructuralRelation {
    id: String,
    relation_type: String,
    from_id: String,
    to_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct StructuralContextV1 {
    schema_version: String,
    model_id: String,
    model_name: String,
    scope: Vec<String>,
    sources: Vec<StructuralSource>,
    sections: Vec<StructuralRecord>,
    axioms: Vec<StructuralRecord>,
    assumptions: Vec<StructuralRecord>,
    definitions: Vec<StructuralRecord>,
    equations: Vec<StructuralRecord>,
    symbols: Vec<StructuralSymbol>,
    initial_conditions: Vec<StructuralRecord>,
    boundary_conditions: Vec<StructuralRecord>,
    observables: Vec<StructuralRecord>,
    tools: Vec<StructuralRecord>,
    experiments: Vec<StructuralRecord>,
    predictions: Vec<StructuralRecord>,
    falsification_criteria: Vec<StructuralRecord>,
    open_questions: Vec<StructuralRecord>,
    relations: Vec<StructuralRelation>,
}

fn stable_structural_id(kind: &str, source: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(source.as_bytes());
    hasher.update(b"\0");
    hasher.update(text.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("{}-{}", kind, &digest[..16])
}

fn split_scanned_candidate(value: &str) -> (&str, &str) {
    value.split_once(": ").unwrap_or(("unknown.md", value))
}

fn project_relative_source_path(project_root: &Path, theory_dir: &Path, relative: &str) -> String {
    let path = theory_dir.join(relative);
    match path.strip_prefix(project_root) {
        Ok(project_relative) => project_relative.to_string_lossy().replace('\\', "/"),
        Err(_) => format!("external-theory/{}", relative.replace('\\', "/").trim_start_matches('/')),
    }
}

fn extract_latex_symbols(text: &str) -> Vec<String> {
    let excluded = ["begin", "end", "frac", "left", "right", "text", "mathrm", "mathbf", "partial"];
    let chars = text.chars().collect::<Vec<_>>();
    let mut symbols = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != '\\' {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        let command_start = index;
        while index < chars.len() && chars[index].is_ascii_alphabetic() {
            index += 1;
        }
        if command_start == index {
            continue;
        }
        let command = chars[command_start..index].iter().collect::<String>();
        if command == "mathcal" && index + 2 < chars.len() && chars[index] == '{' {
            if let Some(close_offset) = chars[index + 1..].iter().position(|value| *value == '}') {
                let close = index + 1 + close_offset;
                symbols.push(chars[start..=close].iter().collect());
                index = close + 1;
                continue;
            }
        }
        if !excluded.contains(&command.as_str()) {
            symbols.push(format!("\\{command}"));
        }
    }
    symbols.sort();
    symbols.dedup();
    symbols
}

fn push_structural_record(
    target: &mut Vec<StructuralRecord>,
    kind: &str,
    path: &str,
    source_id: &str,
    text: &str,
) {
    target.push(StructuralRecord {
        id: stable_structural_id(kind, path, text),
        text: text.to_string(),
        source_id: source_id.to_string(),
    });
}

fn compile_structural_context(
    project_root: &Path,
    theory_dir: &Path,
    master_axiom_path: &Path,
    tools_dir: &Path,
    scan: &serde_json::Value,
) -> StructuralContextV1 {
    let mut source_by_path = std::collections::BTreeMap::<String, String>::new();
    let mut sections = Vec::new();
    let mut axioms = Vec::new();
    let mut assumptions = Vec::new();
    let mut definitions = Vec::new();
    let mut equations = Vec::new();
    let mut initial_conditions = Vec::new();
    let mut boundary_conditions = Vec::new();
    let mut observables = Vec::new();
    let mut tools = Vec::new();
    let mut experiments = Vec::new();
    let mut predictions = Vec::new();
    let mut falsification_criteria = Vec::new();
    let mut open_questions = Vec::new();

    let mut add_records = |field: &str, kind: &str, target: &mut Vec<StructuralRecord>| {
        if let Some(values) = scan[field].as_array() {
            for value in values.iter().filter_map(|item| item.as_str()) {
                let (relative, text) = split_scanned_candidate(value);
                let path = project_relative_source_path(project_root, theory_dir, relative);
                let source_id = source_by_path
                    .entry(path.clone())
                    .or_insert_with(|| stable_structural_id("src", &path, ""))
                    .clone();
                target.push(StructuralRecord {
                    id: stable_structural_id(kind, &path, text),
                    text: text.to_string(),
                    source_id,
                });
            }
        }
    };

    add_records("headings", "sec", &mut sections);
    add_records("hypothesis_candidates", "ax", &mut axioms);
    add_records("equation_candidates", "eq", &mut equations);

    let master_path = master_axiom_path
        .strip_prefix(project_root)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| {
            let file_name = master_axiom_path.file_name().and_then(|value| value.to_str()).unwrap_or("master_axiom.md");
            format!("external-master/{file_name}")
        });
    if let Ok(content) = fs::read_to_string(master_axiom_path) {
        let source_id = source_by_path
            .entry(master_path.clone())
            .or_insert_with(|| stable_structural_id("src", &master_path, ""))
            .clone();
        let mut section_type = "";
        for line in content.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let lower = line.to_ascii_lowercase();
            if line.starts_with('#') {
                section_type = if lower.contains("assumption") {
                    "assumption"
                } else if lower.contains("definition") {
                    "definition"
                } else if lower.contains("initial condition") {
                    "initial_condition"
                } else if lower.contains("boundary") || lower.contains("constraint") {
                    "boundary_condition"
                } else if lower.contains("observable") || lower.contains("dataset") || lower.contains("mapping") {
                    "observable"
                } else if lower.contains("experiment") || lower.contains("test") {
                    "experiment"
                } else if lower.contains("prediction") {
                    "prediction"
                } else if lower.contains("falsif") {
                    "falsification"
                } else if lower.contains("open question") || lower.contains("unresolved") || lower.contains("contradiction") {
                    "open_question"
                } else {
                    ""
                };
            }
            if lower.contains("axiom") || lower.contains("hypoth") || lower.contains("postulat") {
                push_structural_record(&mut axioms, "ax", &master_path, &source_id, line);
            }
            if line.contains("$$") || line.contains("\\begin") || line.contains("\\frac") || line.contains("\\partial") || line.contains("\\mathcal") {
                push_structural_record(&mut equations, "eq", &master_path, &source_id, line);
            }
            if !line.starts_with('#') {
                match section_type {
                    "assumption" => push_structural_record(&mut assumptions, "asm", &master_path, &source_id, line),
                    "definition" => push_structural_record(&mut definitions, "def", &master_path, &source_id, line),
                    "initial_condition" => push_structural_record(&mut initial_conditions, "ic", &master_path, &source_id, line),
                    "boundary_condition" => push_structural_record(&mut boundary_conditions, "bc", &master_path, &source_id, line),
                    "observable" => push_structural_record(&mut observables, "obs", &master_path, &source_id, line),
                    "experiment" => push_structural_record(&mut experiments, "exp", &master_path, &source_id, line),
                    "prediction" => push_structural_record(&mut predictions, "pred", &master_path, &source_id, line),
                    "falsification" => push_structural_record(&mut falsification_criteria, "fals", &master_path, &source_id, line),
                    "open_question" => push_structural_record(&mut open_questions, "open", &master_path, &source_id, line),
                    _ => {}
                }
            }
        }
    }

    if tools_dir.exists() {
        for item in collect_tool_inventory(project_root, tools_dir.to_string_lossy().as_ref()) {
            let item = item.trim_start_matches("- ");
            let (raw_path, summary) = item.split_once(": ").unwrap_or((item, "No summary available."));
            let path = if Path::new(raw_path).is_absolute() {
                let file_name = Path::new(raw_path).file_name().and_then(|value| value.to_str()).unwrap_or("tool");
                format!("external-tools/{file_name}")
            } else {
                raw_path.replace('\\', "/")
            };
            let source_id = source_by_path
                .entry(path.clone())
                .or_insert_with(|| stable_structural_id("src", &path, ""))
                .clone();
            push_structural_record(&mut tools, "tool", &path, &source_id, summary);
        }
    }

    sections.sort_by(|left, right| left.id.cmp(&right.id));
    sections.dedup_by(|left, right| left.id == right.id);
    axioms.sort_by(|left, right| left.id.cmp(&right.id));
    axioms.dedup_by(|left, right| left.id == right.id);
    equations.sort_by(|left, right| left.id.cmp(&right.id));
    equations.dedup_by(|left, right| left.id == right.id);
    for records in [
        &mut assumptions,
        &mut definitions,
        &mut initial_conditions,
        &mut boundary_conditions,
        &mut observables,
        &mut tools,
        &mut experiments,
        &mut predictions,
        &mut falsification_criteria,
        &mut open_questions,
    ] {
        records.sort_by(|left, right| left.id.cmp(&right.id));
        records.dedup_by(|left, right| left.id == right.id);
    }

    let mut symbol_sources = std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
    for equation in &equations {
        for notation in extract_latex_symbols(&equation.text) {
            symbol_sources.entry(notation).or_default().insert(equation.source_id.clone());
        }
    }
    let symbols = symbol_sources
        .into_iter()
        .map(|(notation, source_ids)| StructuralSymbol {
            id: stable_structural_id("sym", "notation", &notation),
            notation,
            symbol_type: None,
            units: None,
            domain: None,
            source_ids: source_ids.into_iter().collect(),
        })
        .collect::<Vec<_>>();

    let mut relations = Vec::new();
    for record in sections
        .iter()
        .chain(axioms.iter())
        .chain(assumptions.iter())
        .chain(definitions.iter())
        .chain(equations.iter())
        .chain(initial_conditions.iter())
        .chain(boundary_conditions.iter())
        .chain(observables.iter())
        .chain(tools.iter())
        .chain(experiments.iter())
        .chain(predictions.iter())
        .chain(falsification_criteria.iter())
        .chain(open_questions.iter())
    {
        relations.push(StructuralRelation {
            id: stable_structural_id("rel", &record.id, &record.source_id),
            relation_type: "derived_from".to_string(),
            from_id: record.id.clone(),
            to_id: record.source_id.clone(),
        });
    }
    relations.sort_by(|left, right| left.id.cmp(&right.id));

    let sources = source_by_path
        .into_iter()
        .map(|(path, id)| StructuralSource {
            id,
            kind: if path == master_path { "master_axiom" } else { "markdown" }.to_string(),
            path,
        })
        .collect::<Vec<_>>();
    let model_label = sections
        .first()
        .map(|record| record.text.as_str())
        .unwrap_or("unidentified-model");

    StructuralContextV1 {
        schema_version: "physics-ide.structural-context/v1".to_string(),
        model_id: stable_structural_id("model", "project", model_label),
        model_name: model_label.to_string(),
        scope: vec!["theory_structure".to_string()],
        sources,
        sections,
        axioms,
        assumptions,
        definitions,
        equations,
        symbols,
        initial_conditions,
        boundary_conditions,
        observables,
        tools,
        experiments,
        predictions,
        falsification_criteria,
        open_questions,
        relations,
    }
}

fn validate_structural_context(context: &StructuralContextV1) -> Result<(), String> {
    if context.model_id.trim().is_empty() || context.model_name.trim().is_empty() {
        return Err("Structural context requires model identity.".to_string());
    }
    if context.sources.iter().any(|source| Path::new(&source.path).is_absolute()) {
        return Err("Structural context source paths must be project-relative or logical external paths.".to_string());
    }

    let mut ids = std::collections::BTreeSet::new();
    for id in context
        .sources
        .iter()
        .map(|record| record.id.as_str())
        .chain(context.sections.iter().map(|record| record.id.as_str()))
        .chain(context.axioms.iter().map(|record| record.id.as_str()))
        .chain(context.assumptions.iter().map(|record| record.id.as_str()))
        .chain(context.definitions.iter().map(|record| record.id.as_str()))
        .chain(context.equations.iter().map(|record| record.id.as_str()))
        .chain(context.symbols.iter().map(|record| record.id.as_str()))
        .chain(context.initial_conditions.iter().map(|record| record.id.as_str()))
        .chain(context.boundary_conditions.iter().map(|record| record.id.as_str()))
        .chain(context.observables.iter().map(|record| record.id.as_str()))
        .chain(context.tools.iter().map(|record| record.id.as_str()))
        .chain(context.experiments.iter().map(|record| record.id.as_str()))
        .chain(context.predictions.iter().map(|record| record.id.as_str()))
        .chain(context.falsification_criteria.iter().map(|record| record.id.as_str()))
        .chain(context.open_questions.iter().map(|record| record.id.as_str()))
        .chain(context.relations.iter().map(|record| record.id.as_str()))
    {
        if id.trim().is_empty() || !ids.insert(id.to_string()) {
            return Err(format!("Structural context contains a missing or duplicate ID: {id}"));
        }
    }

    let node_ids = context
        .sources
        .iter()
        .map(|record| record.id.as_str())
        .chain(context.sections.iter().map(|record| record.id.as_str()))
        .chain(context.axioms.iter().map(|record| record.id.as_str()))
        .chain(context.assumptions.iter().map(|record| record.id.as_str()))
        .chain(context.definitions.iter().map(|record| record.id.as_str()))
        .chain(context.equations.iter().map(|record| record.id.as_str()))
        .chain(context.symbols.iter().map(|record| record.id.as_str()))
        .chain(context.initial_conditions.iter().map(|record| record.id.as_str()))
        .chain(context.boundary_conditions.iter().map(|record| record.id.as_str()))
        .chain(context.observables.iter().map(|record| record.id.as_str()))
        .chain(context.tools.iter().map(|record| record.id.as_str()))
        .chain(context.experiments.iter().map(|record| record.id.as_str()))
        .chain(context.predictions.iter().map(|record| record.id.as_str()))
        .chain(context.falsification_criteria.iter().map(|record| record.id.as_str()))
        .chain(context.open_questions.iter().map(|record| record.id.as_str()))
        .collect::<std::collections::BTreeSet<_>>();

    let source_ids = context.sources.iter().map(|source| source.id.as_str()).collect::<std::collections::BTreeSet<_>>();
    for record in context
        .sections
        .iter()
        .chain(context.axioms.iter())
        .chain(context.assumptions.iter())
        .chain(context.definitions.iter())
        .chain(context.equations.iter())
        .chain(context.initial_conditions.iter())
        .chain(context.boundary_conditions.iter())
        .chain(context.observables.iter())
        .chain(context.tools.iter())
        .chain(context.experiments.iter())
        .chain(context.predictions.iter())
        .chain(context.falsification_criteria.iter())
        .chain(context.open_questions.iter())
    {
        if !source_ids.contains(record.source_id.as_str()) {
            return Err(format!("Structural record has an invalid source: {}", record.id));
        }
    }
    for relation in &context.relations {
        if !node_ids.contains(relation.from_id.as_str()) || !node_ids.contains(relation.to_id.as_str()) {
            return Err(format!("Structural context contains a dangling relation: {}", relation.id));
        }
    }
    for symbol in &context.symbols {
        if symbol.source_ids.iter().any(|source_id| !node_ids.contains(source_id.as_str())) {
            return Err(format!("Structural symbol has an invalid source: {}", symbol.id));
        }
    }
    Ok(())
}

fn compact_context_field(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

fn render_structural_core(context: &StructuralContextV1) -> String {
    let mut lines = vec![
        format!(
            "@ctx-v1|model={}|name={}|record=id,src,text|derived_from=src",
            context.model_id,
            compact_context_field(&context.model_name)
        )
    ];
    for source in &context.sources {
        lines.push(format!("S|{}|{}|{}", source.id, source.kind, compact_context_field(&source.path)));
    }

    let record_groups: [(&str, &Vec<StructuralRecord>); 13] = [
        ("SEC", &context.sections),
        ("AX", &context.axioms),
        ("ASM", &context.assumptions),
        ("DEF", &context.definitions),
        ("EQ", &context.equations),
        ("IC", &context.initial_conditions),
        ("BC", &context.boundary_conditions),
        ("OBS", &context.observables),
        ("TOOL", &context.tools),
        ("EXP", &context.experiments),
        ("PRED", &context.predictions),
        ("FALS", &context.falsification_criteria),
        ("OPEN", &context.open_questions),
    ];
    for (code, records) in record_groups {
        for record in records {
            lines.push(format!(
                "{}|{}|{}|{}",
                code,
                record.id,
                record.source_id,
                compact_context_field(&record.text)
            ));
        }
    }
    for symbol in &context.symbols {
        lines.push(format!(
            "SYM|{}|{}|{}",
            symbol.id,
            compact_context_field(&symbol.notation),
            symbol.source_ids.join(",")
        ));
    }
    lines.join("\n")
}

fn structural_core_has_coverage(context: &StructuralContextV1, core: &str) -> bool {
    core.contains(&context.model_id)
        && context.sources.iter().all(|record| core.contains(&record.id))
        && context.sections.iter().all(|record| core.contains(&record.id))
        && context.axioms.iter().all(|record| core.contains(&record.id))
        && context.assumptions.iter().all(|record| core.contains(&record.id))
        && context.definitions.iter().all(|record| core.contains(&record.id))
        && context.equations.iter().all(|record| core.contains(&record.id))
        && context.symbols.iter().all(|record| core.contains(&record.id))
        && context.initial_conditions.iter().all(|record| core.contains(&record.id))
        && context.boundary_conditions.iter().all(|record| core.contains(&record.id))
        && context.observables.iter().all(|record| core.contains(&record.id))
        && context.tools.iter().all(|record| core.contains(&record.id))
        && context.experiments.iter().all(|record| core.contains(&record.id))
        && context.predictions.iter().all(|record| core.contains(&record.id))
        && context.falsification_criteria.iter().all(|record| core.contains(&record.id))
        && context.open_questions.iter().all(|record| core.contains(&record.id))
}

fn legacy_context_excerpt(master_axiom: &str, awareness: &str) -> String {
    fn excerpt(value: &str, max_chars: usize, label: &str) -> String {
        if value.chars().count() <= max_chars {
            return value.trim().to_string();
        }
        format!(
            "{}\n\n[...truncated {} excerpt...]",
            value.chars().take(max_chars).collect::<String>().trim(),
            label
        )
    }

    format!(
        "Master axiom excerpt:\n{}\nProject awareness excerpt:\n{}",
        excerpt(master_axiom, 1500, "master axiom"),
        excerpt(awareness, 2200, "project awareness")
    )
}

fn structural_prompt_decision(
    context: &StructuralContextV1,
    master_axiom: &str,
    awareness: &str,
) -> serde_json::Value {
    let core = render_structural_core(context);
    let legacy = legacy_context_excerpt(master_axiom, awareness);
    let core_tokens = estimate_token_count(core.chars().count());
    let legacy_tokens = estimate_token_count(legacy.chars().count());
    let coverage_complete = structural_core_has_coverage(context, &core);
    let eligible = coverage_complete && core_tokens < legacy_tokens;
    let enabled = false;
    let reduction_percent = if legacy_tokens == 0 {
        0.0
    } else {
        ((legacy_tokens as f64 - core_tokens as f64) / legacy_tokens as f64) * 100.0
    };

    serde_json::json!({
        "enabled": enabled,
        "eligible": eligible,
        "coverage_complete": coverage_complete,
        "core_estimated_tokens": core_tokens,
        "legacy_estimated_tokens": legacy_tokens,
        "estimated_reduction_percent": reduction_percent,
        "reason": if !coverage_complete {
            "semantic_id_coverage_failed"
        } else if core_tokens >= legacy_tokens {
            "compact_core_not_smaller"
        } else {
            "awaiting_retrieval_and_human_equivalence_approval"
        },
        "content": core
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

    for candidate_model in ["gemini-2.5-flash", "gemini-2.5-flash-lite", "gemini-2.5-pro"] {
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

    let mut config = load_app_config(&app).unwrap_or_default();

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

    let tree_path = project_root.join("workspace_tree.md");
    let legacy_tree_path = project_root.join("workspace_tree.txt");
    if !tree_path.exists() {
        if let Ok(tree) = build_compact_workspace_root(&project_root) {
            let _ = fs::write(&tree_path, &tree);
            let _ = fs::write(&legacy_tree_path, &tree);
        }
    }

    let primer_path = project_root.join("next_session_notes.md");
    let recap_path = project_root.join("session_recap.md");
    let awareness_path = project_root.join("project_awareness.md");
    let briefing_path = project_root.join("ai_briefing.md");
    let structural_context_path = project_root.join("structural_context.v1.json");
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
        build_first_session_briefing_markdown(&project_root, &config.ai_file_access_mode)
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
            &config.ai_file_access_mode,
        )
    };
    if !awareness_path.exists() {
        let _ = fs::write(&awareness_path, &awareness_markdown);
    }
    if !briefing_path.exists() {
        let _ = fs::write(&briefing_path, ai_briefing_markdown);
    }

    let structural_context = compile_structural_context(
        &project_root,
        Path::new(&theory_dir),
        &master_axiom_path,
        Path::new(&tools_dir),
        &scan,
    );
    validate_structural_context(&structural_context)?;
    let structural_context_json = serde_json::to_string_pretty(&structural_context)
        .map_err(|e| format!("Failed to serialize structural context: {e}"))?;
    let structural_context_file = format!("{structural_context_json}\n");
    fs::write(&structural_context_path, &structural_context_file)
        .map_err(|e| format!("Failed to write structural context: {e}"))?;
    let mut structural_hasher = Sha256::new();
    structural_hasher.update(structural_context_file.as_bytes());
    let master_axiom_content = fs::read_to_string(&master_axiom_path).unwrap_or_default();
    let structural_prompt = structural_prompt_decision(
        &structural_context,
        &master_axiom_content,
        &awareness_markdown,
    );
    let structural_context_payload = serde_json::json!({
        "name": "structural_context",
        "path": structural_context_path.to_string_lossy().to_string(),
        "exists": true,
        "schema_version": structural_context.schema_version,
        "model_id": structural_context.model_id,
        "bytes": structural_context_file.len(),
        "sha256": format!("{:x}", structural_hasher.finalize()),
        "prompt_core": structural_prompt,
        "counts": {
            "sources": structural_context.sources.len(),
            "sections": structural_context.sections.len(),
            "axioms": structural_context.axioms.len(),
            "assumptions": structural_context.assumptions.len(),
            "definitions": structural_context.definitions.len(),
            "equations": structural_context.equations.len(),
            "symbols": structural_context.symbols.len(),
            "initial_conditions": structural_context.initial_conditions.len(),
            "boundary_conditions": structural_context.boundary_conditions.len(),
            "observables": structural_context.observables.len(),
            "tools": structural_context.tools.len(),
            "experiments": structural_context.experiments.len(),
            "predictions": structural_context.predictions.len(),
            "falsification_criteria": structural_context.falsification_criteria.len(),
            "open_questions": structural_context.open_questions.len(),
            "relations": structural_context.relations.len()
        }
    });

    let (awareness_payload, awareness_diag) = read_source_payload("project_awareness", &awareness_path, 12_000);
    let (briefing_payload, briefing_diag) = read_source_payload("ai_briefing", &briefing_path, 12_000);
    if let Some(diag) = awareness_diag {
        diagnostics.push(diag);
    }
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
        "ai_file_access_mode": config.ai_file_access_mode,
        "ai_file_access_status": describe_ai_file_access_mode(&config.ai_file_access_mode),
        "primer": primer_payload,
        "session_recap": recap_payload,
        "workspace_tree": tree_payload,
        "master_axiom": axiom_payload,
        "project_awareness": awareness_payload,
        "ai_briefing": briefing_payload,
        "structural_context": structural_context_payload,
        "generated_files": {
            "session_recap": recap_path.to_string_lossy().to_string(),
            "ai_briefing": briefing_path.to_string_lossy().to_string(),
            "workspace_tree": tree_path.to_string_lossy().to_string(),
            "scratchpad": primer_path.to_string_lossy().to_string(),
            "master_axiom": master_axiom_path.to_string_lossy().to_string(),
            "project_awareness": awareness_path.to_string_lossy().to_string(),
            "structural_context": structural_context_path.to_string_lossy().to_string()
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

    let mut config = load_app_config(&app).unwrap_or_default();
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
    let tree_path = project_root.join("workspace_tree.md");
    let legacy_tree_path = project_root.join("workspace_tree.txt");
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

        // Keep the user-selected tree, or create a compact root-only fallback.
        if !tree_path.exists() {
            if let Ok(tree) = build_compact_workspace_root(&project_root) {
                let _ = fs::write(&tree_path, &tree);
                let _ = fs::write(&legacy_tree_path, &tree);
            }
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
            &config.ai_file_access_mode,
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

    let startup_guide = build_first_session_briefing_markdown(&project_root, &config.ai_file_access_mode);
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

    let config = load_app_config(&app).unwrap_or_default();

    let mut payload = result;
    payload["master_axiom_file"] = serde_json::Value::String(config.master_axiom_file.clone());
    Ok(payload.to_string())
}

#[tauri::command]
fn generate_master_axiom_from_theory(theory_dir: String, master_axiom_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let config = load_app_config(&app).unwrap_or_default();

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
fn edit_markdown_document_with_git_save(payload: LaunchFileEditorPayload, workspace_root: String, app: tauri::AppHandle) -> Result<String, String> {
    let file_path = payload.file_path.trim();
    if file_path.is_empty() {
        return Err("No markdown file path provided.".to_string());
    }

    let workspace_root = workspace_root.trim();
    if workspace_root.is_empty() {
        return Err("Workspace root is required for git-level save.".to_string());
    }

    let workspace_root_path = PathBuf::from(workspace_root);
    if !workspace_root_path.exists() || !workspace_root_path.is_dir() {
        return Err(format!("Workspace root does not exist: {}", workspace_root));
    }

    let markdown_path = PathBuf::from(file_path);
    if !markdown_path.exists() || !markdown_path.is_file() {
        return Err(format!("Markdown file does not exist: {}", file_path));
    }

    if !is_path_within_root(&markdown_path, &workspace_root_path) {
        return Err("Markdown file is outside the active workspace root.".to_string());
    }

    let git_check = std::process::Command::new("git")
        .args(["-C", workspace_root, "rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("Failed to verify git repository: {}", e))?;

    let git_repo_available = git_check.status.success();
    let github_configured = if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&data) {
                !config.github_username.trim().is_empty() && !config.github_api_key.trim().is_empty()
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let editor_cmd = if payload.editor.trim().is_empty() {
        std::env::var("EDITOR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "sensible-editor".to_string())
    } else {
        payload.editor.trim().to_string()
    };

    let terminal_app = if payload.terminal_app.trim().is_empty() {
        "x-terminal-emulator".to_string()
    } else {
        payload.terminal_app.trim().to_string()
    };

    let file_for_shell = shell_single_quote(file_path);
    let root_for_shell = shell_single_quote(workspace_root);
    let commit_msg = format!(
        "docs: update {}",
        markdown_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "markdown document".to_string())
    );
    let commit_for_shell = shell_single_quote(&commit_msg);

    let script = if git_repo_available && github_configured {
        format!(
            "{editor} {file}; git -C {root} add -- {file}; git -C {root} commit -m {message} || echo 'No commit created (no changes or commit blocked).'",
            editor = editor_cmd,
            file = file_for_shell,
            root = root_for_shell,
            message = commit_for_shell
        )
    } else {
        format!(
            "{editor} {file}; sync; echo 'Local save mode complete.'",
            editor = editor_cmd,
            file = file_for_shell
        )
    };

    let mut cmd = std::process::Command::new(&terminal_app);
    if terminal_app.contains("gnome-terminal") {
        cmd.args(["--", "bash", "-lc", &script]);
    } else {
        cmd.args(["-e", "bash", "-lc", &script]);
    }

    cmd.current_dir(&workspace_root_path)
        .spawn()
        .map_err(|e| format!("Failed to launch terminal/editor workflow: {}", e))?;

    if git_repo_available && github_configured {
        Ok(format!(
            "Opened editor for {}. Closing the editor will trigger git add + commit.",
            file_path
        ))
    } else if git_repo_available {
        Ok(format!(
            "Opened editor for {} in local-save mode (GitHub remote not configured). Closing the editor keeps changes local.",
            file_path
        ))
    } else {
        Ok(format!(
            "Opened editor for {} in local-save mode (no git repository). Closing the editor keeps changes local.",
            file_path
        ))
    }
}

#[tauri::command]
fn get_markdown_git_feedback(workspace_root: String, file_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let workspace_root = workspace_root.trim();
    let file_path = file_path.trim();

    if workspace_root.is_empty() {
        return Err("Workspace root is required.".to_string());
    }
    if file_path.is_empty() {
        return Err("Markdown file path is required.".to_string());
    }

    let workspace_root_path = PathBuf::from(workspace_root);
    let markdown_path = PathBuf::from(file_path);

    if !workspace_root_path.exists() || !workspace_root_path.is_dir() {
        return Err(format!("Workspace root does not exist: {}", workspace_root));
    }
    if !markdown_path.exists() || !markdown_path.is_file() {
        return Err(format!("Markdown file does not exist: {}", file_path));
    }
    if !is_path_within_root(&markdown_path, &workspace_root_path) {
        return Err("Markdown file is outside the active workspace root.".to_string());
    }

    let git_check = std::process::Command::new("git")
        .args(["-C", workspace_root, "rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("Failed to verify git repository: {}", e))?;

    if !git_check.status.success() {
        return Ok(serde_json::json!({
            "is_repo": false,
            "github_configured": false,
            "local_save_only": true,
            "message": "Workspace is not a git repository. Local-save mode is active."
        })
        .to_string());
    }

    let github_configured = if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&data) {
                !config.github_username.trim().is_empty() && !config.github_api_key.trim().is_empty()
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    let status_output = std::process::Command::new("git")
        .args(["-C", workspace_root, "status", "--porcelain", "--", file_path])
        .output()
        .map_err(|e| format!("Failed to query git status: {}", e))?;

    let status_text = String::from_utf8_lossy(&status_output.stdout).to_string();
    let status_lines: Vec<String> = status_text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    let mut staged = false;
    let mut unstaged = false;
    for line in &status_lines {
        let bytes = line.as_bytes();
        if bytes.len() >= 2 {
            let index_status = bytes[0] as char;
            let worktree_status = bytes[1] as char;
            if index_status != ' ' && index_status != '?' {
                staged = true;
            }
            if worktree_status != ' ' {
                unstaged = true;
            }
            if index_status == '?' && worktree_status == '?' {
                unstaged = true;
            }
        }
    }

    let log_output = std::process::Command::new("git")
        .args([
            "-C",
            workspace_root,
            "log",
            "-n",
            "1",
            "--pretty=format:%H|%s|%ct",
            "--",
            file_path,
        ])
        .output()
        .map_err(|e| format!("Failed to query git log: {}", e))?;

    let log_text = String::from_utf8_lossy(&log_output.stdout).trim().to_string();
    let last_commit = if log_text.is_empty() {
        serde_json::Value::Null
    } else {
        let parts: Vec<&str> = log_text.splitn(3, '|').collect();
        let hash = parts.first().copied().unwrap_or("").to_string();
        let subject = parts.get(1).copied().unwrap_or("").to_string();
        let epoch = parts
            .get(2)
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        serde_json::json!({
            "hash": hash,
            "short_hash": hash.chars().take(10).collect::<String>(),
            "subject": subject,
            "epoch": epoch
        })
    };

    Ok(serde_json::json!({
        "is_repo": true,
        "github_configured": github_configured,
        "local_save_only": !github_configured,
        "dirty": !status_lines.is_empty(),
        "staged": staged,
        "unstaged": unstaged,
        "status_lines": status_lines,
        "last_commit": last_commit
    })
    .to_string())
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
    let root = std::path::Path::new(&rootPath);
    if !root.exists() || !root.is_dir() {
        return Err(format!("Workspace path is invalid: {}", rootPath));
    }

    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let versions_dir = root.join("versions");

    if !versions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tags = Vec::new();
    for entry in std::fs::read_dir(&versions_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.trim().is_empty() {
                tags.push(name);
            }
        }
    }

    tags.sort();
    tags.reverse();
    Ok(tags)
}

fn normalize_version_tag(raw_tag: &str) -> Result<String, String> {
    let trimmed = raw_tag.trim();
    if trimmed.is_empty() {
        return Err("Version tag cannot be empty.".to_string());
    }
    if trimmed.len() > 120 {
        return Err("Version tag is too long (max 120 characters).".to_string());
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("Version tag may only include letters, numbers, '-', '_', and '.'.".to_string());
    }

    Ok(trimmed.to_string())
}

#[tauri::command]
fn save_as_version(tag: String, root_path: String) -> Result<String, String> {
    let normalized_tag = normalize_version_tag(&tag)?;
    let src = std::path::Path::new(&root_path);

    if !src.exists() {
        return Err(format!("Source path does not exist: {}", root_path));
    }

    if !src.is_dir() {
        return Err(format!("Source path is not a directory: {}", root_path));
    }

    let src = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let versions_dir = src.join("versions");
    std::fs::create_dir_all(&versions_dir)
        .map_err(|e| format!("Failed to create versions directory: {}", e))?;
    let dest_dir = versions_dir.join(&normalized_tag);

    if dest_dir.exists() {
        return Err(format!(
            "Version '{}' already exists. Choose a new version tag.",
            normalized_tag
        ));
    }

    fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if matches!(name.as_ref(), ".git" | "versions" | "target" | "node_modules" | "dist" | "build" | ".venv" | "venv" | "__pycache__" | ".idea" | ".vscode") {
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

    let saved_at_epoch_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let manifest = serde_json::json!({
        "version_tag": normalized_tag,
        "saved_at_epoch_seconds": saved_at_epoch_seconds,
        "workspace_root": src.to_string_lossy().to_string(),
        "mode": "local_snapshot"
    });

    let manifest_path = dest_dir.join("version_manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write version manifest: {}", e))?;

    Ok(format!(
        "Version '{}' saved locally to {}",
        manifest["version_tag"].as_str().unwrap_or("unknown"),
        dest_dir.to_string_lossy()
    ))
}

#[tauri::command]
#[allow(non_snake_case)]
fn save_equation_to_md(content: String, path: String, app: tauri::AppHandle) -> Result<String, String> {
    let target_path = std::path::PathBuf::from(&path);
    let config = load_app_config(&app)
        .map_err(|e| format!("Failed to load app config: {e}"))?;
    let workspace_root = config.project_root_dir.trim();
    let workspace_path = if workspace_root.is_empty() {
        std::path::PathBuf::from(".")
    } else {
        std::path::PathBuf::from(workspace_root)
    };

    ensure_ai_file_access(&config, "write", &target_path, &workspace_path)?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {e}"))?;
    }
    std::fs::write(&target_path, content).map_err(|e| e.to_string())?;
    Ok(format!("Equation saved successfully to {}", path))
}

#[tauri::command]
fn save_scratchpad_content(content: String, path: String, app: tauri::AppHandle) -> Result<String, String> {
    let target_path = std::path::PathBuf::from(&path);
    let config = load_app_config(&app)
        .map_err(|e| format!("Failed to load app config: {e}"))?;
    let workspace_root = config.project_root_dir.trim();
    let workspace_path = if workspace_root.is_empty() {
        std::path::PathBuf::from(".")
    } else {
        std::path::PathBuf::from(workspace_root)
    };

    ensure_ai_file_access(&config, "write", &target_path, &workspace_path)?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent directory: {e}"))?;
    }
    std::fs::write(&target_path, content).map_err(|e| e.to_string())?;
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
    let mut config = load_app_config(&app).unwrap_or_default();

    config.editor = payload.editor;
    config.terminal_app = payload.terminal_app;
    config.gemini_api_key = normalize_api_key(&payload.gemini_key);
    config.openai_api_key = normalize_api_key(&payload.openai_key);
    config.github_username = payload.github_username;
    config.github_api_key = normalize_api_key(&payload.github_api_key);
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
    config.ai_file_access_mode = payload.ai_file_access_mode;

    if !config.project_root_dir.trim().is_empty() {
        config.last_root_dir = config.project_root_dir.clone();
    }

    if config.theory_md_dir.trim().is_empty() && !config.project_root_dir.trim().is_empty() {
        config.theory_md_dir = config.project_root_dir.clone();
    }

    save_app_config(&app, &config)?;

    Ok("Settings saved successfully".to_string())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_workspace_scoped_ai_file_operations_when_enabled() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_ai_file_access_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let target_path = workspace_root.join("notes.md");

        let mut config = AppConfig::default();
        config.ai_file_access_mode = "read_write".to_string();

        let result = ensure_ai_file_access(&config, "write", &target_path, &workspace_root);
        assert!(result.is_ok());
    }

    #[test]
    fn blocks_ai_file_operations_when_disabled_or_outside_workspace() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_ai_file_access_block_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let workspace_root = temp_dir.join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let outside_target = temp_dir.join("outside.md");

        let config = AppConfig::default();
        let blocked_by_mode = ensure_ai_file_access(&config, "write", &workspace_root.join("notes.md"), &workspace_root);
        let blocked_by_scope = ensure_ai_file_access(&AppConfig { ai_file_access_mode: "read_write".to_string(), ..AppConfig::default() }, "write", &outside_target, &workspace_root);

        assert!(blocked_by_mode.is_err());
        assert!(blocked_by_scope.is_err());
    }

    #[test]
    fn briefing_reflects_current_ai_file_access_mode() {
        let project_root = std::env::temp_dir().join("physics_ide_ai_briefing_access_test");
        let _ = fs::remove_dir_all(&project_root);
        let _ = fs::create_dir_all(&project_root);
        let master_axiom = project_root.join("master_axiom.md");
        let primer = project_root.join("next_session_notes.md");
        let recap = project_root.join("session_recap.md");
        let tree = project_root.join("workspace_tree.md");
        let awareness = "## Awareness\n- demo";
        let packet = build_ai_briefing_markdown(
            &project_root,
            "Summary text",
            &primer,
            &recap,
            &tree,
            &master_axiom,
            awareness,
            None,
            "read_write",
        );

        assert!(packet.contains("AI file access status"), "briefing should describe current file access status");
        assert!(packet.contains("read/write access"), "briefing should describe the enabled read/write mode");
        assert!(!packet.contains("advisory-only"), "briefing should not describe AI as advisory-only when access is enabled");
    }

    #[test]
    fn normalize_api_key_trims_surrounding_whitespace_and_quotes() {
        assert_eq!(normalize_api_key("  \"sk-test-key\"  "), "sk-test-key");
        assert_eq!(normalize_api_key("\n'AIza-test-key'\t"), "AIza-test-key");
    }

    #[test]
    fn encrypts_and_decrypts_secrets_with_the_local_master_secret() {
        let master_secret = "test-master-secret";
        let secret = "sk-test-key";
        let encrypted = encrypt_secret_with_key(secret, master_secret).unwrap();
        let decrypted = decrypt_secret_with_key(&encrypted, master_secret).unwrap();
        assert_eq!(decrypted, secret);
        assert!(encrypted.starts_with(ENCRYPTED_SECRET_PREFIX));
    }

    #[test]
    fn percent_encodes_gemini_api_keys_for_request_urls() {
        assert_eq!(percent_encode_component("AIzaSyA/Plus+123=abc"), "AIzaSyA%2FPlus%2B123%3Dabc");
    }

    #[test]
    fn updates_workspace_root_fields_when_workspace_is_saved() {
        let mut config = AppConfig::default();
        update_workspace_root_in_config(&mut config, " /tmp/workspace ");
        assert_eq!(config.last_root_dir, "/tmp/workspace");
        assert_eq!(config.project_root_dir, "/tmp/workspace");
    }

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
    fn normalizes_gemini_models_to_current_supported_models() {
        assert_eq!(normalize_model_for_provider("gemini", "gemini-2.5-flash"), "gemini-2.5-flash");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-2.5-flash-lite"), "gemini-2.5-flash-lite");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-2.0-flash"), "gemini-2.5-flash");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-3-flash"), "gemini-2.5-pro");
        assert_eq!(normalize_model_for_provider("gemini", "gemini-1.5-pro"), "gemini-2.5-pro");
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
    fn compiles_deterministic_structural_context_with_source_coverage() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_structural_context_test_{}", std::process::id()));
        let theory_dir = temp_dir.join("theory");
        let tools_dir = temp_dir.join("tools");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&theory_dir).unwrap();
        fs::create_dir_all(&tools_dir).unwrap();
        fs::write(
            theory_dir.join("foundations.md"),
            "# Foundations\n\n## Core Axiom\nThe axiom defines a scalar field.\n\n$$\\mathcal{L} = \\frac{1}{2}\\partial_\\mu \\phi \\partial^\\mu \\phi$$\n",
        )
        .unwrap();
        let master_axiom_path = temp_dir.join("master_axiom.md");
        fs::write(
            &master_axiom_path,
            "# Master Axiom\n\n## Hypothesis\nThe hypothesis constrains the field.\n\n## Structural Assumptions\nSpacetime is differentiable.\n\n## Boundary Conditions\nThe field vanishes at infinity.\n\n## Predictions\nThe model predicts a bounded signal.\n\n## Falsification Criteria\nReject the model when the signal diverges.\n\n## Governing Equation\n$$\\mathcal{L}=V(\\phi)$$\n",
        )
        .unwrap();
        fs::write(tools_dir.join("analyze.py"), "# analysis tool\nprint('analyze')\n").unwrap();

        let scan = scan_markdown_theory(theory_dir.to_str().unwrap());
        let first = compile_structural_context(&temp_dir, &theory_dir, &master_axiom_path, &tools_dir, &scan);
        let second = compile_structural_context(&temp_dir, &theory_dir, &master_axiom_path, &tools_dir, &scan);
        let first_json = serde_json::to_string_pretty(&first).unwrap();
        let second_json = serde_json::to_string_pretty(&second).unwrap();
        let awareness = build_project_awareness_markdown(
            &temp_dir,
            theory_dir.to_str().unwrap(),
            &master_axiom_path,
            tools_dir.to_str().unwrap(),
            &scan,
        );
        let master_axiom = fs::read_to_string(&master_axiom_path).unwrap();
        let first_core = render_structural_core(&first);
        let second_core = render_structural_core(&second);
        let prompt_decision = structural_prompt_decision(&first, &master_axiom, &awareness);

        assert_eq!(first_json, second_json);
        assert_eq!(first_core, second_core);
        assert!(structural_core_has_coverage(&first, &first_core));
        assert!(!structural_core_has_coverage(&first, "@ctx-v1|incomplete"));
        assert!(prompt_decision["coverage_complete"].as_bool().unwrap());
        assert!(
            prompt_decision["eligible"].as_bool().unwrap(),
            "Structural core was not smaller: core={} legacy={}",
            prompt_decision["core_estimated_tokens"],
            prompt_decision["legacy_estimated_tokens"]
        );
        assert!(!prompt_decision["enabled"].as_bool().unwrap());
        assert_eq!(first.schema_version, "physics-ide.structural-context/v1");
        assert!(first.sources.iter().all(|source| !source.path.contains(temp_dir.to_string_lossy().as_ref())));
        assert!(first.symbols.iter().any(|symbol| symbol.notation == "\\mathcal{L}"));
        assert!(first.symbols.iter().any(|symbol| symbol.notation == "\\phi"));
        assert!(first.axioms.len() >= scan["hypothesis_candidates"].as_array().unwrap().len());
        assert!(first.equations.len() >= scan["equation_candidates"].as_array().unwrap().len());
        assert_eq!(first.assumptions.len(), 1);
        assert_eq!(first.boundary_conditions.len(), 1);
        assert_eq!(first.predictions.len(), 1);
        assert_eq!(first.falsification_criteria.len(), 1);
        assert_eq!(first.tools.len(), 1);
        let record_count = first.sections.len()
            + first.axioms.len()
            + first.assumptions.len()
            + first.definitions.len()
            + first.equations.len()
            + first.initial_conditions.len()
            + first.boundary_conditions.len()
            + first.observables.len()
            + first.tools.len()
            + first.experiments.len()
            + first.predictions.len()
            + first.falsification_criteria.len()
            + first.open_questions.len();
        assert_eq!(first.relations.len(), record_count);
        validate_structural_context(&first).unwrap();
    }

    #[test]
    fn structural_context_validation_rejects_dangling_relations() {
        let mut context = StructuralContextV1 {
            schema_version: "physics-ide.structural-context/v1".to_string(),
            model_id: "model-test".to_string(),
            model_name: "Test Model".to_string(),
            scope: vec!["test".to_string()],
            sources: vec![StructuralSource {
                id: "src-test".to_string(),
                path: "theory.md".to_string(),
                kind: "markdown".to_string(),
            }],
            sections: Vec::new(),
            axioms: Vec::new(),
            assumptions: Vec::new(),
            definitions: Vec::new(),
            equations: Vec::new(),
            symbols: Vec::new(),
            initial_conditions: Vec::new(),
            boundary_conditions: Vec::new(),
            observables: Vec::new(),
            tools: Vec::new(),
            experiments: Vec::new(),
            predictions: Vec::new(),
            falsification_criteria: Vec::new(),
            open_questions: Vec::new(),
            relations: vec![StructuralRelation {
                id: "rel-test".to_string(),
                relation_type: "derived_from".to_string(),
                from_id: "missing-node".to_string(),
                to_id: "src-test".to_string(),
            }],
        };

        let error = validate_structural_context(&context).unwrap_err();
        assert!(error.contains("dangling relation"));

        context.relations.clear();
        validate_structural_context(&context).unwrap();
    }

    #[test]
    fn retrieval_index_refreshes_incrementally_and_returns_explanatory_neighbors() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_retrieval_test_{}", std::process::id()));
        let index_path = temp_dir.join(".test-retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let theory_path = temp_dir.join("bmi_neutrino.md");
        fs::write(
            &theory_path,
            "# BMI Foundations\nThe bimodal interaction defines the model.\n\n## Neutrino mechanism\nNeutrino mass is determined by the interaction eigenvalue and boundary coupling.\n\n## Consequence\nThe effective mass changes when the coupling changes.\n",
        )
        .unwrap();
        fs::write(
            temp_dir.join("ai_briefing.md"),
            "# Generated Briefing\nThis synthetic duplicate should never enter retrieval.",
        )
        .unwrap();

        let first = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(first["indexed_files"].as_u64().unwrap(), 1);
        assert_eq!(first["unchanged_files"].as_u64().unwrap(), 0);

        let second = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(second["indexed_files"].as_u64().unwrap(), 0);
        assert_eq!(second["unchanged_files"].as_u64().unwrap(), 1);

        let query = query_retrieval_index_value(&index_path, "how is neutrino mass determined", 4).unwrap();
        let results = query["results"].as_array().unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["relative_path"], "bmi_neutrino.md");
        assert!(results[0]["content"].as_str().unwrap().contains("interaction eigenvalue"));
        assert!(!results[0]["neighbors"].as_array().unwrap().is_empty());
        let generated_query = query_retrieval_index_value(&index_path, "synthetic duplicate", 4).unwrap();
        assert!(generated_query["results"].as_array().unwrap().is_empty());

        fs::write(
            &theory_path,
            "# BMI Foundations\nThe bimodal interaction defines the model.\n\n## Neutrino mechanism\nNeutrino mass is determined by a revised interaction eigenvalue and boundary coupling.\n",
        )
        .unwrap();
        let changed = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(changed["indexed_files"].as_u64().unwrap(), 1);

        fs::remove_file(&theory_path).unwrap();
        let deleted = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(deleted["deleted_files"].as_u64().unwrap(), 1);
        let empty = query_retrieval_index_value(&index_path, "neutrino mass", 4).unwrap();
        assert!(empty["results"].as_array().unwrap().is_empty());
    }

    #[test]
    fn retrieval_chunks_bound_large_unicode_sections() {
        let content = format!("# Large Section\n{}", "λ".repeat(9_001));
        let chunks = chunk_markdown("large.md", &content);

        assert!(chunks.len() >= 4);
        assert!(chunks.iter().all(|chunk| chunk.content.chars().count() <= 4_000));
        assert!(chunks.iter().all(|chunk| chunk.line_start >= 1 && chunk.line_end >= chunk.line_start));
    }

    #[test]
    fn retrieval_evidence_packet_enforces_budget_and_reports_exclusions() {
        let retrieval = serde_json::json!({
            "results": [
                {
                    "relative_path": "theory/primary.md",
                    "chunk_id": "chunk-primary",
                    "fusion_score": 0.03,
                    "lexical_rank": 1,
                    "vector_rank": 2,
                    "line_start": 1,
                    "line_end": 3,
                    "content": "λ".repeat(700),
                    "neighbors": [],
                    "graph_neighbors": []
                },
                {
                    "relative_path": "theory/excluded.md",
                    "chunk_id": "chunk-excluded",
                    "fusion_score": 0.02,
                    "lexical_rank": 2,
                    "vector_rank": 1,
                    "line_start": 4,
                    "line_end": 6,
                    "content": "excluded evidence",
                    "neighbors": [],
                    "graph_neighbors": []
                }
            ]
        });

        let packet = build_retrieval_evidence_packet(&retrieval, Some(100));
        let diagnostics = &packet["diagnostics"];
        assert_eq!(diagnostics["budget_characters"].as_u64().unwrap(), 500);
        assert_eq!(diagnostics["used_characters"].as_u64().unwrap(), 500);
        assert_eq!(diagnostics["used_estimated_tokens"].as_u64().unwrap(), 125);
        assert_eq!(diagnostics["included_rows"].as_u64().unwrap(), 1);
        assert_eq!(diagnostics["excluded_rows"].as_u64().unwrap(), 1);
        assert_eq!(diagnostics["truncated_snippets"].as_u64().unwrap(), 1);
        assert_eq!(diagnostics["provider_text_truncated"], true);
        assert_eq!(diagnostics["excluded_sources"][0], "theory/excluded.md");
        assert_eq!(packet["provider_text"].as_str().unwrap().chars().count(), 500);
        assert_eq!(
            packet["evidence"][0]["snippets"][0]["content"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            500
        );
    }

    #[test]
    fn inspects_and_recovers_corrupt_retrieval_indexes_locally() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_corrupt_retrieval_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let absent = inspect_retrieval_index_value(&index_path);
        assert_eq!(absent["status"], "not_built");

        fs::write(&index_path, "not a sqlite database").unwrap();
        let corrupt = inspect_retrieval_index_value(&index_path);
        assert_eq!(corrupt["status"], "corrupt");

        let connection = open_retrieval_index(&index_path).unwrap();
        drop(connection);
        let recovered = inspect_retrieval_index_value(&index_path);
        assert_eq!(recovered["status"], "ready");
        assert_eq!(recovered["integrity"], "ok");
        assert_eq!(recovered["quarantined_indexes"].as_u64().unwrap(), 1);
        let quarantined = temp_dir.join("corrupt-index").join("retrieval.sqlite3");
        assert_eq!(fs::read_to_string(quarantined).unwrap(), "not a sqlite database");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn quarantines_incompatible_future_retrieval_schema_without_mutating_it() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_future_retrieval_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let future = rusqlite::Connection::open(&index_path).unwrap();
        future.execute_batch("PRAGMA user_version=2; CREATE TABLE future_records(id INTEGER);").unwrap();
        drop(future);

        let incompatible = inspect_retrieval_index_value(&index_path);
        assert_eq!(incompatible["status"], "incompatible");
        let recovered = open_retrieval_index(&index_path).unwrap();
        let active_version: i64 = recovered.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        assert_eq!(active_version, 1);
        drop(recovered);

        let quarantined_path = temp_dir.join("corrupt-index").join("retrieval.sqlite3");
        let quarantined = rusqlite::Connection::open_with_flags(
            quarantined_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let quarantined_version: i64 = quarantined.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        assert_eq!(quarantined_version, 2);
        drop(quarantined);
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn retrieval_refresh_invalidates_renamed_sources_only() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_retrieval_rename_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let old_path = temp_dir.join("old-name.md");
        let new_path = temp_dir.join("new-name.md");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(&old_path, "# Renamed source\nA stable mechanism remains unchanged.\n").unwrap();

        let first = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(first["indexed_files"].as_u64().unwrap(), 1);
        fs::rename(&old_path, &new_path).unwrap();
        let renamed = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(renamed["indexed_files"].as_u64().unwrap(), 1);
        assert_eq!(renamed["deleted_files"].as_u64().unwrap(), 1);

        let connection = open_retrieval_index(&index_path).unwrap();
        let paths = {
            let mut statement = connection
                .prepare("SELECT path FROM retrieval_files ORDER BY path")
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .filter_map(Result::ok)
                .collect::<Vec<_>>()
        };
        assert_eq!(paths, vec!["new-name.md"]);
        drop(connection);
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn loads_selected_embedding_metadata_and_sqlite_vector_search() {
        let model = fastembed::TextEmbedding::get_model_info(
            &fastembed::EmbeddingModel::AllMiniLML6V2,
        )
        .unwrap();
        assert_eq!(model.model_code, RETRIEVAL_EMBEDDING_MODEL_ID);
        assert_eq!(model.dim, RETRIEVAL_EMBEDDING_DIMENSIONS);

        let temp_dir = std::env::temp_dir().join(format!("physics_ide_vector_test_{}", std::process::id()));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        let connection = open_retrieval_index(&index_path).unwrap();
        let version: String = connection.query_row("SELECT vec_version()", [], |row| row.get(0)).unwrap();
        assert!(!version.is_empty());

        let encode = |values: &[f32]| {
            values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>()
        };
        let mut first = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
        let mut second = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
        first[0] = 1.0;
        second[1] = 1.0;
        connection.execute(
            "INSERT INTO retrieval_vectors(rowid, embedding) VALUES (?1, ?2)",
            rusqlite::params![1, encode(&first)],
        ).unwrap();
        connection.execute(
            "INSERT INTO retrieval_vectors(rowid, embedding) VALUES (?1, ?2)",
            rusqlite::params![2, encode(&second)],
        ).unwrap();

        let nearest: i64 = connection.query_row(
            "SELECT rowid FROM retrieval_vectors WHERE embedding MATCH ?1 AND k = 1 ORDER BY distance",
            [encode(&first)],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(nearest, 1);
        drop(connection);
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn reports_missing_local_embedding_assets_without_network_access() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_missing_model_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let status = embedding_model_status_value(&temp_dir);
        assert_eq!(status["status"], "model_assets_required");
        assert_eq!(status["asset_count"].as_u64().unwrap(), 5);
        assert_eq!(status["errors"].as_array().unwrap().len(), 5);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn synchronizes_only_missing_and_stale_retrieval_vectors() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_vector_sync_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let theory_path = temp_dir.join("theory.md");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(
            &theory_path,
            "# Foundation\nA manifold defines the geometry.\n\n## Mechanism\nA coupling determines the effective mass.\n",
        )
        .unwrap();
        refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();

        let deterministic_embed = |texts: &[String]| {
            Ok(texts
                .iter()
                .map(|text| {
                    let mut embedding = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
                    embedding[0] = 1.0;
                    embedding[1] = text.len() as f32;
                    embedding
                })
                .collect::<Vec<_>>())
        };
        let first = sync_retrieval_vectors_value(&index_path, deterministic_embed).unwrap();
        assert_eq!(first["embedded_chunks"].as_u64().unwrap(), 2);
        assert_eq!(first["removed_vectors"].as_u64().unwrap(), 0);

        let second = sync_retrieval_vectors_value(&index_path, deterministic_embed).unwrap();
        assert_eq!(second["embedded_chunks"].as_u64().unwrap(), 0);
        assert_eq!(second["removed_vectors"].as_u64().unwrap(), 0);

        fs::write(
            &theory_path,
            "# Foundation\nA manifold defines the geometry.\n\n## Mechanism\nA revised coupling determines the effective mass.\n",
        )
        .unwrap();
        refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        let failed = sync_retrieval_vectors_value(&index_path, |_texts| {
            Err("synthetic embedding failure".to_string())
        });
        assert!(failed.is_err());
        let connection = open_retrieval_index(&index_path).unwrap();
        let retained_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM retrieval_vector_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(retained_count, 2);
        drop(connection);

        let changed = sync_retrieval_vectors_value(&index_path, deterministic_embed).unwrap();
        assert_eq!(changed["embedded_chunks"].as_u64().unwrap(), 1);
        assert_eq!(changed["removed_vectors"].as_u64().unwrap(), 1);

        fs::remove_file(&theory_path).unwrap();
        refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        let deleted = sync_retrieval_vectors_value(&index_path, deterministic_embed).unwrap();
        assert_eq!(deleted["embedded_chunks"].as_u64().unwrap(), 0);
        assert_eq!(deleted["removed_vectors"].as_u64().unwrap(), 2);

        let connection = open_retrieval_index(&index_path).unwrap();
        let vector_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM retrieval_vector_records", [], |row| row.get(0))
            .unwrap();
        assert_eq!(vector_count, 0);
        drop(connection);
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn fuses_lexical_and_vector_candidates_without_theory_specific_rules() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_hybrid_retrieval_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(
            temp_dir.join("theory.md"),
            "# Dual evidence\nNeutrino mass follows the internal coupling.\n\n## Lexical evidence\nNeutrino observations constrain the detector.\n\n## Semantic bridge\nThe hidden boundary response follows the internal coupling.\n",
        )
        .unwrap();
        refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        sync_retrieval_vectors_value(&index_path, |texts| {
            Ok(texts
                .iter()
                .map(|text| {
                    let mut embedding = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
                    if text.contains("Neutrino mass") || text.contains("hidden boundary") {
                        embedding[0] = 1.0;
                    } else {
                        embedding[1] = 1.0;
                    }
                    embedding
                })
                .collect::<Vec<_>>())
        })
        .unwrap();

        let mut query_embedding = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
        query_embedding[0] = 1.0;
        let result = query_retrieval_index_hybrid_value(
            &index_path,
            "neutrino mass",
            3,
            Some(&query_embedding),
            RetrievalQueryMode::Hybrid,
        )
        .unwrap();
        assert_eq!(result["search_mode"], "hybrid_rrf_fts5_vector_with_neighbors");
        assert_eq!(result["vector_status"], "used");
        let rows = result["results"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0]["content"].as_str().unwrap().contains("Neutrino mass"));
        assert!(rows[0]["lexical_rank"].is_number());
        assert!(rows[0]["vector_rank"].is_number());
        assert!(rows.iter().any(|row| {
            row["content"].as_str().unwrap().contains("hidden boundary")
                && row["lexical_rank"].is_null()
                && row["vector_rank"].is_number()
        }));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn expands_only_source_explicit_typed_graph_relations() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_graph_retrieval_test_{}",
            std::process::id()
        ));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let theory_path = temp_dir.join("theory.md");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(
            &theory_path,
            "# Boundary condition\nThe field vanishes at the outer surface.\n\n## Dynamics\nThe evolution depends on Boundary condition.\n",
        )
        .unwrap();

        let refresh = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(refresh["graph_edges"].as_u64().unwrap(), 1);
        let query = query_retrieval_index_value(&index_path, "evolution dynamics", 2).unwrap();
        let dynamics = query["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["heading"] == "Dynamics")
            .unwrap();
        let graph_neighbors = dynamics["graph_neighbors"].as_array().unwrap();
        assert_eq!(graph_neighbors.len(), 1);
        assert_eq!(graph_neighbors[0]["relation_type"], "depends_on");
        assert_eq!(graph_neighbors[0]["direction"], "outgoing");
        assert_eq!(graph_neighbors[0]["heading"], "Boundary condition");

        fs::write(
            &theory_path,
            "# Boundary condition\nThe field vanishes at the outer surface.\n\n## Dynamics\nThe evolution is evaluated independently.\n",
        )
        .unwrap();
        let changed = refresh_retrieval_index_value(&temp_dir, &index_path).unwrap();
        assert_eq!(changed["graph_edges"].as_u64().unwrap(), 0);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn cross_theory_benchmark_reports_recall_superiority_and_mrr_gate_independently() {
        let temp_dir = std::env::temp_dir().join(format!(
            "physics_ide_retrieval_benchmark_test_{}",
            std::process::id()
        ));
        let result = run_retrieval_benchmark_value(&temp_dir, |texts| {
            Ok(texts
                .iter()
                .map(|text| {
                    let mut embedding = vec![0.0f32; RETRIEVAL_EMBEDDING_DIMENSIONS];
                    let query_dimension = match text.as_str() {
                        "Where does Omega_m enter the expansion equation?" => Some(0),
                        "What establishes particle inertia?" => Some(1),
                        "Which observation contradicts stationary Earth?" => Some(2),
                        "What governs bosonic field feedback?" => Some(3),
                        "What does luminosity distance depend on?" => Some(4),
                        "Which experiment uses protocol EX_27 to test boundary mode locking?" => Some(5),
                        "Where is chi_perp defined?" => Some(6),
                        "Which instrument tests the fixed-frame prediction?" => Some(7),
                        _ => None,
                    };
                    if let Some(dimension) = query_dimension {
                        embedding[dimension] = 1.0;
                    } else if text.contains("effective mass emerges") {
                        embedding[1] = 1.0;
                    } else if text.contains("quartic lambda") {
                        embedding[3] = 1.0;
                    } else if text.contains("Omega_m") {
                        embedding[0] = 0.7;
                        embedding[100] = 0.714;
                    } else if text.contains("stellar parallax contradicts") {
                        embedding[2] = 0.7;
                        embedding[102] = 0.714;
                    } else if text.contains("Luminosity distance depends") {
                        embedding[4] = 0.7;
                        embedding[104] = 0.714;
                    } else if text.contains("Protocol EX_27") {
                        embedding[5] = 0.7;
                        embedding[105] = 0.714;
                    } else if text.contains("uncommon symbol chi_perp") {
                        embedding[6] = 0.7;
                        embedding[106] = 0.714;
                    } else if text.contains("ring-laser apparatus") {
                        embedding[7] = 1.0;
                    } else if text.contains("matter fraction")
                        || text.contains("density contribution")
                        || text.contains("cosmological matter term")
                    {
                        embedding[0] = 0.8;
                        embedding[200] = 0.6;
                    } else if text.contains("fixed terrestrial")
                        || text.contains("unmoving observer")
                        || text.contains("terrestrial platform")
                    {
                        embedding[2] = 0.8;
                        embedding[202] = 0.6;
                    } else if text.contains("brightness-based distance")
                        || text.contains("inferred source distance")
                        || text.contains("observational distance mapping")
                    {
                        embedding[4] = 0.8;
                        embedding[204] = 0.6;
                    } else if text.contains("experimental protocol")
                        || text.contains("laboratory procedure")
                        || text.contains("unrelated trial")
                    {
                        embedding[5] = 0.8;
                        embedding[205] = 0.6;
                    } else if text.contains("projected transverse")
                        || text.contains("orthogonal invariant")
                        || text.contains("field projection")
                    {
                        embedding[6] = 0.8;
                        embedding[206] = 0.6;
                    } else {
                        embedding[20] = 1.0;
                    }
                    embedding
                })
                .collect::<Vec<_>>())
        })
        .unwrap();

        assert_eq!(result["status"], "fail", "{result}");
        assert_eq!(result["family_count"].as_u64().unwrap(), 4);
        assert_eq!(result["case_count"].as_u64().unwrap(), 8);
        assert_eq!(result["hits_at_3"]["lexical"].as_u64().unwrap(), 7);
        assert_eq!(result["hits_at_3"]["vector"].as_u64().unwrap(), 1);
        assert_eq!(result["hits_at_3"]["hybrid"].as_u64().unwrap(), 8, "{result}");
        assert_eq!(result["graph"]["complete"], true);
        assert_eq!(result["strict_superiority_status"], "pass");
        assert_eq!(result["acceptance"]["passed"], false);
    }

    #[test]
    #[ignore = "requires PHYSICS_IDE_EMBEDDING_MODEL_DIR with the pinned local model assets"]
    fn embeds_with_verified_local_model_assets() {
        let directory = std::env::var("PHYSICS_IDE_EMBEDDING_MODEL_DIR").unwrap();
        let status = install_embedding_model_value(Path::new(&directory)).unwrap();
        assert_eq!(status["status"], "ready");
        assert_eq!(status["inference_status"], "ready");

        let index_path = Path::new(&directory).join("integration-retrieval.sqlite3");
        let _ = delete_retrieval_index_value(&index_path);
        let connection = open_retrieval_index(&index_path).unwrap();
        connection
            .execute(
                "INSERT INTO retrieval_chunks (chunk_id, path, chunk_index, line_start, line_end, heading, content)
                 VALUES (?1, ?2, 0, 1, 2, ?3, ?4)",
                rusqlite::params![
                    "chunk-integration-probe",
                    "theory.md",
                    "Boundary mechanism",
                    "A manifold coupling determines the boundary response."
                ],
            )
            .unwrap();
        drop(connection);
        let mut model = load_local_embedding_model(Path::new(&directory)).unwrap();
        let sync = sync_retrieval_vectors_value(&index_path, |texts| {
            model.embed(texts, Some(RETRIEVAL_EMBEDDING_BATCH_SIZE)).map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(sync["embedded_chunks"].as_u64().unwrap(), 1);
        let connection = open_retrieval_index(&index_path).unwrap();
        let vector_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM retrieval_vectors", [], |row| row.get(0))
            .unwrap();
        assert_eq!(vector_count, 1);
        drop(connection);
        let query_embedding = model
            .embed(vec!["What determines the boundary response?"], Some(1))
            .unwrap()
            .pop()
            .unwrap();
        let query = query_retrieval_index_hybrid_value(
            &index_path,
            "What determines the boundary response?",
            4,
            Some(&query_embedding),
            RetrievalQueryMode::Hybrid,
        )
        .unwrap();
        assert_eq!(query["search_mode"], "hybrid_rrf_fts5_vector_with_neighbors");
        assert_eq!(query["vector_status"], "used");
        assert_eq!(query["results"].as_array().unwrap().len(), 1);
        delete_retrieval_index_value(&index_path).unwrap();

        let benchmark_root = Path::new(&directory).join("integration-retrieval-benchmark");
        let benchmark = run_retrieval_benchmark_value(&benchmark_root, |texts| {
            model
                .embed(texts, Some(RETRIEVAL_EMBEDDING_BATCH_SIZE))
                .map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(benchmark["case_count"].as_u64().unwrap(), 8);
        assert_eq!(benchmark["family_count"].as_u64().unwrap(), 4);
        assert_eq!(benchmark["hits_at_3"]["hybrid"].as_u64().unwrap(), 8, "{benchmark}");
        assert_eq!(benchmark["hybrid_non_inferior"], true, "{benchmark}");
        assert_eq!(benchmark["acceptance"]["passed"], true, "{benchmark}");
    }

    #[test]
    fn deletes_local_retrieval_index_and_sqlite_sidecars() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_retrieval_delete_test_{}", std::process::id()));
        let index_path = temp_dir.join("retrieval.sqlite3");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(&index_path, "index").unwrap();
        fs::write(format!("{}-wal", index_path.to_string_lossy()), "wal").unwrap();
        fs::write(format!("{}-shm", index_path.to_string_lossy()), "shm").unwrap();

        let deleted = delete_retrieval_index_value(&index_path).unwrap();
        assert_eq!(deleted["status"], "deleted");
        assert_eq!(deleted["deleted_files"].as_u64().unwrap(), 3);
        assert!(!index_path.exists());
        assert!(!temp_dir.exists());

        let absent = delete_retrieval_index_value(&index_path).unwrap();
        assert_eq!(absent["status"], "not_found");
        assert_eq!(absent["deleted_files"].as_u64().unwrap(), 0);
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
        let snapshot_dir = source_dir.join("versions").join("v1.0.0");

        assert!(snapshot_dir.join("notes.md").exists());
        assert!(!snapshot_dir.join(".git").exists());
        assert!(!snapshot_dir.join("build").exists());
        assert!(snapshot_dir.join("version_manifest.json").exists());
        assert!(result.contains("saved locally"));
    }

    #[test]
    fn help_search_matches_common_workspace_typo() {
        let response = search_help_docs("wokrspaces".to_string()).unwrap();
        let payload: serde_json::Value = serde_json::from_str(&response).unwrap();
        let results = payload["results"].as_array().unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0]["id"].as_str().unwrap(), "app-layers");
    }

    #[test]
    fn vector_model_installer_frontend_has_observable_two_step_wiring() {
        let html = include_str!("../../src/index.html");
        for required in [
            "id=\"embedding-model-install\"",
            "onclick=\"installLocalEmbeddingModel()\"",
            "let embeddingInstallConfirmationPending = false;",
            "Confirm 91 MB Download",
            "Installing and verifying local vector model...",
            "invoke('install_embedding_model')",
            "Install Vector Model clicked.",
        ] {
            assert!(html.contains(required), "Missing installer UI contract: {required}");
        }
    }

    #[test]
    fn extract_evidence_snippet_handles_multibyte_characters() {
        let content = "A short intro with ✅ emoji before the analysis topic and some more words to pad the excerpt.";
        let snippet = extract_evidence_snippet(content, &["analysis".to_string()], 120);
        assert!(snippet.contains("analysis"));
        assert!(!snippet.is_empty());
    }

    #[test]
    fn builds_ai_friendly_workspace_tree_markdown() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_tree_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("docs").join("guides")).unwrap();
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::write(temp_dir.join("README.md"), "# Demo\n").unwrap();
        fs::write(temp_dir.join("src").join("main.js"), "console.log('hi');\n").unwrap();
        fs::write(temp_dir.join("docs").join("guides").join("setup.md"), "# Setup\n").unwrap();

        let tree = build_workspace_tree_string(&temp_dir).unwrap();

        assert!(tree.contains("# Workspace Tree"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("main.js"));
        assert!(tree.contains("docs/"));
        assert!(tree.contains("guides/"));
        assert!(tree.contains("setup.md"));
    }

    #[test]
    fn compact_workspace_root_does_not_walk_project_files() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_compact_tree_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("nested")).unwrap();
        fs::write(temp_dir.join("nested").join("large_context.md"), "not selected").unwrap();

        let tree = build_compact_workspace_root(&temp_dir).unwrap();

        assert!(tree.starts_with("@tree-v1\n"));
        assert!(!tree.contains("nested"));
        assert!(!tree.contains("large_context.md"));
    }

    #[test]
    fn visible_workspace_tree_is_exported_without_expanding_scope() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_visible_tree_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let visible_tree = "@tree-v1\nproject/\n  src/\n    main.js";

        let tree = workspace_tree_for_export(&temp_dir, Some(visible_tree)).unwrap();

        assert_eq!(tree, format!("{visible_tree}\n"));
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
    fn theory_import_checklist_marks_complete_when_all_artifacts_exist() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_checklist_complete_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("theory")).unwrap();
        fs::create_dir_all(temp_dir.join("reports")).unwrap();

        fs::write(
            temp_dir.join("theory").join("chapter_1.md"),
            "# Lambda CDM Notes\n\nThis chapter summarizes assumptions and predictions.\n",
        )
        .unwrap();

        fs::write(
            temp_dir.join("master_axiom.md"),
            "## Core Axiom\n\ntext\n\n## Hypothesis\n\ntext\n\n## Predictions\n\ntext\n\n## Observational Consequences\n\ntext\n",
        )
        .unwrap();

        fs::write(temp_dir.join("ai_briefing.md"), "briefing").unwrap();
        fs::write(temp_dir.join("session_recap.md"), "recap").unwrap();
        fs::write(temp_dir.join("project_awareness.md"), "awareness").unwrap();
        fs::write(temp_dir.join("workspace_tree.txt"), "tree").unwrap();
        fs::write(temp_dir.join("reports").join("experiment_run_001.md"), "artifact").unwrap();
        fs::write(temp_dir.join("reports").join("scorecard_validation_001.json"), "{}").unwrap();

        let checklist = build_theory_import_checklist(
            &temp_dir,
            temp_dir.join("theory").to_str().unwrap(),
            &temp_dir.join("master_axiom.md"),
            "",
        );

        assert_eq!(checklist["status"].as_str().unwrap(), "complete");
        assert_eq!(checklist["completed_count"].as_u64().unwrap(), 6);
    }

    #[test]
    fn theory_import_checklist_surfaces_missing_steps() {
        let temp_dir = std::env::temp_dir().join(format!("physics_ide_checklist_incomplete_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("theory")).unwrap();

        fs::write(
            temp_dir.join("theory").join("chapter_1.md"),
            "# Incomplete Theory\n\nA short draft.\n",
        )
        .unwrap();

        let checklist = build_theory_import_checklist(
            &temp_dir,
            temp_dir.join("theory").to_str().unwrap(),
            &temp_dir.join("master_axiom.md"),
            "",
        );

        assert_eq!(checklist["status"].as_str().unwrap(), "incomplete");
        assert!(checklist["completed_count"].as_u64().unwrap() < checklist["total_count"].as_u64().unwrap());
        assert_eq!(checklist["next_recommended_step"].as_str().unwrap(), "Master axiom");
    }

    #[test]
    fn parse_validate_model_selection_payload_accepts_direct_and_wrapped_objects() {
        let direct_payload = serde_json::json!({
            "provider": "openai",
            "model": "gpt-4.1",
            "apiKey": "sk-test"
        });
        let wrapped_payload = serde_json::json!({
            "payload": {
                "provider": "openai",
                "model": "gpt-4.1",
                "apiKey": "sk-test"
            }
        });

        let direct = serde_json::from_value::<ValidateModelSelectionPayload>(direct_payload).unwrap();
        let wrapped = parse_validate_model_selection_payload(wrapped_payload).unwrap();

        assert_eq!(direct.provider, "openai");
        assert_eq!(direct.model, "gpt-4.1");
        assert_eq!(direct.api_key, "sk-test");
        assert_eq!(wrapped.provider, "openai");
        assert_eq!(wrapped.model, "gpt-4.1");
        assert_eq!(wrapped.api_key, "sk-test");
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
                llm_usage: std::sync::Mutex::new(std::collections::HashMap::new()),
                embedding_model: std::sync::Mutex::new(None),
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            get_help_doc,
            search_help_docs,
            list_markdown_documents,
            read_markdown_document,
            search_markdown_documents,
            edit_markdown_document_with_git_save,
            get_markdown_git_feedback,
            list_theory_profiles,
            save_theory_profile,
            load_theory_profile,
            rename_theory_profile,
            delete_theory_profile,
            close_active_theory,
            save_root_directory,
            save_user_settings,
            read_directory,
            save_as_version,
            restore_version,
            save_as_hypothesis,
            get_current_branch,
            terminate_hypothesis_branch,
            get_version_tags,
            save_equation_to_md,
            save_scratchpad_content,
            read_attachment_file,
            compute_cosmology_metrics_command,
            generate_empirical_analysis_primer,
            export_workspace_tree,
            estimate_prompt_usage,
            get_llm_usage,
            run_openai_cache_probe,
            run_structural_ab_probe,
            send_llm_prompt,
            compile_ai_briefing,
            validate_model_selection,
            fetch_provider_model_catalog,
            verify_theory_import_checklist,
            import_theory_source_command,
            generate_master_axiom_from_theory,
            list_markdown_files,
            collect_probe_evidence,
            refresh_retrieval_index,
            inspect_retrieval_index,
            rebuild_retrieval_index,
            query_retrieval_index,
            delete_retrieval_index,
            get_embedding_model_status,
            install_embedding_model,
            run_retrieval_benchmark,
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
