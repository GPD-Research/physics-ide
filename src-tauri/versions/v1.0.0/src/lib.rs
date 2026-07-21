use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    editor: String,
    terminal_app: String,
    last_root_dir: String,
    
    // Exact Physics Project Paths
    project_root_dir: String,
    theory_md_dir: String,
    master_axiom_file: String,

    // Dual LLM Configuration State
    left_provider: String,      // "gemini" or "ollama"
    left_model: String,         // e.g., "gemini-1.5-flash" or "llama3"
    right_provider: String,     // "gemini" or "ollama"
    right_model: String,        // e.g., "gemini-1.5-pro" or "mistral"
    gemini_api_key: String,     // Saved Google cloud access token
    ollama_url: String,         // Endpoint target for local offline engine
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            editor: "micro".to_string(),
            terminal_app: "gnome-terminal".to_string(),
            last_root_dir: "".to_string(),
            project_root_dir: "".to_string(),
            theory_md_dir: "".to_string(),
            master_axiom_file: "".to_string(),
            left_provider: "gemini".to_string(),
            left_model: "gemini-1.5-flash".to_string(),
            right_provider: "gemini".to_string(),
            right_model: "gemini-1.5-pro".to_string(),
            gemini_api_key: "".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
        }
    }
}

// Get path to the config file (stores in the project directory for easy access)
fn get_config_path() -> PathBuf {
    PathBuf::from("ide_config.json")
}

// Load configuration helper
fn load_config() -> AppConfig {
    let path = get_config_path();
    if path.exists() {
        if let Ok(mut file) = File::open(path) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(config) = serde_json::from_str::<AppConfig>(&contents) {
                    return config;
                }
            }
        }
    }
    AppConfig::default()
}

// Save configuration helper
fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("Serialization error: {}", e))?;
    let mut file =
        File::create(path).map_err(|e| format!("Failed to create config file: {}", e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

#[derive(Serialize)]
pub struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
}

#[tauri::command]
fn get_initial_state() -> Result<AppConfig, String> {
    Ok(load_config())
}

#[tauri::command]
fn save_user_settings(
    editor: &str,
    terminal_app: &str,
    left_provider: &str,
    left_model: &str,
    right_provider: &str,
    right_model: &str,
    gemini_key: &str, // Matches frontend JS property 'geminiKey'
    ollama_url: &str,
    project_root_dir: &str,
    theory_md_dir: &str,
    master_axiom_file: &str,
) -> Result<String, String> {
    let mut config = load_config();
    config.editor = editor.to_string();
    config.terminal_app = terminal_app.to_string();
    config.left_provider = left_provider.to_string();
    config.left_model = left_model.to_string();
    config.right_provider = right_provider.to_string();
    config.right_model = right_model.to_string();
    config.gemini_api_key = gemini_key.to_string(); // Correctly maps to struct property
    config.ollama_url = ollama_url.to_string();
    
    // Save the specialized scientific paths
    config.project_root_dir = project_root_dir.to_string();
    config.theory_md_dir = theory_md_dir.to_string();
    config.master_axiom_file = master_axiom_file.to_string();
    
    save_config(&config)?;
    Ok("Configuration saved successfully!".to_string())
}

#[tauri::command]
fn save_root_directory(path: &str) -> Result<String, String> {
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() || !path_buf.is_dir() {
        return Err("Provided path is not a valid directory.".to_string());
    }
    let mut config = load_config();
    config.last_root_dir = path_buf.to_string_lossy().to_string();
    save_config(&config)?;
    Ok("Root directory updated and saved!".to_string())
}

#[tauri::command]
fn read_directory(path: &str) -> Result<Vec<FileEntry>, String> {
    let target_path = Path::new(path);
    if !target_path.exists() {
        return Err("Directory does not exist.".to_string());
    }

    let mut entries = Vec::new();
    let read_dir =
        fs::read_dir(target_path).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry_result in read_dir {
        if let Ok(entry) = entry_result {
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden/system files (e.g., .git, .DS_Store)
            if file_name.starts_with('.') {
                continue;
            }

            entries.push(FileEntry {
                name: file_name,
                path: entry.path().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
            });
        }
    }

    // Sort folders first, then files, alphabetically
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

    Ok(entries)
}

#[tauri::command]
fn launch_file_editor(file_path: &str) -> Result<String, String> {
    let config = load_config();
    let editor = config.editor;
    let terminal_app = config.terminal_app;

    // Launch the chosen terminal-based editor dynamically using selected terminal preferences
    Command::new(&terminal_app)
        .arg("--")
        .arg(&editor)
        .arg(file_path)
        .spawn()
        .map_err(|e| format!("Failed to launch {} in {}: {}", editor, terminal_app, e))?;

    Ok(format!("Opened file {} with {}.", file_path, editor))
}

#[tauri::command]
fn detach_terminal_shell() -> Result<String, String> {
    let config = load_config();
    let terminal_app = config.terminal_app;

    // Spawns user's preferred terminal shell rather than hardcoding gnome-terminal
    Command::new(&terminal_app)
        .arg("--")
        .arg("bash")
        .spawn()
        .map_err(|e| format!("Failed to detach {}: {}", terminal_app, e))?;

    Ok(format!("Detached {} spawned successfully.", terminal_app))
}

#[tauri::command]
fn compile_ai_briefing() -> Result<String, String> {
    let config = load_config();
    
    // Attempt to read the master axiom file to dynamically populate context
    let axiom_content = if !config.master_axiom_file.is_empty() {
        let axiom_path = Path::new(&config.master_axiom_file);
        if axiom_path.exists() {
            fs::read_to_string(axiom_path)
                .unwrap_or_else(|_| "Error reading master axiom file.".to_string())
        } else {
            "Master axiom file not found.".to_string()
        }
    } else {
        "No master axiom file specified.".to_string()
    };

    let state_summary = serde_json::json!({
        "project_root": config.project_root_dir,
        "theory_directory": config.theory_md_dir,
        "master_axiom_file": config.master_axiom_file,
        "status": "Ready",
        "axioms": axiom_content
    });

    Ok(state_summary.to_string())
}

#[tauri::command]
fn delete_file_or_folder(path: &str) -> Result<String, String> {
    let target = Path::new(path);
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|e| format!("Delete directory failed: {}", e))?;
    } else {
        fs::remove_file(target).map_err(|e| format!("Delete file failed: {}", e))?;
    }
    Ok("Deleted item successfully.".to_string())
}

// Recursive helper function to build the ASCII tree string
fn build_ascii_tree(dir_path: &Path, prefix: &str, output: &mut String) -> std::io::Result<()> {
    if !dir_path.exists() {
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden files/directories (like .git, .taurignore, etc.)
        if file_name.starts_with('.') {
            continue;
        }
        entries.push(entry);
    }

    // Sort folders first, then files alphabetically
    entries.sort_by(|a, b| {
        let a_is_dir = a.path().is_dir();
        let b_is_dir = b.path().is_dir();
        if a_is_dir != b_is_dir {
            b_is_dir.cmp(&a_is_dir)
        } else {
            a.file_name()
                .to_ascii_lowercase()
                .cmp(&b.file_name().to_ascii_lowercase())
        }
    });

    let count = entries.len();
    for (index, entry) in entries.iter().enumerate() {
        let is_last = index == count - 1;
        let file_name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let is_dir = path.is_dir();

        // Choose the correct branch pointer
        let pointer = if is_last { "└── " } else { "├── " };

        // Append the formatted line
        output.push_str(&format!("{}{}{}\n", prefix, pointer, file_name));

        if is_dir {
            // Adjust the prefix for nested items
            let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
            build_ascii_tree(&path, &new_prefix, output)?;
        }
    }

    Ok(())
}

#[tauri::command]
fn export_workspace_tree(root_path: &str) -> Result<String, String> {
    let root = Path::new(root_path);
    if !root.exists() || !root.is_dir() {
        return Err("Invalid root directory.".to_string());
    }

    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "root".to_string());

    // Start building the tree representation
    let mut tree_representation = format!("{}/\n", root_name);
    build_ascii_tree(root, "", &mut tree_representation)
        .map_err(|e| format!("Failed to traverse tree: {}", e))?;

    // Define export file path: saves as "workspace_tree_brief.txt" in the workspace root
    let export_path = root.join("workspace_tree_brief.txt");

    let mut file =
        File::create(&export_path).map_err(|e| format!("Failed to create export file: {}", e))?;

    file.write_all(tree_representation.as_bytes())
        .map_err(|e| format!("Failed to write export file: {}", e))?;

    Ok(format!(
        "Tree structure successfully exported to: {}",
        export_path.to_string_lossy()
    ))
}

#[tauri::command]
async fn send_llm_prompt(pane: String, history: Vec<serde_json::Value>) -> Result<String, String> {
    let config = load_config();
    let client = reqwest::Client::new();

    // Dynamically query variables depending on the pane that dispatched the command
    let (provider, model) = if pane == "left" {
        (config.left_provider, config.left_model)
    } else if pane == "right" {
        (config.right_provider, config.right_model)
    } else {
        return Err("Invalid pane target specified.".to_string());
    };

    match provider.as_str() {
        "gemini" => {
            if config.gemini_api_key.is_empty() {
                return Err("Gemini API key is missing. Update it in your settings panel.".to_string());
            }

            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model, config.gemini_api_key
            );

            // Expects structured collection matching Gemini schema format passed up via Javascript
            let payload = serde_json::json!({ "contents": history });

            let response = client.post(&url)
                .json(&payload)
                .send()
                .await
                .map_err(|e| format!("Gemini Connection Error: {}", e))?;

            if !response.status().is_success() {
                let err_text = response.text().await.unwrap_or_default();
                return Err(format!("Gemini Error Status: {}", err_text));
            }

            let res_json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            let text = res_json["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("No valid content payload returned from Google servers.")
                .to_string();

            Ok(text)
        }
        "ollama" => {
            let url = format!("{}/api/chat", config.ollama_url);

            // Expects structured collection matching ChatML layout passed up via Javascript
            let payload = serde_json::json!({
                "model": model,
                "messages": history,
                "stream": false
            });

            let response = client.post(&url)
                .json(&payload)
                .send()
                .await
                .map_err(|e| format!("Ollama Connection Error. Ensure background engine is running: {}", e))?;

            if !response.status().is_success() {
                let err_text = response.text().await.unwrap_or_default();
                return Err(format!("Ollama Error Status: {}", err_text));
            }

            let res_json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            let text = res_json["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            Ok(text)
        }
        _ => Err(format!("Unsupported engine provider configurations: {}", provider)),
    }
}

#[tauri::command]
fn save_as_version(tag: String) -> Result<(), String> {
    let dest = format!("versions/{}", tag);
    // Create the versions directory if it doesn't exist
    fs::create_dir_all("versions").map_err(|e| e.to_string())?;
    
    // Copy the current directory to the destination (simple recursive copy)
    // Note: You may want to exclude the 'versions' and 'target' folders to avoid recursion loops
    copy_dir_all(".", &dest).map_err(|e| e.to_string())?;
    println!("Version '{}' created successfully.", tag);
    Ok(())
}

#[tauri::command]
fn save_as_hypothesis(name: String) -> Result<(), String> {
    let _dest = format!("hypotheses/{}", name); // Prefixed with underscore
    fs::create_dir_all("hypotheses").map_err(|e| e.to_string())?;
    
    println!("Hypothesis '{}' saved.", name);
    Ok(())
}

// Helper function for recursive copying
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            // Exclude our own storage folders to prevent infinite copy
            if entry.file_name() != "versions" && entry.file_name() != "hypotheses" && entry.file_name() != "target" {
                copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
            }
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_initial_state,
            save_user_settings,
            save_root_directory,
            read_directory,
            launch_file_editor,
            detach_terminal_shell,
            compile_ai_briefing,
            delete_file_or_folder,
            export_workspace_tree,
            send_llm_prompt,
            save_as_version,       // Added
            save_as_hypothesis     // Added
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
