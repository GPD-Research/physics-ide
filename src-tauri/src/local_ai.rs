//! Local AI (Ollama) support: hardware probing, curated model catalog,
//! performance estimation, install-command construction, and chat calls.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const DEFAULT_OLLAMA_HOST: &str = "http://127.0.0.1:11434";
pub const DEFAULT_LOCAL_MODEL: &str = "llama3.1:8b";

/// Prompt size the estimate assumes per query: startup primer (master axiom)
/// plus retrieval packet plus a few turns of history, and a typical reply.
/// Per-query prompt tokens actually evaluated: Ollama reuses the KV cache for the
/// unchanged primer prefix, so only the new turn plus retrieval evidence is processed.
const ASSUMED_PROMPT_TOKENS: f64 = 2_000.0;
const ASSUMED_REPLY_TOKENS: f64 = 500.0;
/// Beyond this a model is reported unusable; slower-than-comfortable models are "deep".
const MAX_ACCEPTABLE_SECONDS: f64 = 300.0;
const SPECTRUM_MAX_SECONDS: f64 = 120.0;
const DISK_FULL_FRACTION: f64 = 0.95;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vram_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareSpecs {
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub ram_total_gb: f64,
    pub ram_available_gb: f64,
    pub gpus: Vec<GpuInfo>,
    pub vram_total_gb: f64,
    pub disk_path: String,
    pub disk_total_gb: f64,
    pub disk_free_gb: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelSpec {
    pub name: String,
    pub family: String,
    pub params_b: f64,
    pub quant: String,
    pub file_gb: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceEstimate {
    /// "quick" | "balanced" | "deep" | "unusable"
    pub verdict: String,
    /// 0.0 = quick end, 1.0 = deep end of the spectrum.
    pub spectrum_position: f64,
    pub est_response_seconds: f64,
    pub gen_tokens_per_sec: f64,
    pub prompt_tokens_per_sec: f64,
    pub runs_on: String,
    pub memory_needed_gb: f64,
    pub disk_ok: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledModel {
    pub name: String,
    pub size_gb: f64,
    pub quant: String,
    pub params: String,
    pub family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaStatus {
    pub binary_found: bool,
    pub binary_path: String,
    pub running: bool,
    pub version: String,
    pub host: String,
    pub installed_models: Vec<InstalledModel>,
    pub error: String,
}

fn model(name: &str, family: &str, params_b: f64, quant: &str, file_gb: f64, description: &str) -> LocalModelSpec {
    LocalModelSpec {
        name: name.to_string(),
        family: family.to_string(),
        params_b,
        quant: quant.to_string(),
        file_gb,
        description: description.to_string(),
    }
}

/// Curated catalog of Ollama library models suitable for theory/physics work.
/// Sizes are approximate download sizes from the Ollama library.
pub fn curated_model_catalog() -> Vec<LocalModelSpec> {
    vec![
        model("llama3.2:1b", "Llama 3.2", 1.0, "Q8_0", 1.3, "Tiny general model; very fast, limited reasoning."),
        model("llama3.2:3b", "Llama 3.2", 3.0, "Q4_K_M", 2.0, "Small general model; quick replies, light reasoning."),
        model("qwen2.5:3b", "Qwen 2.5", 3.0, "Q4_K_M", 1.9, "Small, strong for its size on math and code."),
        model("phi4-mini:3.8b", "Phi-4 Mini", 3.8, "Q4_K_M", 2.5, "Compact reasoning-focused model."),
        model("llama3.1:8b", "Llama 3.1", 8.0, "Q4_K_M", 4.9, "Default balanced general model."),
        model("qwen2.5:7b", "Qwen 2.5", 7.6, "Q4_K_M", 4.7, "Balanced; good math/LaTeX fluency."),
        model("qwen2.5-coder:7b", "Qwen 2.5 Coder", 7.6, "Q4_K_M", 4.7, "Code-oriented; good for scripting tasks."),
        model("mistral:7b", "Mistral", 7.2, "Q4_K_M", 4.1, "Fast general model."),
        model("deepseek-r1:8b", "DeepSeek R1", 8.0, "Q4_K_M", 4.9, "Reasoning model (thinks before answering)."),
        model("gemma2:9b", "Gemma 2", 9.2, "Q4_K_M", 5.4, "Balanced general model."),
        model("qwen2.5:7b-instruct-q8_0", "Qwen 2.5", 7.6, "Q8_0", 8.1, "Higher-fidelity quant of Qwen 2.5 7B."),
        model("llama3.1:8b-instruct-q8_0", "Llama 3.1", 8.0, "Q8_0", 8.5, "Higher-fidelity quant of Llama 3.1 8B."),
        model("phi4:14b", "Phi-4", 14.7, "Q4_K_M", 9.1, "Strong reasoning and math at mid size."),
        model("qwen2.5:14b", "Qwen 2.5", 14.8, "Q4_K_M", 9.0, "Deeper general model with good math."),
        model("qwen2.5-coder:14b", "Qwen 2.5 Coder", 14.8, "Q4_K_M", 9.0, "Mid-size code model."),
        model("deepseek-r1:14b", "DeepSeek R1", 14.8, "Q4_K_M", 9.0, "Mid-size reasoning model."),
        model("gemma2:27b", "Gemma 2", 27.2, "Q4_K_M", 16.0, "Large general model."),
        model("qwen2.5:32b", "Qwen 2.5", 32.8, "Q4_K_M", 20.0, "Large; near frontier quality on math."),
        model("qwen2.5-coder:32b", "Qwen 2.5 Coder", 32.8, "Q4_K_M", 20.0, "Large code model."),
        model("deepseek-r1:32b", "DeepSeek R1", 32.8, "Q4_K_M", 20.0, "Large reasoning model."),
        model("llama3.3:70b", "Llama 3.3", 70.6, "Q4_K_M", 43.0, "Very large; needs workstation-class memory."),
        model("deepseek-r1:70b", "DeepSeek R1", 70.6, "Q4_K_M", 43.0, "Very large reasoning model."),
        model("qwen2.5:72b", "Qwen 2.5", 72.7, "Q4_K_M", 47.0, "Very large; deepest math of the Qwen family."),
    ]
}

/// Selectable Ollama quantization tags: (tag as used in Ollama library names, approx GB per billion params, label).
pub const QUANT_OPTIONS: &[(&str, f64, &str)] = &[
    ("q4_0", 0.57, "q4_0 — lightest, fastest, lowest fidelity"),
    ("q4_K_M", 0.62, "q4_K_M — Ollama default balance"),
    ("q5_K_M", 0.72, "q5_K_M — slightly larger, better fidelity"),
    ("q8_0", 1.07, "q8_0 — near-lossless, ~2× the q4 size"),
    ("fp16", 2.0, "fp16 — unquantized, largest"),
];

pub fn quant_gb_per_b(quant: &str) -> Option<f64> {
    QUANT_OPTIONS
        .iter()
        .find(|(tag, _, _)| tag.eq_ignore_ascii_case(quant))
        .map(|(_, gb, _)| *gb)
}

/// Rewrite a curated catalog entry to a specific quantization tag, following the
/// Ollama library naming for each family (`qwen2.5:7b-instruct-q4_0`, `phi4:14b-q8_0`).
/// Returns `None` for families whose tags do not follow a predictable pattern.
pub fn apply_quantization(spec: &LocalModelSpec, quant: &str) -> Option<LocalModelSpec> {
    let quant = quant.trim();
    if quant.is_empty() {
        return Some(spec.clone());
    }
    let gb_per_b = quant_gb_per_b(quant)?;
    let (repo, tag) = spec.name.split_once(':')?;
    let size = tag.split('-').next()?;
    if !size.ends_with('b') {
        return None;
    }
    let name = match repo {
        "qwen2.5" | "qwen2.5-coder" | "llama3.1" | "llama3.2" | "llama3.3" | "mistral" | "gemma2" => {
            format!("{repo}:{size}-instruct-{quant}")
        }
        "phi4" | "phi4-mini" => format!("{repo}:{size}-{quant}"),
        _ => return None,
    };
    let mut out = spec.clone();
    out.name = name;
    out.quant = quant.to_ascii_uppercase().replace("FP16", "F16");
    out.file_gb = round1(spec.params_b * gb_per_b);
    Some(out)
}

fn read_to_string(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn parse_meminfo_kb(meminfo: &str, key: &str) -> Option<f64> {
    meminfo
        .lines()
        .find(|line| line.starts_with(key))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<f64>().ok())
}

fn kb_to_gb(kb: f64) -> f64 {
    kb / (1024.0 * 1024.0)
}

fn bytes_to_gb(bytes: f64) -> f64 {
    bytes / (1024.0 * 1024.0 * 1024.0)
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Validate a user-chosen model storage directory: empty means "Ollama default";
/// otherwise it must be an absolute path to an existing directory.
pub fn normalize_models_dir(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err(format!("Model storage location must be an absolute path (got '{trimmed}')."));
    }
    if !path.is_dir() {
        return Err(format!("Model storage location '{trimmed}' is not an existing directory."));
    }
    Ok(trimmed.to_string())
}

/// Directory where Ollama stores models: the app setting if set, else `$OLLAMA_MODELS`, else `~/.ollama/models`.
pub fn ollama_models_dir_with(configured: &str) -> PathBuf {
    if !configured.trim().is_empty() {
        return PathBuf::from(configured.trim());
    }
    ollama_models_dir()
}

/// Directory where Ollama stores models (`$OLLAMA_MODELS` or `~/.ollama/models`).
pub fn ollama_models_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("OLLAMA_MODELS") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".ollama").join("models")
}

fn nearest_existing(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    while !current.exists() {
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return PathBuf::from("/"),
        }
    }
    current
}

fn disk_usage_gb(path: &Path) -> Option<(f64, f64)> {
    let output = Command::new("df").arg("-k").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_df_k(&String::from_utf8_lossy(&output.stdout))
}

/// Parses POSIX `df -k` output (Linux and macOS): columns are
/// `Filesystem 1K-blocks Used Available ...`; the device name may wrap onto
/// its own line, so the numeric columns are located from the end of the row.
fn parse_df_k(text: &str) -> Option<(f64, f64)> {
    let body: Vec<&str> = text.lines().skip(1).flat_map(|l| l.split_whitespace()).collect();
    let numeric: Vec<f64> = body.iter().filter_map(|t| t.parse::<f64>().ok()).collect();
    if numeric.len() < 3 {
        return None;
    }
    let size_kb = numeric[0];
    let avail_kb = numeric[2];
    Some((kb_to_gb(size_kb), kb_to_gb(avail_kb)))
}

fn sysctl(key: &str) -> Option<String> {
    let output = Command::new("sysctl").args(["-n", key]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Approximate free RAM on macOS from `vm_stat` (free + inactive + speculative pages).
fn macos_available_ram_gb() -> Option<f64> {
    let output = Command::new("vm_stat").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let page_size = text
        .lines()
        .next()
        .and_then(|l| l.split("page size of").nth(1))
        .and_then(|l| l.split_whitespace().next())
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(16384.0);
    let pages = |key: &str| -> f64 {
        text.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split(':').nth(1))
            .map(|v| v.trim().trim_end_matches('.'))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let free = pages("Pages free") + pages("Pages inactive") + pages("Pages speculative");
    if free <= 0.0 {
        return None;
    }
    Some(bytes_to_gb(free * page_size))
}

fn detect_nvidia_gpus() -> Vec<GpuInfo> {
    let output = match Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.rsplitn(2, ',');
            let mem_mb = parts.next()?.trim().parse::<f64>().ok()?;
            let name = parts.next()?.trim().to_string();
            Some(GpuInfo { name, vram_gb: round1(mem_mb / 1024.0) })
        })
        .collect()
}

fn detect_amd_gpus() -> Vec<GpuInfo> {
    let output = match Command::new("rocm-smi").args(["--showmeminfo", "vram", "--csv"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split(',').collect();
            let total_bytes = cols.get(1)?.trim().parse::<f64>().ok()?;
            Some(GpuInfo {
                name: format!("AMD GPU {}", cols.first().unwrap_or(&"").trim()),
                vram_gb: round1(bytes_to_gb(total_bytes)),
            })
        })
        .collect()
}

fn detect_apple_silicon(cpu_model: &str, ram_total_gb: f64, notes: &mut Vec<String>) -> Vec<GpuInfo> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let arm = sysctl("hw.optional.arm64").map(|v| v == "1").unwrap_or(false);
    if !arm {
        notes.push("Intel Mac detected; Ollama runs on CPU here (no Metal acceleration for discrete AMD GPUs).".to_string());
        return Vec::new();
    }
    notes.push(format!(
        "Apple Silicon: the GPU shares the {ram_total_gb} GB of unified memory (about 75% is usable for models); estimates use Metal bandwidth for this chip."
    ));
    vec![GpuInfo {
        name: format!("{} GPU (unified memory)", cpu_model.trim()),
        vram_gb: round1(ram_total_gb),
    }]
}

/// Memory bandwidth (GB/s) of Apple Silicon chips by tier, inferred from the
/// CPU brand string (e.g. "Apple M3 Pro").
fn apple_unified_bandwidth_gbps(chip: &str) -> f64 {
    let chip = chip.to_ascii_lowercase();
    if chip.contains("ultra") {
        800.0
    } else if chip.contains("max") {
        400.0
    } else if chip.contains("pro") {
        200.0
    } else if chip.contains("m4") || chip.contains("m3") || chip.contains("m2") || chip.contains("m1") {
        110.0
    } else {
        100.0
    }
}

pub fn check_hardware_specs_with(models_dir_override: &str) -> HardwareSpecs {
    let mut notes = Vec::new();

    let cpu_cores = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(0);
    let (cpu_model, ram_total_gb, ram_available_gb) = if cfg!(target_os = "macos") {
        let cpu_model = sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "Unknown CPU".to_string());
        let ram_total_gb = sysctl("hw.memsize")
            .and_then(|v| v.parse::<f64>().ok())
            .map(bytes_to_gb)
            .unwrap_or(0.0);
        let ram_available_gb = macos_available_ram_gb().unwrap_or(ram_total_gb * 0.6);
        if ram_total_gb <= 0.0 {
            notes.push("Could not read hw.memsize; RAM figures are unavailable.".to_string());
        }
        (cpu_model, ram_total_gb, ram_available_gb)
    } else {
        let cpuinfo = read_to_string("/proc/cpuinfo");
        let cpu_model = cpuinfo
            .lines()
            .find(|line| line.starts_with("model name"))
            .and_then(|line| line.split(':').nth(1))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());
        let meminfo = read_to_string("/proc/meminfo");
        let ram_total_gb = parse_meminfo_kb(&meminfo, "MemTotal:").map(kb_to_gb).unwrap_or(0.0);
        let ram_available_gb = parse_meminfo_kb(&meminfo, "MemAvailable:").map(kb_to_gb).unwrap_or(0.0);
        if ram_total_gb <= 0.0 {
            notes.push("Could not read /proc/meminfo; RAM figures are unavailable.".to_string());
        }
        (cpu_model, ram_total_gb, ram_available_gb)
    };

    let mut gpus = detect_nvidia_gpus();
    if gpus.is_empty() {
        gpus = detect_amd_gpus();
    }
    if gpus.is_empty() {
        gpus = detect_apple_silicon(&cpu_model, ram_total_gb, &mut notes);
    }
    if gpus.is_empty() {
        notes.push("No discrete GPU detected (nvidia-smi / rocm-smi not available); estimates assume CPU inference.".to_string());
    }
    let vram_total_gb: f64 = gpus.iter().map(|gpu| gpu.vram_gb).sum();

    let models_dir = ollama_models_dir_with(models_dir_override);
    let probe_dir = nearest_existing(&models_dir);
    let (disk_total_gb, disk_free_gb) = disk_usage_gb(&probe_dir).unwrap_or_else(|| {
        notes.push(format!("Could not read free space for {}.", probe_dir.display()));
        (0.0, 0.0)
    });

    HardwareSpecs {
        cpu_model,
        cpu_cores,
        ram_total_gb: round1(ram_total_gb),
        ram_available_gb: round1(ram_available_gb),
        gpus,
        vram_total_gb: round1(vram_total_gb),
        disk_path: models_dir.to_string_lossy().to_string(),
        disk_total_gb: round1(disk_total_gb),
        disk_free_gb: round1(disk_free_gb),
        notes,
    }
}

/// Rough effective memory bandwidth (GB/s) by VRAM tier when the exact card
/// is unknown. Token generation on dense models is bandwidth-bound, so
/// tokens/sec ≈ bandwidth / weight bytes.
fn gpu_bandwidth_gbps(vram_gb: f64) -> f64 {
    if vram_gb >= 40.0 {
        1_500.0
    } else if vram_gb >= 24.0 {
        900.0
    } else if vram_gb >= 16.0 {
        650.0
    } else if vram_gb >= 12.0 {
        450.0
    } else if vram_gb >= 8.0 {
        320.0
    } else {
        200.0
    }
}

fn cpu_bandwidth_gbps(cores: u32) -> f64 {
    if cores >= 16 {
        60.0
    } else if cores >= 8 {
        45.0
    } else {
        30.0
    }
}

pub fn estimate_performance(spec: &LocalModelSpec, hw: &HardwareSpecs) -> PerformanceEstimate {
    let mut reasons = Vec::new();

    // KV cache for a ~16K context plus runtime overhead.
    let kv_gb = (spec.params_b / 8.0).max(0.5) * 1.0;
    let memory_needed_gb = round1(spec.file_gb * 1.1 + kv_gb);

    let unified = cfg!(target_os = "macos") && !hw.gpus.is_empty();
    let vram = hw.vram_total_gb;
    let ram = if hw.ram_total_gb > 0.0 { hw.ram_total_gb } else { f64::INFINITY };

    let (runs_on, bandwidth, prompt_factor) = if !unified && vram > 0.0 && memory_needed_gb <= vram * 0.92 {
        reasons.push(format!("Fits in GPU memory ({memory_needed_gb} GB needed, {vram} GB VRAM)."));
        ("gpu".to_string(), gpu_bandwidth_gbps(vram), 25.0)
    } else if !unified && vram > 0.0 && memory_needed_gb <= vram + ram * 0.8 {
        let gpu_share = (vram * 0.92 / memory_needed_gb).clamp(0.0, 1.0);
        let cpu_bw = cpu_bandwidth_gbps(hw.cpu_cores);
        let blended = 1.0 / (gpu_share / gpu_bandwidth_gbps(vram) + (1.0 - gpu_share) / cpu_bw);
        reasons.push(format!(
            "Splits across GPU and CPU ({:.0}% of weights on GPU); CPU-side layers dominate speed.",
            gpu_share * 100.0
        ));
        ("gpu+cpu".to_string(), blended, 6.0)
    } else if unified && memory_needed_gb <= ram * 0.75 {
        let chip = hw.gpus.first().map(|g| g.name.as_str()).unwrap_or("");
        reasons.push(format!("Fits in unified memory ({memory_needed_gb} GB needed, {} GB RAM).", hw.ram_total_gb));
        ("unified".to_string(), apple_unified_bandwidth_gbps(chip), 15.0)
    } else if memory_needed_gb <= ram * 0.8 {
        reasons.push(format!("Runs on CPU from system RAM ({memory_needed_gb} GB needed, {} GB RAM).", hw.ram_total_gb));
        ("cpu".to_string(), cpu_bandwidth_gbps(hw.cpu_cores), 6.0)
    } else {
        reasons.push(format!(
            "Needs about {memory_needed_gb} GB of memory but only {} GB RAM / {vram} GB VRAM available.",
            hw.ram_total_gb
        ));
        return PerformanceEstimate {
            verdict: "unusable".to_string(),
            spectrum_position: 1.0,
            est_response_seconds: f64::INFINITY,
            gen_tokens_per_sec: 0.0,
            prompt_tokens_per_sec: 0.0,
            runs_on: "none".to_string(),
            memory_needed_gb,
            disk_ok: disk_check(spec, hw, &mut reasons),
            reasons,
        };
    };

    let gen_tps = (bandwidth / spec.file_gb.max(0.1)).min(200.0);
    let prompt_tps = gen_tps * prompt_factor;
    let est_seconds = ASSUMED_PROMPT_TOKENS / prompt_tps + ASSUMED_REPLY_TOKENS / gen_tps + 1.5;

    let disk_ok = disk_check(spec, hw, &mut reasons);

    let verdict = if !disk_ok {
        "unusable".to_string()
    } else if est_seconds > MAX_ACCEPTABLE_SECONDS {
        reasons.push(format!(
            "Estimated {:.0}s per query exceeds the {:.0}s usable limit.",
            est_seconds, MAX_ACCEPTABLE_SECONDS
        ));
        "unusable".to_string()
    } else if est_seconds <= 15.0 {
        "quick".to_string()
    } else if est_seconds <= 45.0 {
        "balanced".to_string()
    } else {
        reasons.push(format!("Slow on this hardware (~{:.0}s per query) but workable.", est_seconds));
        "deep".to_string()
    };

    PerformanceEstimate {
        verdict,
        spectrum_position: (est_seconds / SPECTRUM_MAX_SECONDS).clamp(0.0, 1.0),
        est_response_seconds: round1(est_seconds),
        gen_tokens_per_sec: round1(gen_tps),
        prompt_tokens_per_sec: round1(prompt_tps),
        runs_on,
        memory_needed_gb,
        disk_ok,
        reasons,
    }
}

fn disk_check(spec: &LocalModelSpec, hw: &HardwareSpecs, reasons: &mut Vec<String>) -> bool {
    if hw.disk_total_gb <= 0.0 {
        return true;
    }
    if spec.file_gb > hw.disk_free_gb {
        reasons.push(format!(
            "Download needs {} GB but only {} GB is free on the model disk.",
            spec.file_gb, hw.disk_free_gb
        ));
        return false;
    }
    let used_after = (hw.disk_total_gb - hw.disk_free_gb + spec.file_gb) / hw.disk_total_gb;
    if used_after >= DISK_FULL_FRACTION {
        reasons.push(format!(
            "Installing would fill the disk to {:.0}% (limit 95%).",
            used_after * 100.0
        ));
        return false;
    }
    true
}

pub(crate) fn which(binary: &str) -> Option<String> {
    let output = Command::new("which").arg(binary).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// GUI apps on macOS don't inherit the shell PATH, so check the usual
/// Homebrew and Ollama.app locations when `which` finds nothing.
fn find_ollama_in_known_locations() -> Option<String> {
    [
        "/opt/homebrew/bin/ollama",
        "/usr/local/bin/ollama",
        "/Applications/Ollama.app/Contents/Resources/ollama",
    ]
    .iter()
    .find(|p| Path::new(p).exists())
    .map(|p| p.to_string())
}

pub fn normalize_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return DEFAULT_OLLAMA_HOST.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn http_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))
}

fn parse_installed_models(tags: &serde_json::Value) -> Vec<InstalledModel> {
    tags.get("models")
        .and_then(|value| value.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| {
                    let name = entry.get("name").or_else(|| entry.get("model"))?.as_str()?.to_string();
                    let details = entry.get("details").cloned().unwrap_or(serde_json::Value::Null);
                    let text = |key: &str| details.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    Some(InstalledModel {
                        name,
                        size_gb: round1(bytes_to_gb(entry.get("size").and_then(|v| v.as_f64()).unwrap_or(0.0))),
                        quant: text("quantization_level"),
                        params: text("parameter_size"),
                        family: text("family"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn ollama_status(host: &str) -> OllamaStatus {
    let host = normalize_host(host);
    let mut status = OllamaStatus {
        host: host.clone(),
        ..OllamaStatus::default()
    };
    if let Some(path) = which("ollama").or_else(find_ollama_in_known_locations) {
        status.binary_found = true;
        status.binary_path = path;
    }

    let client = match http_client(5) {
        Ok(client) => client,
        Err(err) => {
            status.error = err;
            return status;
        }
    };

    match client.get(format!("{host}/api/version")).send() {
        Ok(response) if response.status().is_success() => {
            status.running = true;
            if let Ok(parsed) = response.json::<serde_json::Value>() {
                status.version = parsed.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
        }
        Ok(response) => {
            status.error = format!("Ollama server answered {} at {host}", response.status());
            return status;
        }
        Err(err) => {
            status.error = if status.binary_found {
                format!("Ollama is installed but the server is not reachable at {host} ({err}). Start it with `ollama serve`.")
            } else {
                format!("Ollama is not installed and no server is reachable at {host}.")
            };
            return status;
        }
    }

    match client.get(format!("{host}/api/tags")).send() {
        Ok(response) if response.status().is_success() => {
            if let Ok(parsed) = response.json::<serde_json::Value>() {
                status.installed_models = parse_installed_models(&parsed);
            }
        }
        Ok(response) => status.error = format!("Failed to list models: HTTP {}", response.status()),
        Err(err) => status.error = format!("Failed to list models: {err}"),
    }
    status
}

pub fn list_installed_model_names(host: &str) -> Result<Vec<String>, String> {
    let status = ollama_status(host);
    if !status.running {
        return Err(if status.error.is_empty() {
            format!("Ollama server is not reachable at {}", status.host)
        } else {
            status.error
        });
    }
    Ok(status.installed_models.into_iter().map(|model| model.name).collect())
}

/// Returns true when `candidate` names the same model as an installed tag
/// (Ollama appends `:latest` to untagged names).
pub fn model_matches(candidate: &str, installed: &str) -> bool {
    let normalize = |value: &str| {
        let trimmed = value.trim();
        trimmed.strip_suffix(":latest").unwrap_or(trimmed).to_ascii_lowercase()
    };
    normalize(candidate) == normalize(installed)
}

pub fn validate_model(host: &str, model: &str) -> Result<(), String> {
    let installed = list_installed_model_names(host)?;
    if installed.iter().any(|name| model_matches(model, name)) {
        Ok(())
    } else {
        Err(format!(
            "Model '{model}' is not downloaded in Ollama. Use Install from the Local AI panel (or run `ollama pull {model}`)."
        ))
    }
}

/// Shell script run inside a terminal window to install Ollama (if needed)
/// and pull the requested model.
pub fn build_install_script_with_dir(model: &str, ollama_installed: bool, models_dir: &str) -> String {
    build_install_script_for(model, ollama_installed, cfg!(target_os = "macos"), models_dir)
}

pub fn build_install_script_for(model: &str, ollama_installed: bool, macos: bool, models_dir: &str) -> String {
    let quoted = shell_quote(model);
    let mut script = String::from("set -e\n");
    if macos {
        script.push_str("export PATH=\"/opt/homebrew/bin:/usr/local/bin:$PATH\"\n");
    }
    let models_dir = models_dir.trim();
    if !models_dir.is_empty() {
        let quoted_dir = shell_quote(models_dir);
        script.push_str(&format!("export OLLAMA_MODELS={quoted_dir}\n"));
        script.push_str(&format!("mkdir -p {quoted_dir}\n"));
        script.push_str(&format!("echo \"== Models will be stored in {quoted_dir} ==\"\n"));
        script.push_str("if curl -fsS http://127.0.0.1:11434/api/version >/dev/null 2>&1; then\n");
        script.push_str("  echo '== Restarting the Ollama server so it uses this storage location =='\n");
        if macos {
            script.push_str("  brew services stop ollama >/dev/null 2>&1 || true\n");
        } else {
            script.push_str("  (command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet ollama && sudo systemctl stop ollama) || true\n");
        }
        script.push_str("  pkill -f 'ollama serve' >/dev/null 2>&1 || true\n");
        script.push_str("  sleep 2\n");
        script.push_str("fi\n");
        if macos {
            script.push_str("echo 'Note: to make this permanent for a brew-managed Ollama, add OLLAMA_MODELS to its launchd plist or start it via this app.'\n");
        } else {
            script.push_str("echo 'Note: to make this permanent for the systemd service run: sudo systemctl edit ollama  and add  [Service] Environment=OLLAMA_MODELS='\"$OLLAMA_MODELS\"'\n");
        }
    }
    if !ollama_installed {
        script.push_str("echo '== Installing Ollama (https://ollama.com/download) =='\n");
        if macos {
            script.push_str("if command -v brew >/dev/null 2>&1; then\n");
            script.push_str("  brew install ollama\n");
            script.push_str("else\n");
            script.push_str("  echo 'Homebrew not found. Download Ollama for macOS from https://ollama.com/download, open it once, then re-run Install.'\n");
            script.push_str("  read -r -p 'Press Enter to close this window...' _\n");
            script.push_str("  exit 1\n");
            script.push_str("fi\n");
        } else {
            script.push_str("curl -fsSL https://ollama.com/install.sh | sh\n");
        }
    }
    script.push_str("if ! curl -fsS http://127.0.0.1:11434/api/version >/dev/null 2>&1; then\n");
    script.push_str("  echo '== Starting Ollama server in the background =='\n");
    script.push_str("  (nohup ollama serve >/dev/null 2>&1 &)\n");
    script.push_str("  sleep 3\n");
    script.push_str("fi\n");
    script.push_str(&format!("echo \"== Downloading model {} ==\"\n", quoted));
    script.push_str(&format!("ollama pull {}\n", quoted));
    script.push_str("echo\n");
    script.push_str("echo 'Done. Return to the Physics IDE and click Refresh in the Local AI panel.'\n");
    script.push_str("read -r -p 'Press Enter to close this window...' _\n");
    script
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn build_terminal_command(terminal_app: &str, script: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "macos") {
        return build_macos_terminal_command(terminal_app, script);
    }
    build_linux_terminal_command(terminal_app, script)
}

/// macOS terminals can't take an inline command, so the script is written to
/// a `.command` file (Terminal/iTerm run those directly) and opened with
/// `open -a <App>`.
fn build_macos_terminal_command(terminal_app: &str, script: &str) -> (String, Vec<String>) {
    let app = if terminal_app.trim().is_empty() || terminal_app.contains('/') && !terminal_app.ends_with(".app") {
        "Terminal".to_string()
    } else {
        terminal_app.trim().trim_end_matches(".app").to_string()
    };
    let path = std::env::temp_dir().join("physics-ide-ollama-install.command");
    let _ = std::fs::write(&path, format!("#!/bin/bash\n{script}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    (
        "open".to_string(),
        vec!["-a".to_string(), app, path.to_string_lossy().to_string()],
    )
}

fn build_linux_terminal_command(terminal_app: &str, script: &str) -> (String, Vec<String>) {
    let terminal_app = if terminal_app.trim().is_empty() {
        "x-terminal-emulator"
    } else {
        terminal_app.trim()
    };
    let mut args = Vec::new();
    if terminal_app.contains("gnome-terminal") || terminal_app.contains("kgx") {
        args.push("--".to_string());
    } else {
        args.push("-e".to_string());
    }
    args.push("bash".to_string());
    args.push("-c".to_string());
    args.push(script.to_string());
    (terminal_app.to_string(), args)
}

/// Per-pane Ollama runtime overrides. `None` leaves the model's own default in place
/// (the app only adds `num_ctx` when unset, see `OLLAMA_NUM_CTX`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OllamaOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_thread: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_gpu: Option<i64>,
    /// Ollama duration string such as "5m", "1h" or "-1" (keep loaded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
}

impl OllamaOptions {
    /// The `options` object sent with a chat request: model defaults for anything unset,
    /// except `num_ctx`, which falls back to the app's working-context size.
    pub fn request_options(&self) -> serde_json::Value {
        let mut o = serde_json::Map::new();
        o.insert("num_ctx".into(), serde_json::json!(self.num_ctx.unwrap_or(OLLAMA_NUM_CTX)));
        if let Some(v) = self.num_predict { o.insert("num_predict".into(), serde_json::json!(v)); }
        if let Some(v) = self.temperature { o.insert("temperature".into(), serde_json::json!(v)); }
        if let Some(v) = self.top_p { o.insert("top_p".into(), serde_json::json!(v)); }
        if let Some(v) = self.top_k { o.insert("top_k".into(), serde_json::json!(v)); }
        if let Some(v) = self.repeat_penalty { o.insert("repeat_penalty".into(), serde_json::json!(v)); }
        if let Some(v) = self.num_thread { o.insert("num_thread".into(), serde_json::json!(v)); }
        if let Some(v) = self.num_gpu { o.insert("num_gpu".into(), serde_json::json!(v)); }
        serde_json::Value::Object(o)
    }

    pub fn keep_alive(&self) -> Option<&str> {
        self.keep_alive.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }
}

/// What `/api/show` reports about an installed model: its Modelfile PARAMETERs and
/// the architecture's trained context length, so the UI can show real defaults.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LocalModelInfo {
    pub model: String,
    pub family: String,
    pub parameter_size: String,
    pub quantization: String,
    pub context_length: Option<u64>,
    /// PARAMETER lines from the Modelfile, e.g. {"temperature": "0.7", "num_ctx": "8192", "stop": "<|im_end|>"}.
    pub parameters: std::collections::BTreeMap<String, String>,
    pub app_default_num_ctx: u64,
}

pub fn parse_model_info(model: &str, show: &serde_json::Value) -> LocalModelInfo {
    let details = show.get("details").cloned().unwrap_or(serde_json::Value::Null);
    let text = |key: &str| details.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut parameters = std::collections::BTreeMap::new();
    if let Some(raw) = show.get("parameters").and_then(|v| v.as_str()) {
        for line in raw.lines() {
            let mut parts = line.trim().splitn(2, char::is_whitespace);
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                let value = value.trim().trim_matches('"').to_string();
                parameters
                    .entry(key.to_string())
                    .and_modify(|existing: &mut String| {
                        existing.push_str(", ");
                        existing.push_str(&value);
                    })
                    .or_insert(value);
            }
        }
    }
    let context_length = show
        .get("model_info")
        .and_then(|v| v.as_object())
        .and_then(|info| info.iter().find(|(k, _)| k.ends_with(".context_length")).and_then(|(_, v)| v.as_u64()));
    LocalModelInfo {
        model: model.to_string(),
        family: text("family"),
        parameter_size: text("parameter_size"),
        quantization: text("quantization_level"),
        context_length,
        parameters,
        app_default_num_ctx: OLLAMA_NUM_CTX,
    }
}

pub fn local_model_info(host: &str, model: &str) -> Result<LocalModelInfo, String> {
    let host = normalize_host(host);
    let client = http_client(15)?;
    let response = client
        .post(format!("{host}/api/show"))
        .json(&serde_json::json!({ "model": model }))
        .send()
        .map_err(|e| format!("Ollama /api/show failed ({host}): {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Ollama has no model '{model}' (HTTP {}). Pull it first.", response.status()));
    }
    let show = response.json::<serde_json::Value>().map_err(|e| format!("Unreadable /api/show reply: {e}"))?;
    Ok(parse_model_info(model, &show))
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaChatResult {
    pub content: String,
    pub prompt_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

fn canonical_to_ollama_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .filter_map(|message| {
            let role = message.get("role").and_then(|v| v.as_str()).unwrap_or("user");
            let role = match role {
                "model" | "assistant" => "assistant",
                "system" => "system",
                _ => "user",
            };
            let content = if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
                text.to_string()
            } else if let Some(parts) = message.get("content").and_then(|v| v.as_array()) {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else if let Some(parts) = message.get("parts").and_then(|v| v.as_array()) {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                return None;
            };
            if content.trim().is_empty() {
                return None;
            }
            Some(serde_json::json!({ "role": role, "content": content }))
        })
        .collect()
}

/// Context window requested from Ollama; large enough for the master-axiom
/// primer plus retrieval packet. Ollama caps it to what the model supports.
const OLLAMA_NUM_CTX: u64 = 16_384;

/// Default silence limit for a local reply. There is no total cap: a reply is only
/// abandoned when Ollama sends nothing for this long (prompt reading on CPU included).
pub const DEFAULT_IDLE_TIMEOUT_MINS: u32 = 60;
pub const MAX_IDLE_TIMEOUT_MINS: u32 = 1440;

/// Pane keys whose in-flight local reply should be dropped (closing the connection
/// makes Ollama stop generating).
static CHAT_CANCEL: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn request_chat_cancel(key: &str) {
    if let Ok(mut keys) = CHAT_CANCEL.lock() {
        if !keys.iter().any(|k| k == key) {
            keys.push(key.to_string());
        }
    }
}

fn take_chat_cancel(key: &str) -> bool {
    match CHAT_CANCEL.lock() {
        Ok(mut keys) => {
            let before = keys.len();
            keys.retain(|k| k != key);
            keys.len() != before
        }
        Err(_) => false,
    }
}

/// Snapshot sent to the UI about once a second while a local reply is in flight.
#[derive(Debug, Clone, Serialize)]
pub struct ChatProgress {
    /// "reading" until the first token arrives (prompt prefill), then "writing".
    pub phase: &'static str,
    pub elapsed_secs: u64,
    pub first_token_secs: Option<u64>,
    pub tokens: u64,
}

#[derive(Debug, Default)]
struct StreamAcc {
    content: String,
    chunks: u64,
    prompt_tokens: Option<u64>,
    output_tokens: Option<u64>,
    done: bool,
}

/// Applies one NDJSON line of Ollama's streaming /api/chat response.
fn apply_stream_line(acc: &mut StreamAcc, line: &str) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("Ollama sent malformed stream data: {e}"))?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Err(format!("Ollama error: {err}"));
    }
    if let Some(piece) = v.pointer("/message/content").and_then(|c| c.as_str()) {
        if !piece.is_empty() {
            acc.content.push_str(piece);
            acc.chunks += 1;
        }
    }
    if v.get("done").and_then(|d| d.as_bool()) == Some(true) {
        acc.done = true;
        acc.prompt_tokens = v.get("prompt_eval_count").and_then(|n| n.as_u64());
        acc.output_tokens = v.get("eval_count").and_then(|n| n.as_u64());
    }
    Ok(())
}

/// reqwest's Display drops the cause ("error sending request for url ..."), so name
/// the common cases and append the source chain for the rest.
fn describe_chat_error(host: &str, idle_secs: u64, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return format!(
            "Local AI request stopped: Ollama sent nothing for {} min ({host}). For very slow prompts raise Settings ▸ AI Models ▸ Local AI silence limit, or use a smaller model.",
            idle_secs / 60
        );
    }
    if e.is_connect() {
        return format!("Local AI request failed ({host}): could not connect. Is Ollama running?");
    }
    let mut detail = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(cause) = source {
        detail.push_str(": ");
        detail.push_str(&cause.to_string());
        source = cause.source();
    }
    format!("Local AI request failed ({host}): {detail}. Ollama may have stopped or crashed mid-reply; check `journalctl -u ollama`.")
}

async fn stream_chat(
    client: reqwest::Client,
    host: String,
    body: serde_json::Value,
    idle_secs: u64,
    acc: Arc<Mutex<StreamAcc>>,
) -> Result<(), String> {
    let mut response = client
        .post(format!("{host}/api/chat"))
        .json(&body)
        .send()
        .await
        .map_err(|e| describe_chat_error(&host, idle_secs, &e))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| text.trim().to_string());
        return Err(format!("Ollama returned HTTP {status}: {detail}"));
    }
    let mut pending: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| describe_chat_error(&host, idle_secs, &e))?
    {
        pending.extend_from_slice(&chunk);
        while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = pending.drain(..=pos).collect();
            let mut guard = acc.lock().map_err(|_| "Local AI stream state poisoned".to_string())?;
            apply_stream_line(&mut guard, &String::from_utf8_lossy(&line))?;
        }
    }
    let mut guard = acc.lock().map_err(|_| "Local AI stream state poisoned".to_string())?;
    apply_stream_line(&mut guard, &String::from_utf8_lossy(&pending))?;
    if !guard.done {
        return Err(format!("Local AI request failed ({host}): Ollama closed the stream before finishing the reply."));
    }
    Ok(())
}

/// Streams one local reply. `cancel_key` (the pane) lets `request_chat_cancel` abort it;
/// `on_progress` is called about once a second until the reply completes.
pub fn chat(
    host: &str,
    model: &str,
    messages: &[serde_json::Value],
    idle_timeout_mins: u32,
    options: &OllamaOptions,
    cancel_key: &str,
    mut on_progress: impl FnMut(&ChatProgress),
) -> Result<OllamaChatResult, String> {
    let host = normalize_host(host);
    let idle_secs = u64::from(idle_timeout_mins.clamp(1, MAX_IDLE_TIMEOUT_MINS)) * 60;
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(std::time::Duration::from_secs(idle_secs))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let mut body = serde_json::json!({
        "model": model,
        "messages": canonical_to_ollama_messages(messages),
        "stream": true,
        "options": options.request_options()
    });
    if let Some(keep_alive) = options.keep_alive() {
        body["keep_alive"] = serde_json::json!(keep_alive);
    }
    take_chat_cancel(cancel_key);
    let acc = Arc::new(Mutex::new(StreamAcc::default()));
    let (tx, rx) = std::sync::mpsc::channel();
    let task = tauri::async_runtime::spawn(stream_chat_task(client, host.clone(), body, idle_secs, acc.clone(), tx));
    let started = std::time::Instant::now();
    let mut first_token_secs = None;
    let outcome = loop {
        match rx.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(result) => break result,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Local AI stream task ended unexpectedly".to_string())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        if take_chat_cancel(cancel_key) {
            task.abort();
            break Err("Local AI request stopped by user.".to_string());
        }
        let tokens = acc.lock().map(|a| a.chunks).unwrap_or(0);
        let elapsed_secs = started.elapsed().as_secs();
        if tokens > 0 && first_token_secs.is_none() {
            first_token_secs = Some(elapsed_secs);
        }
        on_progress(&ChatProgress {
            phase: if tokens == 0 { "reading" } else { "writing" },
            elapsed_secs,
            first_token_secs,
            tokens,
        });
    };
    outcome?;
    let mut guard = acc.lock().map_err(|_| "Local AI stream state poisoned".to_string())?;
    let content = std::mem::take(&mut guard.content);
    if content.trim().is_empty() {
        return Err("Ollama returned an empty reply".to_string());
    }
    Ok(OllamaChatResult {
        content,
        prompt_tokens: guard.prompt_tokens,
        output_tokens: guard.output_tokens.or(Some(guard.chunks)),
    })
}

async fn stream_chat_task(
    client: reqwest::Client,
    host: String,
    body: serde_json::Value,
    idle_secs: u64,
    acc: Arc<Mutex<StreamAcc>>,
    tx: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let _ = tx.send(stream_chat(client, host, body, idle_secs, acc).await);
}

#[cfg(test)]
mod tests {
    #[test]
    fn ollama_options_only_send_overrides() {
        let defaults = OllamaOptions::default().request_options();
        assert_eq!(defaults, serde_json::json!({ "num_ctx": OLLAMA_NUM_CTX }));
        let custom = OllamaOptions { num_ctx: Some(8192), temperature: Some(0.2), keep_alive: Some(" 30m ".into()), ..Default::default() };
        assert_eq!(custom.request_options(), serde_json::json!({ "num_ctx": 8192, "temperature": 0.2 }));
        assert_eq!(custom.keep_alive(), Some("30m"));
        assert_eq!(OllamaOptions { keep_alive: Some("  ".into()), ..Default::default() }.keep_alive(), None);
    }

    #[test]
    fn model_info_parses_show_reply() {
        let show = serde_json::json!({
            "details": { "family": "qwen2", "parameter_size": "7.6B", "quantization_level": "Q4_K_M" },
            "parameters": "num_ctx                        32768\nstop                           \"<|im_start|>\"\nstop                           \"<|im_end|>\"\ntemperature                    0.7",
            "model_info": { "general.architecture": "qwen2", "qwen2.context_length": 32768 }
        });
        let info = parse_model_info("qwen2.5-coder:7b", &show);
        assert_eq!(info.context_length, Some(32768));
        assert_eq!(info.parameters["num_ctx"], "32768");
        assert_eq!(info.parameters["temperature"], "0.7");
        assert_eq!(info.parameters["stop"], "<|im_start|>, <|im_end|>");
        assert_eq!(info.quantization, "Q4_K_M");
    }

    use super::*;

    #[test]
    fn stream_lines_accumulate_reply_and_counts() {
        let mut acc = StreamAcc::default();
        apply_stream_line(&mut acc, r#"{"message":{"role":"assistant","content":"Hel"},"done":false}"#).unwrap();
        apply_stream_line(&mut acc, "\n").unwrap();
        apply_stream_line(&mut acc, r#"{"message":{"role":"assistant","content":"lo"},"done":false}"#).unwrap();
        apply_stream_line(&mut acc, r#"{"message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":10428,"eval_count":2}"#).unwrap();
        assert_eq!(acc.content, "Hello");
        assert_eq!(acc.chunks, 2);
        assert!(acc.done);
        assert_eq!((acc.prompt_tokens, acc.output_tokens), (Some(10428), Some(2)));
        let err = apply_stream_line(&mut acc, r#"{"error":"model runner has unexpectedly stopped"}"#).unwrap_err();
        assert!(err.contains("unexpectedly stopped"), "{err}");
    }

    #[test]
    fn cancel_flag_is_per_pane_and_consumed() {
        request_chat_cancel("left");
        assert!(!take_chat_cancel("right"));
        assert!(take_chat_cancel("left"));
        assert!(!take_chat_cancel("left"));
    }

    #[test]
    fn quantization_rewrites_names_and_sizes() {
        let cat = curated_model_catalog();
        let qwen = cat.iter().find(|m| m.name == "qwen2.5:7b").unwrap();
        let q4 = apply_quantization(qwen, "q4_0").unwrap();
        assert_eq!(q4.name, "qwen2.5:7b-instruct-q4_0");
        assert!(q4.file_gb < qwen.file_gb);
        let q8 = apply_quantization(qwen, "q8_0").unwrap();
        assert_eq!(q8.name, "qwen2.5:7b-instruct-q8_0");
        assert!((q8.file_gb - 8.1).abs() < 0.3);
        let phi = cat.iter().find(|m| m.name == "phi4:14b").unwrap();
        assert_eq!(apply_quantization(phi, "q5_K_M").unwrap().name, "phi4:14b-q5_K_M");
        let r1 = cat.iter().find(|m| m.name == "deepseek-r1:8b").unwrap();
        assert!(apply_quantization(r1, "q4_0").is_none());
        assert!(apply_quantization(qwen, "q9_9").is_none());
    }

    fn hw(ram: f64, vram: f64, disk_total: f64, disk_free: f64) -> HardwareSpecs {
        HardwareSpecs {
            cpu_cores: 8,
            ram_total_gb: ram,
            ram_available_gb: ram * 0.7,
            vram_total_gb: vram,
            disk_total_gb: disk_total,
            disk_free_gb: disk_free,
            ..HardwareSpecs::default()
        }
    }

    fn spec(name: &str) -> LocalModelSpec {
        curated_model_catalog().into_iter().find(|m| m.name == name).expect("model in catalog")
    }

    #[test]
    fn small_model_on_gpu_is_quick() {
        let est = estimate_performance(&spec("llama3.1:8b"), &hw(32.0, 12.0, 500.0, 300.0));
        assert_eq!(est.runs_on, "gpu");
        assert_eq!(est.verdict, "quick");
        assert!(est.est_response_seconds < 10.0, "{est:?}");
    }

    #[test]
    fn large_model_on_cpu_is_unusable_by_time() {
        let est = estimate_performance(&spec("llama3.3:70b"), &hw(128.0, 0.0, 2000.0, 1500.0));
        assert_eq!(est.runs_on, "cpu");
        assert_eq!(est.verdict, "unusable");
        assert!(est.est_response_seconds > MAX_ACCEPTABLE_SECONDS);
    }

    #[test]
    fn mid_size_model_on_laptop_cpu_is_deep_not_unusable() {
        let mut laptop = hw(16.0, 0.0, 500.0, 300.0);
        laptop.cpu_cores = 16;
        let est = estimate_performance(&spec("qwen2.5-coder:7b"), &laptop);
        assert_eq!(est.runs_on, "cpu");
        assert_eq!(est.verdict, "deep", "{est:?}");
        let small = estimate_performance(&spec("qwen2.5:3b"), &laptop);
        assert_ne!(small.verdict, "unusable", "{small:?}");
    }

    #[test]
    fn model_that_does_not_fit_memory_is_unusable() {
        let est = estimate_performance(&spec("qwen2.5:72b"), &hw(16.0, 8.0, 500.0, 300.0));
        assert_eq!(est.verdict, "unusable");
        assert_eq!(est.runs_on, "none");
    }

    #[test]
    fn disk_nearly_full_after_install_is_unusable() {
        let est = estimate_performance(&spec("llama3.1:8b"), &hw(32.0, 12.0, 100.0, 7.0));
        assert!(!est.disk_ok);
        assert_eq!(est.verdict, "unusable");
        assert!(est.reasons.iter().any(|r| r.contains("95%")));
    }

    #[test]
    fn insufficient_disk_is_unusable() {
        let est = estimate_performance(&spec("qwen2.5:32b"), &hw(64.0, 24.0, 1000.0, 10.0));
        assert!(!est.disk_ok);
        assert_eq!(est.verdict, "unusable");
    }

    #[test]
    fn spectrum_position_grows_with_response_time() {
        let fast = estimate_performance(&spec("llama3.2:3b"), &hw(32.0, 12.0, 500.0, 300.0));
        let slow = estimate_performance(&spec("qwen2.5:14b"), &hw(32.0, 0.0, 500.0, 300.0));
        assert!(fast.spectrum_position < slow.spectrum_position);
        assert!((0.0..=1.0).contains(&slow.spectrum_position));
    }

    #[test]
    fn model_matching_ignores_latest_tag_and_case() {
        assert!(model_matches("llama3.1", "llama3.1:latest"));
        assert!(model_matches("Qwen2.5:7b", "qwen2.5:7b"));
        assert!(!model_matches("qwen2.5:7b", "qwen2.5:14b"));
    }

    #[test]
    fn install_script_exports_models_dir_when_set() {
        let script = build_install_script_for("qwen2.5:7b", true, false, "/mnt/models/ollama");
        assert!(script.contains("export OLLAMA_MODELS='/mnt/models/ollama'"));
        assert!(script.contains("mkdir -p '/mnt/models/ollama'"));
        assert!(script.contains("pkill -f 'ollama serve'"));
        assert!(script.contains("systemctl edit ollama"));
        let plain = build_install_script_for("qwen2.5:7b", true, false, "");
        assert!(!plain.contains("OLLAMA_MODELS"));
        let mac = build_install_script_for("qwen2.5:7b", true, true, "/Volumes/Models");
        assert!(mac.contains("brew services stop ollama"));
    }

    #[test]
    fn models_dir_normalization() {
        assert_eq!(normalize_models_dir("  ").unwrap(), "");
        assert!(normalize_models_dir("relative/dir").is_err());
        assert!(normalize_models_dir("/definitely/not/here/xyz").is_err());
        let tmp = std::env::temp_dir();
        let ok = normalize_models_dir(&format!(" {} ", tmp.display())).unwrap();
        assert_eq!(ok, tmp.to_string_lossy());
        assert_eq!(ollama_models_dir_with(&ok), tmp);
    }

    #[test]
    fn install_script_includes_installer_only_when_missing() {
        let with = build_install_script_with_dir("llama3.1:8b", false, "");
        assert!(with.contains("ollama.com/install.sh"));
        assert!(with.contains("ollama pull 'llama3.1:8b'"));
        let without = build_install_script_with_dir("llama3.1:8b", true, "");
        assert!(!without.contains("install.sh"));
    }

    #[test]
    fn terminal_command_shapes() {
        let (app, args) = build_linux_terminal_command("gnome-terminal", "echo hi");
        assert_eq!(app, "gnome-terminal");
        assert_eq!(args[0], "--");
        let (_, args) = build_linux_terminal_command("xterm", "echo hi");
        assert_eq!(args[0], "-e");
        assert_eq!(args[1..3], ["bash".to_string(), "-c".to_string()]);
    }

    #[test]
    fn canonical_messages_convert_gemini_and_openai_shapes() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "primer"}),
            serde_json::json!({"role": "user", "parts": [{"text": "hello"}]}),
            serde_json::json!({"role": "model", "content": [{"type": "text", "text": "hi"}]}),
        ];
        let converted = canonical_to_ollama_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[1]["content"], "hello");
        assert_eq!(converted[2]["role"], "assistant");
    }

    #[test]
    fn parses_installed_models_from_tags() {
        let tags = serde_json::json!({"models": [{"name": "qwen2.5:7b", "size": 4_700_000_000u64,
            "details": {"quantization_level": "Q4_K_M", "parameter_size": "7.6B", "family": "qwen2"}}]});
        let models = parse_installed_models(&tags);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].quant, "Q4_K_M");
        assert!(models[0].size_gb > 4.0);
    }

    #[test]
    fn df_k_parses_linux_and_macos_layouts() {
        let linux = "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/nvme0n1p2 1000000 400000 600000 40% /\n";
        let (total, free) = parse_df_k(linux).unwrap();
        assert!((total - kb_to_gb(1_000_000.0)).abs() < 1e-9);
        assert!((free - kb_to_gb(600_000.0)).abs() < 1e-9);
        let mac = "Filesystem   1024-blocks      Used Available Capacity iused ifree %iused  Mounted on\n/dev/disk3s5   971350180 300000000 500000000    38%  1000 2000    0%   /System/Volumes/Data\n";
        let (total, free) = parse_df_k(mac).unwrap();
        assert!((total - kb_to_gb(971_350_180.0)).abs() < 1e-9);
        assert!((free - kb_to_gb(500_000_000.0)).abs() < 1e-9);
    }

    #[test]
    fn apple_bandwidth_tiers_increase() {
        assert!(apple_unified_bandwidth_gbps("Apple M3") < apple_unified_bandwidth_gbps("Apple M3 Pro"));
        assert!(apple_unified_bandwidth_gbps("Apple M3 Pro") < apple_unified_bandwidth_gbps("Apple M3 Max"));
        assert!(apple_unified_bandwidth_gbps("Apple M3 Max") < apple_unified_bandwidth_gbps("Apple M2 Ultra"));
    }

    #[test]
    fn macos_install_script_uses_brew_not_linux_installer() {
        let script = build_install_script_for("qwen2.5:14b", false, true, "");
        assert!(script.contains("brew install ollama"));
        assert!(!script.contains("install.sh"));
        assert!(script.contains("ollama pull 'qwen2.5:14b'"));
        let linux = build_install_script_for("qwen2.5:14b", false, false, "");
        assert!(linux.contains("install.sh"));
    }
}
