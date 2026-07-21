use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

// --- RUNTIME MEMORY STATE ---
pub struct AppState {
    pub workspace_path: std::sync::Mutex<String>,
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

#[tauri::command]
fn compile_ai_briefing(state: tauri::State<AppState>) -> Result<String, String> {
    // The frontend expects a stringified JSON object to parse for the tooltip
    let current_path = state.workspace_path.lock().unwrap_or_else(|_| panic!("Mutex poisoned")).clone();
    
    // TODO: Write logic to scan equations and structure
    let briefing = serde_json::json!({
        "status": "Ready (Mocked)",
        "project_root": if current_path.is_empty() { "Not set" } else { &current_path }
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
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(format!("Equation saved successfully to {}", path))
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
