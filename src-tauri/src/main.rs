#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod manifest;
use notify::{Watcher, RecursiveMode};
use std::path::Path;
use std::thread;
use std::time::{Instant, Duration};

fn main() {
    // Start the watcher thread before the app runs
    thread::spawn(|| {
        if let Err(e) = watch_project() {
            eprintln!("Watcher error: {:?}", e);
        }
    });

    // Run your existing library logic
    physics_ide_lib::run();
}

fn watch_project() -> notify::Result<()> {
    let manifest = manifest::ProjectPrimeManifest::load_from_file();
    println!("Manifest loaded for project: {}", manifest.project_name);

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(Path::new("."), RecursiveMode::Recursive)?;

    // Debounce state
    let mut last_event_time = Instant::now();
    let debounce_duration = Duration::from_secs(2);

    for res in rx {
        match res {
            Ok(event) => {
                // Filter: Ignore internal manifest updates or git/target noise
                if event.paths.iter().any(|p| p.to_str().map_or(false, |s| s.contains("project_prime_manifest.json") || s.contains(".git") || s.contains("target"))) {
                    continue;
                }

                // Debounce check
                if last_event_time.elapsed() > debounce_duration {
                    println!("Change confirmed (debounced): {:?}", event.kind);
                    // Here: Call your logic to sync with the manifest
                    last_event_time = Instant::now();
                }
            },
            Err(e) => println!("Watch error: {:?}", e),
        }
    }
    Ok(())
}
