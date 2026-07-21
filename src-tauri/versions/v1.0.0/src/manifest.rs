use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CausalHistory {
    pub auto_generated: Vec<String>,
    pub user_note: Option<String>,
    pub last_updated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ManifestNode {
    pub path: String,
    pub is_relevant: bool,
    pub show_children: bool,
    pub causal_history: CausalHistory,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProjectPrimeManifest {
    pub project_name: String,
    pub nodes: Vec<ManifestNode>,
}
use std::fs;
use std::path::Path;

impl ProjectPrimeManifest {
    pub fn load_from_file() -> Self {
        let path = "project_prime_manifest.json";
        if Path::new(path).exists() {
            let data = fs::read_to_string(path).expect("Unable to read manifest");
            serde_json::from_str(&data).expect("Error parsing manifest JSON")
        } else {
            // Default blank state if file is missing
            ProjectPrimeManifest {
                project_name: "physics_ide".to_string(),
                nodes: Vec::new(),
            }
        }
    }
}
