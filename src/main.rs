//! MeshCore Ed25519 Vanity Key Generator
//!
//! High-performance key generator with CPU multi-threading and GPU support.
//! Generates Ed25519 keys compatible with MeshCore's specific format.

mod gpu_detect;
mod keygen;
#[cfg(target_os = "macos")]
mod metal_gpu;
mod pattern;
mod worker;

use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::keygen::KeyInfo;
use crate::pattern::{PatternConfig, PatternMode};
use crate::worker::WorkerPool;

/// JSON output structure for a found key
#[derive(Serialize)]
struct KeyOutput {
    pub index: usize,
    pub public_key: String,
    pub private_key: String,
    pub node_id: String,
    pub first_8: String,
    pub last_8: String,
    pub meshcore_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_file: Option<String>,
}

/// JSON output structure for the summary
#[derive(Serialize)]
struct SummaryOutput {
    pub total_time_seconds: f64,
    pub total_attempts: u64,
    pub average_rate: f64,
    pub keys_found: usize,
    pub keys_valid: usize,
    pub keys: Vec<KeyOutput>,
}

/// MeshCore Ed25519 Vanity Key Generator
#[derive(Parser, Debug)]
#[command(name = "meshcore-keygen")]
#[command(about = "High-performance MeshCore Ed25519 vanity key generator")]
#[command(version)]
struct Args {
    /// Number of keys to find (stops after finding this many)
    #[arg(short = 'n', long, default_value = "1")]
    target_keys: usize,

    /// Number of worker threads (defaults to detected CPU cores)
    #[arg(short, long)]
    workers: Option<usize>,

    /// Enable Metal GPU acceleration (macOS only)
    #[arg(long, default_value = "false")]
    gpu: bool,

    /// Pattern mode: 2, 4, 6, or 8 character matching
    #[arg(long, value_parser = clap::value_parser!(u8).range(2..=8))]
    pattern: Option<u8>,

    /// Search for keys starting with this hex prefix
    #[arg(long)]
    prefix: Option<String>,

    /// Search for keys where first N chars match last N chars
    #[arg(long, value_parser = clap::value_parser!(u8).range(2..=8))]
    vanity: Option<u8>,

    /// Output directory for key files (default: current directory)
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Maximum time to run in seconds (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_time: u64,

    /// Disable MeshCore verification (checks prefix and ECDH). Verification is enabled by default; pass `--no-verify` to disable.
    #[arg(long = "no-verify", action = clap::ArgAction::SetTrue, default_value_t = false)]
    no_verify: bool,

    /// Skip keys that already exist in the output directory
    #[arg(long, default_value = "true")]
    skip_existing: bool,

    /// Output results as JSON instead of human-readable format
    #[arg(long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Use nearly all CPU cores (on macOS use total cores minus one). Overrides default core heuristics.
    #[arg(long, default_value_t = false)]
    brutal: bool,

    /// Power-saving mode: use only efficiency cores on macOS, or half cores on other platforms
    #[arg(long, default_value_t = false)]
    powersave: bool,

    /// Benchmark mode: measure speed without saving keys to disk
    #[arg(long, default_value_t = false)]
    benchmark: bool,

    /// Beautiful display mode: enhanced statistics with cleaner formatting
    #[arg(long, default_value_t = false)]
    beautiful: bool,

    /// Display refresh interval in milliseconds (default: 500ms for smoother display)
    #[arg(long, default_value = "500")]
    refresh_ms: u64,

    /// Run tests
    #[arg(long)]
    test: bool,
}

fn main() {
    let args = Args::parse();

    if args.test {
        run_tests();
        return;
    }

    // Prepare output directories
    let base_output = args.output.clone(); // root where timestamped runs will live

    // If user did not provide an explicit output (default '.'), create a timestamped subdirectory
    let output_dir: PathBuf = if base_output == Path::new(".") {
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
        let newdir = base_output.join(ts);
        fs::create_dir_all(&newdir).expect("Failed to create timestamped output directory");
        newdir
    } else {
        if !base_output.exists() {
            fs::create_dir_all(&base_output).expect("Failed to create output directory");
        }
        base_output.clone()
    };

    // Load existing keys to avoid duplicates. We scan the base output root (not the per-run subdir)
    let existing_keys = if args.skip_existing {
        load_existing_keys(&base_output)
    } else {
        HashSet::new()
    };

    // Compute effective verification flag (verification is ON by default)
    let verify = !args.no_verify;

    // Configure pattern matching
    let pattern_config = build_pattern_config(&args);

    if !args.json {
        println!(
            "{}",
            style("╔════════════════════════════════════════════════════════════╗").cyan()
        );
        println!(
            "{}",
            style("║     MeshCore Ed25519 Vanity Key Generator (Rust)           ║").cyan()
        );
        println!(
            "{}",
            style("╚════════════════════════════════════════════════════════════╝").cyan()
        );
        println!();

        // Detect system capabilities
        let cpu_cores = detect_cpu_cores(args.brutal, args.powersave);
        if args.brutal {
            println!(
                "{} Brutal mode: using nearly all available cores (leaving one free)",
                style("⚠").yellow()
            );
        }
        if args.powersave {
            println!(
                "{} Power-save mode: using efficiency cores only",
                style("🔋").green()
            );
        }
        if args.benchmark {
            println!(
                "{} Benchmark mode: keys will NOT be saved to disk",
                style("⚡").yellow()
            );
        }
        let worker_count = args.workers.unwrap_or(cpu_cores);

        println!(
            "{} Detected {} CPU cores, using {} workers",
            style("ℹ").blue(),
            cpu_cores,
            worker_count
        );
        println!(
            "{} Pattern: {}",
            style("ℹ").blue(),
            pattern_config.description()
        );
        println!("{} Target: {} key(s)", style("ℹ").blue(), args.target_keys);

        if verify {
            println!(
                "{} MeshCore verification: {}",
                style("ℹ").blue(),
                style("ENABLED").green()
            );
        }

        if !existing_keys.is_empty() {
            println!(
                "{} Loaded {} existing keys (will skip duplicates)",
                style("ℹ").blue(),
                existing_keys.len()
            );
        }

        #[cfg(target_os = "macos")]
        if args.gpu {
            println!(
                "{} Metal GPU acceleration: {}",
                style("ℹ").blue(),
                style("ENABLED").green()
            );
        }

        println!();
    }

    let cpu_cores = detect_cpu_cores(args.brutal, args.powersave);
    let worker_count = args.workers.unwrap_or(cpu_cores);

    // Shared state
    let found_count = Arc::new(AtomicU64::new(0));
    let total_attempts = Arc::new(AtomicU64::new(0));
    let should_stop = Arc::new(AtomicBool::new(false));

    // Progress display (only if not JSON mode)
    let progress_bar = if !args.json {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    let start_time = Instant::now();

    // Channel for found keys
    let (tx, rx) = crossbeam_channel::unbounded::<KeyInfo>();

    // Start worker pool
    let mut worker_pool = WorkerPool::new(
        worker_count,
        pattern_config.clone(),
        tx.clone(),
        total_attempts.clone(),
        should_stop.clone(),
    );

    #[cfg(target_os = "macos")]
    let gpu_counter = {
        if args.gpu {
            worker_pool.enable_gpu();
        }
        // Attach optional GPU counter and start workers
        let counter = Arc::new(AtomicU64::new(0));
        if args.gpu {
            worker_pool.set_gpu_attempts(counter.clone());
        }
        counter
    };

    worker_pool.start();

    // Snapshot per-worker counters for live stats
    let worker_counters = worker_pool.attempts_per_worker_snapshot();
    let mut prev_worker_totals: Vec<u64> = worker_counters
        .iter()
        .map(|c| c.load(Ordering::Relaxed))
        .collect();
    let mut prev_sample = Instant::now();
    // Per-core EMA (exponential moving average) to smooth instant rate spikes/zeros
    // Sliding window of recent instantaneous samples per core to compute a short-term average
    let window_size: usize = 6; // average over last 6 samples
    let mut per_core_windows: Vec<Vec<f64>> =
        vec![vec![0.0f64; window_size]; worker_counters.len()];
    let mut window_idx: usize = 0;

    // Collect found keys with their output info
    let mut found_keys: Vec<KeyOutput> = Vec::new();
    let mut known_keys: HashSet<String> = existing_keys;
    let target = args.target_keys;
    let max_time = if args.max_time > 0 {
        Some(Duration::from_secs(args.max_time))
    } else {
        None
    };

    loop {
        // Check for found keys
        while let Ok(key) = rx.try_recv() {
            // Check if this key already exists
            if known_keys.contains(&key.public_hex) {
                if args.verbose && !args.json {
                    eprintln!(
                        "{} Skipping duplicate key: {}",
                        style("⚠").yellow(),
                        &key.public_hex[..16]
                    );
                }
                continue;
            }

            // Verify key for MeshCore compatibility if requested
            let validation = if verify {
                keygen::validate_for_meshcore(&key)
            } else {
                keygen::ValidationResult {
                    valid: true,
                    reason: None,
                }
            };

            // Skip invalid keys if verification is enabled
            if verify && !validation.valid {
                if args.verbose && !args.json {
                    eprintln!(
                        "{} Skipping invalid key: {} - {}",
                        style("⚠").yellow(),
                        &key.public_hex[..16],
                        validation.reason.as_deref().unwrap_or("unknown")
                    );
                }
                continue;
            }

            found_count.fetch_add(1, Ordering::Relaxed);
            let count = found_count.load(Ordering::Relaxed) as usize;

            // Mark this key as known
            known_keys.insert(key.public_hex.clone());

            // Save the key (skip in benchmark mode)
            let saved = if args.benchmark {
                None
            } else {
                save_key(&key, &output_dir, count, args.prefix.as_deref())
            };

            // Create output record
            let key_output = KeyOutput {
                index: count,
                public_key: key.public_hex.clone(),
                private_key: key.private_hex.clone(),
                node_id: key.public_hex[..2].to_string(),
                first_8: key.public_hex[..8].to_string(),
                last_8: key.public_hex[56..].to_string(),
                meshcore_valid: validation.valid,
                validation_error: validation.reason.clone(),
                public_file: saved.as_ref().map(|(p, _)| p.clone()),
                private_file: saved.as_ref().map(|(_, p)| p.clone()),
            };

            if !args.json {
                if let Some(ref pb) = progress_bar {
                    pb.suspend(|| {
                        println!();
                        println!(
                            "{}",
                            style("════════════════════════════════════════════════════════════")
                                .green()
                        );
                        println!(
                            "{} Found matching key #{}",
                            style("✓").green().bold(),
                            count
                        );
                        println!(
                            "{}",
                            style("════════════════════════════════════════════════════════════")
                                .green()
                        );
                        println!("  Public Key:  {}", style(&key.public_hex).yellow());
                        println!("  First 8:     {}", style(&key.public_hex[..8]).cyan());
                        println!("  Last 8:      {}", style(&key.public_hex[56..]).cyan());
                        println!("  Node ID:     {}", style(&key.public_hex[..2]).magenta());
                        if verify {
                            if validation.valid {
                                println!("  MeshCore:    {}", style("✓ Valid").green());
                            } else {
                                println!(
                                    "  MeshCore:    {} {}",
                                    style("✗ Invalid").red(),
                                    validation.reason.as_deref().unwrap_or("")
                                );
                            }
                        }
                        if let Some((pub_path, priv_path)) = &saved {
                            println!("  Saved to:");
                            println!("    Public:  {}", style(pub_path).dim());
                            println!("    Private: {}", style(priv_path).dim());
                        }
                        println!();
                    });
                }
            }

            found_keys.push(key_output);

            if found_keys.len() >= target {
                should_stop.store(true, Ordering::Relaxed);
            }
        }

        // Update progress
        let attempts = total_attempts.load(Ordering::Relaxed);
        let elapsed = start_time.elapsed();
        let _rate = if elapsed.as_secs_f64() > 0.0 {
            attempts as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        // Sample per-worker and GPU instantaneous rates
        let now_sample = Instant::now();
        let dt = now_sample
            .duration_since(prev_sample)
            .as_secs_f64()
            .max(1e-6);
        let mut per_core_rates: Vec<f64> = Vec::with_capacity(worker_counters.len());
        for (i, c) in worker_counters.iter().enumerate() {
            let cur = c.load(Ordering::Relaxed);
            let delta = cur.saturating_sub(prev_worker_totals[i]);
            let inst = delta as f64 / dt;
            // push into circular window and compute average
            per_core_windows[i][window_idx] = inst;
            let sum: f64 = per_core_windows[i].iter().sum();
            let avg = sum / (window_size as f64);
            per_core_rates.push(avg);
            prev_worker_totals[i] = cur;
        }
        window_idx = (window_idx + 1) % window_size;
        prev_sample = now_sample;

        // GPU rate
        let gpu_rate = {
            #[cfg(target_os = "macos")]
            {
                if args.gpu {
                    gpu_counter.load(Ordering::Relaxed) as f64 / elapsed.as_secs_f64().max(1e-6)
                } else {
                    0.0
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                0.0
            }
        };

        // Total instantaneous rate approximate (sum per-core + gpu)
        let total_inst_rate: f64 = per_core_rates.iter().sum::<f64>() + gpu_rate;

        // Estimate probability/time to finish
        let prob_per_attempt = pattern_config.estimated_probability();
        let remaining = if target > found_keys.len() {
            target - found_keys.len()
        } else {
            0
        };
        let eta_seconds = if prob_per_attempt > 0.0 && total_inst_rate > 0.0 {
            let attempts_per_key = 1.0 / prob_per_attempt;
            let expected_attempts = attempts_per_key * (remaining as f64);
            expected_attempts / total_inst_rate
        } else {
            f64::INFINITY
        };

        // Format per-core rates into short fixed-width colored string using compact notation
        let total_physical = num_cpus::get();
        let perf_count = detect_perf_cores_count();
        let efficiency_count = total_physical.saturating_sub(perf_count);

        let per_core_str = per_core_rates
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let label = format!("c{:02}:", i + 1);
                let val = format_compact_f64(*r);
                // pad to fixed width for alignment
                let padded = format!("{:>6}", val);
                let label_s = format!("{}", style(label).cyan());
                let count_s = if args.brutal {
                    // color perf cores red, efficiency green
                    if i >= efficiency_count {
                        format!("{}", style(padded).red())
                    } else {
                        format!("{}", style(padded).green())
                    }
                } else {
                    format!("{}", style(padded).green())
                };
                format!("{}{}", label_s, count_s)
            })
            .collect::<Vec<_>>()
            .join(" ");

        if let Some(ref pb) = progress_bar {
            let eta_display = if eta_seconds.is_finite() {
                let et =
                    chrono::Local::now() + chrono::Duration::seconds(eta_seconds.round() as i64);
                format!("ETA {}", et.format("%Y-%m-%d %H:%M:%S"))
            } else {
                "ETA ∞".to_string()
            };

            let attempts_s = format_compact_u64(attempts);
            let rate_s = format_compact_f64(total_inst_rate);
            let found_s = format_compact_u64(found_count.load(Ordering::Relaxed));
            let target_s = format_compact_u64(target as u64);

            if args.beautiful {
                // Beautiful mode: cleaner multi-line style statistics
                let progress_pct = if target > 0 {
                    (found_count.load(Ordering::Relaxed) as f64 / target as f64 * 100.0).min(100.0)
                } else {
                    0.0
                };

                // Show aggregate CPU rate and GPU rate separately
                let cpu_rate: f64 = per_core_rates.iter().sum();

                let mode_str = if args.benchmark {
                    format!("{}", style("[BENCHMARK]").yellow())
                } else if args.powersave {
                    format!("{}", style("[POWERSAVE]").green())
                } else if args.brutal {
                    format!("{}", style("[BRUTAL]").red())
                } else {
                    "".to_string()
                };

                pb.set_message(format!(
                    "{mode} {attempts:>10} attempts │ {rate:>8}/s │ Progress: {found}/{target} ({pct:>5.1}%) │ CPU:{cpu:>8}/s GPU:{gpu:>8}/s │ {eta}",
                    mode = mode_str,
                    attempts = attempts_s,
                    rate = rate_s,
                    found = found_s,
                    target = target_s,
                    pct = progress_pct,
                    cpu = format_compact_f64(cpu_rate),
                    gpu = format_compact_f64(gpu_rate),
                    eta = eta_display,
                ));
            } else {
                pb.set_message(format!(
                    "{attempts:>10} | Rate: {rate:>8}/s | Found: {found:>6}/{target:<6} | {eta} | GPU:{gpu:>8}/s | {cores}",
                    attempts = attempts_s,
                    rate = rate_s,
                    found = found_s,
                    target = target_s,
                    eta = eta_display,
                    gpu = format_compact_f64(gpu_rate),
                    cores = per_core_str
                ));
            }
        }

        // Check stop conditions
        if should_stop.load(Ordering::Relaxed) {
            break;
        }

        if let Some(max_dur) = max_time {
            if elapsed >= max_dur {
                if !args.json {
                    println!("\n{} Time limit reached", style("⏱").yellow());
                }
                should_stop.store(true, Ordering::Relaxed);
                break;
            }
        }

        // Use configurable refresh interval for smoother display
        std::thread::sleep(Duration::from_millis(args.refresh_ms.max(50)));
    }

    // Cleanup
    worker_pool.stop();
    if let Some(pb) = progress_bar {
        pb.finish_and_clear();
    }

    // Summary
    let elapsed = start_time.elapsed();
    let attempts = total_attempts.load(Ordering::Relaxed);
    let rate = if elapsed.as_secs_f64() > 0.0 {
        attempts as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    let valid_count = found_keys.iter().filter(|k| k.meshcore_valid).count();

    if args.json {
        // Output JSON
        let summary = SummaryOutput {
            total_time_seconds: elapsed.as_secs_f64(),
            total_attempts: attempts,
            average_rate: rate,
            keys_found: found_keys.len(),
            keys_valid: valid_count,
            keys: found_keys,
        };
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        println!();
        println!(
            "{}",
            style("═══════════════════════════════════════════════════════════").cyan()
        );
        println!(
            "{}",
            style("                         SUMMARY                           ")
                .cyan()
                .bold()
        );
        println!(
            "{}",
            style("═══════════════════════════════════════════════════════════").cyan()
        );
        println!("  Total Time:      {:.2}s", elapsed.as_secs_f64());
        println!("  Total Attempts:  {}", format_number(attempts));
        println!("  Average Rate:    {:.0} keys/sec", rate);
        println!("  Keys Found:      {}", found_keys.len());
        if verify {
            println!("  Keys Valid:      {} (MeshCore compatible)", valid_count);
        }
        println!();
    }
}

fn build_pattern_config(args: &Args) -> PatternConfig {
    let mut config = PatternConfig::default();

    if let Some(prefix) = &args.prefix {
        config.mode = PatternMode::Prefix;
        config.prefix = Some(prefix.to_uppercase());
    }

    if let Some(vanity) = args.vanity {
        config.mode = PatternMode::Vanity;
        config.vanity_length = vanity;
    }

    if let Some(pattern) = args.pattern {
        config.mode = PatternMode::Pattern;
        config.vanity_length = pattern;
    }

    // Combine prefix + vanity if both specified
    if args.prefix.is_some() && (args.vanity.is_some() || args.pattern.is_some()) {
        config.mode = PatternMode::PrefixVanity;
    }

    config
}

fn detect_cpu_cores(brutal: bool, powersave: bool) -> usize {
    #[cfg(target_os = "macos")]
    {
        // brutal takes precedence over powersave
        if brutal {
            // Use almost all cores but leave one free for responsiveness
            let cores = num_cpus::get();
            return std::cmp::max(1, cores.saturating_sub(1));
        }

        if powersave {
            // Use only efficiency cores on macOS
            if let Ok(output) = std::process::Command::new("sysctl")
                .args(["-n", "hw.perflevel1.physicalcpu"])
                .output()
            {
                if let Ok(cores) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<usize>()
                {
                    if cores > 0 {
                        return cores;
                    }
                }
            }
            // Fallback: use half of total cores
            let cores = num_cpus::get();
            return std::cmp::max(1, cores / 2);
        }

        // Try to detect performance cores on Apple Silicon
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.perflevel0.physicalcpu"])
            .output()
        {
            if let Ok(cores) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<usize>()
            {
                return cores;
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // brutal takes precedence over powersave
        if brutal {
            let cores = num_cpus::get();
            return std::cmp::max(1, cores.saturating_sub(1));
        }

        if powersave {
            // Use half of total cores on non-macOS
            let cores = num_cpus::get();
            return std::cmp::max(1, cores / 2);
        }
    }

    // Fallback to num_cpus - 75% of cores like the Python version
    let cores = num_cpus::get();
    std::cmp::max(2, (cores as f64 * 0.75).round() as usize)
}

fn detect_perf_cores_count() -> usize {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.perflevel0.physicalcpu"])
            .output()
        {
            if let Ok(cores) = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<usize>()
            {
                return cores;
            }
        }
        0
    }
    #[cfg(not(target_os = "macos"))]
    {
        0
    }
}

/// Load existing public keys from the output directory to avoid duplicates
fn load_existing_keys(output_dir: &PathBuf) -> HashSet<String> {
    let mut keys = HashSet::new();

    // Recursively scan the provided directory for any files ending with `_public.txt`.
    fn scan_dir(dir: &PathBuf, keys: &mut HashSet<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan_dir(&path, keys);
                    continue;
                }

                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("_public.txt") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            let key = content.trim().to_lowercase();
                            if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
                                keys.insert(key);
                            }
                        }
                    }
                }
            }
        }
    }

    scan_dir(output_dir, &mut keys);
    keys
}

fn save_key(
    key: &KeyInfo,
    output_dir: &Path,
    index: usize,
    filename_prefix: Option<&str>,
) -> Option<(String, String)> {
    // If a user-supplied prefix is provided, prefer it as the filename prefix (uppercased).
    // Otherwise fall back to the first 8 hex chars of the public key.
    let pattern_id = if let Some(p) = filename_prefix {
        p.to_uppercase()
    } else {
        key.public_hex[..8].to_uppercase()
    };
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");

    // Use a concise filename: <prefix>_<index>_<timestamp>_public|private.txt
    let pub_filename = format!("{}_{}_{}_public.txt", pattern_id, index, timestamp);
    let priv_filename = format!("{}_{}_{}_private.txt", pattern_id, index, timestamp);

    let pub_path = output_dir.join(&pub_filename);
    let priv_path = output_dir.join(&priv_filename);

    if let Err(e) = fs::write(&pub_path, &key.public_hex) {
        eprintln!("Failed to write public key: {}", e);
        return None;
    }

    if let Err(e) = fs::write(&priv_path, &key.private_hex) {
        eprintln!("Failed to write private key: {}", e);
        return None;
    }

    Some((pub_filename, priv_filename))
}

#[cfg(test)]
mod main_filename_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_key_with_prefix() {
        let dir = tempdir().unwrap();
        let out = dir.path().to_path_buf();

        // Build a dummy key
        let key = KeyInfo {
            public_hex: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                .to_string(),
            private_hex: "00".repeat(64),
            public_bytes: [0xAB; 32],
            private_bytes: [0x00; 64],
        };

        let prefix = Some("abcd");
        let saved = save_key(&key, &out, 3, prefix).expect("save_key failed");
        let pub_name = saved.0;
        assert!(
            pub_name.starts_with("ABCD_3_"),
            "pub filename didn't start with expected prefix: {}",
            pub_name
        );
    }

    #[test]
    fn test_load_existing_keys_recursive() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();

        // Create a timestamped subdirectory to simulate previous run
        let sub = base.join("20260101_000000");
        fs::create_dir_all(&sub).unwrap();

        // Write a public key file
        let pub_path = sub.join("SOME_1_20260101_000000_public.txt");
        let key_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        fs::write(&pub_path, key_hex).unwrap();

        let found = load_existing_keys(&base);
        assert!(
            found.contains(key_hex),
            "load_existing_keys did not find the key in subdir"
        );
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn test_format_compact_u64() {
        // Small numbers stay as-is
        assert_eq!(format_compact_u64(0), "0");
        assert_eq!(format_compact_u64(999), "999");

        // Thousands
        assert_eq!(format_compact_u64(1_000), "1.0k");
        assert_eq!(format_compact_u64(24_800), "24.8k");
        assert_eq!(format_compact_u64(999_999), "1000.0k");

        // Millions
        assert_eq!(format_compact_u64(1_000_000), "1.0M");
        assert_eq!(format_compact_u64(1_200_000), "1.2M");
        assert_eq!(format_compact_u64(999_999_999), "1000.0M");

        // Billions
        assert_eq!(format_compact_u64(1_000_000_000), "1.0B");
        assert_eq!(format_compact_u64(5_500_000_000), "5.5B");
    }

    #[test]
    fn test_format_compact_f64() {
        // Small numbers
        assert_eq!(format_compact_f64(0.0), "0");
        assert_eq!(format_compact_f64(999.0), "999");

        // Thousands
        assert_eq!(format_compact_f64(1_000.0), "1.0k");
        assert_eq!(format_compact_f64(24_800.0), "24.8k");

        // Millions
        assert_eq!(format_compact_f64(1_000_000.0), "1.0M");
        assert_eq!(format_compact_f64(1_200_000.0), "1.2M");

        // Billions
        assert_eq!(format_compact_f64(1_000_000_000.0), "1.0B");

        // Infinity
        assert_eq!(format_compact_f64(f64::INFINITY), "∞");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1_234), "1,234");
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_number(1_234_567_890), "1,234,567,890");
    }
}

#[cfg(test)]
mod cpu_detection_tests {
    use super::*;

    #[test]
    fn test_detect_cpu_cores_normal() {
        // Normal mode should return at least 1 core
        let cores = detect_cpu_cores(false, false);
        assert!(cores >= 1, "Should detect at least 1 core");
    }

    #[test]
    fn test_detect_cpu_cores_brutal() {
        // Brutal mode should return at least as many as normal
        let normal = detect_cpu_cores(false, false);
        let brutal = detect_cpu_cores(true, false);
        assert!(
            brutal >= normal,
            "Brutal mode should use at least as many cores as normal"
        );
    }

    #[test]
    fn test_detect_cpu_cores_powersave() {
        // Powersave should return at least 1 but less than or equal to normal
        let normal = detect_cpu_cores(false, false);
        let powersave = detect_cpu_cores(false, true);
        assert!(powersave >= 1, "Powersave should use at least 1 core");
        assert!(
            powersave <= normal,
            "Powersave should use at most as many cores as normal"
        );
    }

    #[test]
    fn test_powersave_and_brutal_conflict() {
        // When both are set, brutal takes precedence (uses all cores)
        let both = detect_cpu_cores(true, true);
        let brutal = detect_cpu_cores(true, false);
        // They should be roughly the same (brutal wins)
        assert!(both >= 1);
        assert_eq!(
            both, brutal,
            "When both flags set, brutal should take precedence"
        );
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Compact human-readable formatting: 24.8k, 1.2M, etc.
fn format_compact_u64(n: u64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;

    let f = n as f64;
    if f >= B {
        format!("{:.1}B", f / B)
    } else if f >= M {
        format!("{:.1}M", f / M)
    } else if f >= K {
        format!("{:.1}k", f / K)
    } else {
        format!("{}", n)
    }
}

fn format_compact_f64(n: f64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;

    if n.is_infinite() {
        return "∞".to_string();
    }

    if n >= B {
        format!("{:.1}B", n / B)
    } else if n >= M {
        format!("{:.1}M", n / M)
    } else if n >= K {
        format!("{:.1}k", n / K)
    } else {
        format!("{:.0}", n)
    }
}

fn run_tests() {
    println!("{}", style("Running tests...").cyan().bold());
    println!();

    // Test 1: Key generation
    print!("Test 1: Key generation... ");
    let key = keygen::generate_meshcore_keypair();
    assert_eq!(key.public_hex.len(), 64);
    assert_eq!(key.private_hex.len(), 128);
    println!("{}", style("PASS").green());

    // Test 2: Key verification
    print!("Test 2: Key verification... ");
    assert!(keygen::verify_key(&key));
    println!("{}", style("PASS").green());

    // Test 3: MeshCore validation
    print!("Test 3: MeshCore validation... ");
    let mut valid_count = 0;
    for _ in 0..100 {
        let key = keygen::generate_meshcore_keypair();
        let result = keygen::validate_for_meshcore(&key);
        if result.valid {
            valid_count += 1;
        }
    }
    // Most keys should be valid (only ~1.5% have 0x00 or 0xFF prefix)
    assert!(
        valid_count > 90,
        "Expected >90% valid keys, got {}",
        valid_count
    );
    println!("{}", style("PASS").green());

    // Test 4: Pattern matching - prefix
    print!("Test 4: Pattern matching (prefix)... ");
    let config = PatternConfig {
        mode: PatternMode::Prefix,
        prefix: Some("AB".to_string()),
        vanity_length: 8,
    };
    let test_hex = "AB1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12345678";
    assert!(pattern::matches_pattern(test_hex, &config));
    assert!(!pattern::matches_pattern(
        "CD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12345678",
        &config
    ));
    println!("{}", style("PASS").green());

    // Test 5: Pattern matching - vanity
    print!("Test 5: Pattern matching (vanity)... ");
    let config = PatternConfig {
        mode: PatternMode::Vanity,
        prefix: None,
        vanity_length: 4,
    };
    // First 4 == Last 4
    let test_hex = "ABCD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12ABCD";
    assert!(pattern::matches_pattern(test_hex, &config));
    println!("{}", style("PASS").green());

    // Test 6: Multiple key generation
    print!("Test 6: Multiple key generation... ");
    for _ in 0..100 {
        let key = keygen::generate_meshcore_keypair();
        assert!(keygen::verify_key(&key));
    }
    println!("{}", style("PASS").green());

    // Test 7: Key uniqueness
    print!("Test 7: Key uniqueness... ");
    let key1 = keygen::generate_meshcore_keypair();
    let key2 = keygen::generate_meshcore_keypair();
    assert_ne!(key1.public_hex, key2.public_hex);
    assert_ne!(key1.private_hex, key2.private_hex);
    println!("{}", style("PASS").green());

    // Test 8: Invalid prefix detection
    print!("Test 8: Invalid prefix detection... ");
    // A key with 0x00 prefix would be invalid
    assert!(keygen::is_valid_meshcore_prefix(&[0x01; 32]));
    assert!(!keygen::is_valid_meshcore_prefix(&[0x00; 32]));
    assert!(!keygen::is_valid_meshcore_prefix(&[0xFF; 32]));
    println!("{}", style("PASS").green());

    println!();
    println!("{}", style("All tests passed!").green().bold());
}
