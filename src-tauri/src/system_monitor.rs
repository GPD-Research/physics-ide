//! Live CPU / memory / Ollama-process readings for the View ▸ monitor windows.
//! Sampling happens only when a monitor window polls, so it costs nothing while closed.

use serde::Serialize;
use std::sync::{Arc, Mutex};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// One `System` kept across polls: CPU percentages are deltas between two refreshes.
#[derive(Default)]
pub struct SystemMonitorState(pub Arc<Mutex<Option<System>>>);

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ProcessGroupStats {
    pub name: String,
    pub process_count: usize,
    /// Summed per-process CPU, `top` convention: 100 = one logical core.
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Serialize, Debug, Clone)]
pub struct SystemStats {
    pub cpu_brand: String,
    pub core_usage: Vec<f32>,
    pub total_cpu: f32,
    pub physical_cores: Option<usize>,
    pub memory_total: u64,
    pub memory_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub load_average: [f64; 3],
    pub uptime_secs: u64,
    pub ollama: Option<ProcessGroupStats>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorWindowSpec {
    pub kind: &'static str,
    pub label: &'static str,
    pub title: &'static str,
    pub width: f64,
    pub height: f64,
}

/// Ollama serves from `ollama serve` and runs models in `ollama runner` /
/// `ollama_llama_server` / `llama-server` children depending on the version.
pub fn is_ollama_process(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("ollama") || n.contains("llama-server") || n.contains("llama_server")
}

pub fn group_stats<I>(name: &str, samples: I) -> Option<ProcessGroupStats>
where
    I: IntoIterator<Item = (f32, u64)>,
{
    let mut group = ProcessGroupStats {
        name: name.to_string(),
        process_count: 0,
        cpu_percent: 0.0,
        memory_bytes: 0,
    };
    for (cpu, memory) in samples {
        group.process_count += 1;
        group.cpu_percent += cpu;
        group.memory_bytes += memory;
    }
    (group.process_count > 0).then_some(group)
}

pub fn window_spec(kind: &str, logical_cores: usize) -> Result<MonitorWindowSpec, String> {
    match kind {
        "cpu" => Ok(MonitorWindowSpec {
            kind: "cpu",
            label: "sysmon-cpu",
            title: "CPU Monitor",
            width: (60.0 + logical_cores as f64 * 26.0).clamp(300.0, 960.0),
            height: 270.0,
        }),
        "memory" => Ok(MonitorWindowSpec {
            kind: "memory",
            label: "sysmon-memory",
            title: "Memory Monitor",
            width: 420.0,
            height: 250.0,
        }),
        "system" => Ok(MonitorWindowSpec {
            kind: "system",
            label: "sysmon-system",
            title: "System Monitor",
            width: 340.0,
            height: (330.0 + (logical_cores as f64 / 2.0).ceil() * 17.0).clamp(400.0, 900.0),
        }),
        other => Err(format!("Unknown monitor '{other}' (expected cpu, memory or system)")),
    }
}

/// The theme query is built by the main window from its CSS variables; accept only
/// URL-encoded form characters so it cannot alter the window URL beyond its query.
pub fn is_safe_query(query: &str) -> bool {
    query
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "%&=+._-".contains(c))
}

pub fn sample(state: &Mutex<Option<System>>, include_processes: bool) -> Result<SystemStats, String> {
    let mut guard = state
        .lock()
        .map_err(|_| "System monitor state is poisoned".to_string())?;
    let sys = guard.get_or_insert_with(System::new);
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let ollama = if include_processes {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        group_stats(
            "ollama",
            sys.processes()
                .values()
                .filter(|p| is_ollama_process(&p.name().to_string_lossy()))
                .map(|p| (p.cpu_usage(), p.memory())),
        )
    } else {
        None
    };

    let load = System::load_average();
    Ok(SystemStats {
        cpu_brand: sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default(),
        core_usage: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        total_cpu: sys.global_cpu_usage(),
        physical_cores: System::physical_core_count(),
        memory_total: sys.total_memory(),
        memory_used: sys.used_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
        load_average: [load.one, load.five, load.fifteen],
        uptime_secs: System::uptime(),
        ollama,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ollama_server_and_runner_names() {
        for name in ["ollama", "ollama_llama_server", "Ollama", "llama-server"] {
            assert!(is_ollama_process(name), "{name}");
        }
        for name in ["bash", "physics-ide", "llama.py"] {
            assert!(!is_ollama_process(name), "{name}");
        }
    }

    #[test]
    fn groups_sum_cpu_and_memory() {
        let g = group_stats("ollama", [(400.0, 1_000), (12.5, 500)]).unwrap();
        assert_eq!(g.process_count, 2);
        assert!((g.cpu_percent - 412.5).abs() < 1e-3);
        assert_eq!(g.memory_bytes, 1_500);
        assert!(group_stats("ollama", std::iter::empty()).is_none());
    }

    #[test]
    fn window_specs_scale_with_cores_and_reject_unknown() {
        assert_eq!(window_spec("cpu", 16).unwrap().width, 476.0);
        assert_eq!(window_spec("cpu", 2).unwrap().width, 300.0);
        assert_eq!(window_spec("memory", 8).unwrap().label, "sysmon-memory");
        assert!(window_spec("gpu", 8).is_err());
    }

    #[test]
    fn theme_query_must_be_form_encoded() {
        assert!(is_safe_query("bg=%23181825&text=rgba%2824%2C+24%29&scheme=dark"));
        assert!(!is_safe_query("bg=x#frag"));
        assert!(!is_safe_query("a=b/../../etc"));
    }
}
