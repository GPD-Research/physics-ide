use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

// --- RUNTIME MEMORY STATE ---
pub struct AppState {
    pub workspace_path: std::sync::Mutex<String>,
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

// --- PERSISTENT DISK CONFIGURATION ---
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AppConfig {
    last_root_dir: String,
    editor: String,
    terminal_app: String,
    gemini_api_key: String,
    openai_api_key: String,
    ollama_url: String,
    left_provider: String,
    left_model: String,
    right_provider: String,
    right_model: String,
    project_root_dir: String,
    theory_md_dir: String,
    master_axiom_file: String,
    theme: String,           // <-- NEW
    custom_accent: String,   // <-- NEW
    custom_bg_panel: String, // <-- NEW
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

// Helper: Resolve the OS-specific config path
fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
        
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    
    path.push("config.json");
    Ok(path)
}

// --- TAURI COMMANDS ---

#[tauri::command]
fn get_initial_state(app: AppHandle) -> AppConfig {
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(config_data) = fs::read_to_string(config_path) {
            return serde_json::from_str(&config_data).unwrap_or_default();
        }
    }
    AppConfig::default()
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
    if !root.exists() {
        return Err(format!("Root path does not exist: {}", rootPath));
    }

    fn build_tree(dir: &std::path::Path, prefix: &str, output: &mut String) -> std::io::Result<()> {
        let entries = std::fs::read_dir(dir)?;
        let entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        
        for (i, entry) in entries.iter().enumerate() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            
            // Skip target or hidden configuration folders to keep tree clean
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

    let mut tree_string = format!("{}\n", root.file_name().unwrap_or_default().to_string_lossy());
    build_tree(root, "", &mut tree_string).map_err(|e| e.to_string())?;
    
    // Save to file instead of returning the massive string
    let output_file_path = root.join("workspace_tree.txt");
    std::fs::write(&output_file_path, tree_string).map_err(|e| format!("Failed to write file: {}", e))?;
    
    // Return a short, safe string to the frontend UI
    Ok(format!("File Tree txt document generated: {:?}", output_file_path))
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

#[tauri::command]
fn send_llm_prompt(pane: String, history: Vec<serde_json::Value>) -> Result<String, String> {
    // Note: We use serde_json::Value for history because your frontend sends 
    // different JSON shapes depending on if it's formatting for Gemini or Ollama.
    
    // TODO: Implement actual HTTP requests to Ollama or Gemini APIs here
    println!("Received LLM prompt for {} pane. History length: {}", pane, history.len());
    
    Ok(format!("Backend placeholder: Acknowledged prompt for the {} pane.", pane))
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

    format!(
        "# Master Axiom\n\n## Core Axiom\nThe {} framework posits that cosmological structure emerges from a self-consistent relation between order, constraint, and recursive refinement.\n\n## Assumptions\n- Assumption 1: State the foundational conditions under which the model is expected to hold.\n- Assumption 2: State any symmetry, conservation law, or boundary condition that is required.\n\n## Hypothesis\n{}\n\n## Predictions\n1. Specify a measurable signature or scaling relation that follows from the hypothesis.\n2. State a limiting case or boundary condition that should produce a distinct outcome.\n3. Describe the expected observational or analytic difference from competing models.\n\n## Observational Consequences\n- Identify the observational patterns, data products, or simulation outputs implied by the theory.\n- Explain how those consequences would be distinguished from alternative interpretations.\n\n## Testable Criteria\n- What evidence would confirm the hypothesis?\n- What evidence would falsify or constrain it?\n\n## Lagrangian / Action\n{}\n\n## Source Context\n- Theory directory: {}\n- Master axiom file: {}\n- Files scanned: {}\n",
        theory_label,
        hypothesis,
        lagrangian,
        theory_dir,
        master_axiom_path,
        scan["files_scanned"].as_u64().unwrap_or(0)
    )
}

#[tauri::command]
fn compile_ai_briefing(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<String, String> {
    let current_path = state.workspace_path.lock().unwrap_or_else(|_| panic!("Mutex poisoned")).clone();

    let mut config = AppConfig::default();
    if let Ok(config_path) = get_config_path(&app) {
        if let Ok(data) = fs::read_to_string(config_path) {
            config = serde_json::from_str::<AppConfig>(&data).unwrap_or_default();
        }
    }

    let theory_dir = if !config.theory_md_dir.is_empty() {
        config.theory_md_dir.clone()
    } else if !config.project_root_dir.is_empty() {
        config.project_root_dir.clone()
    } else {
        current_path.clone()
    };

    let scan = scan_markdown_theory(&theory_dir);
    let template = build_master_axiom_template(&theory_dir, &config.master_axiom_file, &scan);

    if !config.master_axiom_file.is_empty() {
        let master_path = PathBuf::from(&config.master_axiom_file);
        if let Some(parent) = master_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&master_path, &template);
    }

    let briefing = serde_json::json!({
        "status": "Ready",
        "project_root": if current_path.is_empty() { "Not set".to_string() } else { current_path.clone() },
        "theory_directory": theory_dir,
        "master_axiom_file": config.master_axiom_file,
        "files_scanned": scan["files_scanned"],
        "lagrangian_candidates": scan["lagrangian_candidates"],
        "hypothesis_candidates": scan["hypothesis_candidates"],
        "template": template
    });

    Ok(briefing.to_string())
}

#[tauri::command]
fn launch_file_editor(file_path: String, terminal_app: String, editor: String) -> Result<String, String> {
    let mut cmd = std::process::Command::new(&terminal_app);

    // Handle argument flag differences based on the chosen terminal app
    if terminal_app.contains("gnome-terminal") {
        cmd.arg("--").arg(&editor).arg(&file_path);
    } else {
        // Standard flag for Alacritty, Konsole, XTerm, etc.
        cmd.arg("-e").arg(&editor).arg(&file_path);
    }

    cmd.spawn()
        .map_err(|e| format!("Failed to spawn {} with {}: {}", terminal_app, editor, e))?;

    Ok(format!("Editor successfully launched in {} using {}: {}", terminal_app, editor, file_path))
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

    let parent = src.parent().ok_or_else(|| "Invalid parent directory".to_string())?;
    let folder_name = src.file_name().ok_or_else(|| "Invalid file name".to_string())?;
    let dest_dir = parent.join(format!("{}_{}", folder_name.to_str().unwrap(), tag));

    fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            
            if file_type.is_dir() {
                if entry.file_name() == "target" { continue; }
                copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
            } else {
                std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
            }
        }
        Ok(())
    }

    copy_recursive(src, &dest_dir)
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
#[allow(non_snake_case)]
fn save_user_settings(
    editor: String,
    terminal_app: String,
    gemini_key: String,
    openai_key: String,
    ollama_url: String,
    left_provider: String,
    left_model: String,
    right_provider: String,
    right_model: String,
    project_root_dir: String,
    theory_md_dir: String,
    master_axiom_file: String,
    theme: String,
    custom_accent: String,
    custom_bg_panel: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    
    let config_path = get_config_path(&app)?;
    
    let mut config = if let Ok(data) = std::fs::read_to_string(&config_path) {
        serde_json::from_str::<AppConfig>(&data).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    config.editor = editor;
    config.terminal_app = terminal_app;
    config.gemini_api_key = gemini_key;
    config.openai_api_key = openai_key;
    config.ollama_url = ollama_url;
    config.left_provider = left_provider;
    config.left_model = left_model;
    config.right_provider = right_provider;
    config.right_model = right_model;
    config.project_root_dir = project_root_dir;
    config.theory_md_dir = theory_md_dir;
    config.master_axiom_file = master_axiom_file;
    
    // Assign the new appearance parameters
    config.theme = theme;
    config.custom_accent = custom_accent;
    config.custom_bg_panel = custom_bg_panel;

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
            launch_file_editor,
            detach_terminal_shell,
            git_pull,
            git_push
            // Add your other commands here...
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
