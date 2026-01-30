//! MeshCore Ed25519 Vanity Key Generator
//!
//! High-performance key generator with CPU multi-threading and Metal GPU support.
//! Generates Ed25519 keys compatible with MeshCore's specific format.

mod keygen;
mod pattern;
mod worker;
#[cfg(target_os = "macos")]
mod metal_gpu;

use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs;

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

    /// Output directory for key files
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Maximum time to run in seconds (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_time: u64,

    /// Verify keys are valid for MeshCore (checks prefix and ECDH)
    #[arg(long, default_value = "true")]
    verify: bool,

    /// Skip keys that already exist in the output directory
    #[arg(long, default_value = "true")]
    skip_existing: bool,

    /// Output results as JSON instead of human-readable format
    #[arg(long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

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

    // Ensure output directory exists
    if !args.output.exists() {
        fs::create_dir_all(&args.output).expect("Failed to create output directory");
    }

    // Load existing keys to avoid duplicates
    let existing_keys = if args.skip_existing {
        load_existing_keys(&args.output)
    } else {
        HashSet::new()
    };

    // Configure pattern matching
    let pattern_config = build_pattern_config(&args);
    
    if !args.json {
        println!("{}", style("╔════════════════════════════════════════════════════════════╗").cyan());
        println!("{}", style("║     MeshCore Ed25519 Vanity Key Generator (Rust)           ║").cyan());
        println!("{}", style("╚════════════════════════════════════════════════════════════╝").cyan());
        println!();

        // Detect system capabilities
        let cpu_cores = detect_cpu_cores();
        let worker_count = args.workers.unwrap_or(cpu_cores);
        
        println!("{} Detected {} CPU cores, using {} workers", 
                 style("ℹ").blue(), cpu_cores, worker_count);
        println!("{} Pattern: {}", style("ℹ").blue(), pattern_config.description());
        println!("{} Target: {} key(s)", style("ℹ").blue(), args.target_keys);
        
        if args.verify {
            println!("{} MeshCore verification: {}", style("ℹ").blue(), style("ENABLED").green());
        }
        
        if !existing_keys.is_empty() {
            println!("{} Loaded {} existing keys (will skip duplicates)", 
                     style("ℹ").blue(), existing_keys.len());
        }
        
        #[cfg(target_os = "macos")]
        if args.gpu {
            println!("{} Metal GPU acceleration: {}", style("ℹ").blue(), style("ENABLED").green());
        }
        
        println!();
    }

    let cpu_cores = detect_cpu_cores();
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
                .unwrap()
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
    if args.gpu {
        worker_pool.enable_gpu();
    }
    
    worker_pool.start();
    
    // Collect found keys with their output info
    let mut found_keys: Vec<KeyOutput> = Vec::new();
    let mut known_keys: HashSet<String> = existing_keys;
    let target = args.target_keys;
    let max_time = if args.max_time > 0 { Some(Duration::from_secs(args.max_time)) } else { None };
    
    loop {
        // Check for found keys
        while let Ok(key) = rx.try_recv() {
            // Check if this key already exists
            if known_keys.contains(&key.public_hex) {
                if args.verbose && !args.json {
                    eprintln!("{} Skipping duplicate key: {}", style("⚠").yellow(), &key.public_hex[..16]);
                }
                continue;
            }
            
            // Verify key for MeshCore compatibility if requested
            let validation = if args.verify {
                keygen::validate_for_meshcore(&key)
            } else {
                keygen::ValidationResult { valid: true, reason: None }
            };
            
            // Skip invalid keys if verification is enabled
            if args.verify && !validation.valid {
                if args.verbose && !args.json {
                    eprintln!("{} Skipping invalid key: {} - {}", 
                             style("⚠").yellow(), 
                             &key.public_hex[..16],
                             validation.reason.as_deref().unwrap_or("unknown"));
                }
                continue;
            }
            
            found_count.fetch_add(1, Ordering::Relaxed);
            let count = found_count.load(Ordering::Relaxed) as usize;
            
            // Mark this key as known
            known_keys.insert(key.public_hex.clone());
            
            // Save the key
            let saved = save_key(&key, &args.output, count);
            
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
                        println!("{}", style("════════════════════════════════════════════════════════════").green());
                        println!("{} Found matching key #{}", style("✓").green().bold(), count);
                        println!("{}", style("════════════════════════════════════════════════════════════").green());
                        println!("  Public Key:  {}", style(&key.public_hex).yellow());
                        println!("  First 8:     {}", style(&key.public_hex[..8]).cyan());
                        println!("  Last 8:      {}", style(&key.public_hex[56..]).cyan());
                        println!("  Node ID:     {}", style(&key.public_hex[..2]).magenta());
                        if args.verify {
                            if validation.valid {
                                println!("  MeshCore:    {}", style("✓ Valid").green());
                            } else {
                                println!("  MeshCore:    {} {}", 
                                        style("✗ Invalid").red(),
                                        validation.reason.as_deref().unwrap_or(""));
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
        let rate = if elapsed.as_secs_f64() > 0.0 {
            attempts as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        
        if let Some(ref pb) = progress_bar {
            pb.set_message(format!(
                "Attempts: {} | Rate: {:.0} keys/sec | Found: {}/{}",
                format_number(attempts),
                rate,
                found_count.load(Ordering::Relaxed),
                target
            ));
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
        
        std::thread::sleep(Duration::from_millis(50));
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
        println!("{}", style("═══════════════════════════════════════════════════════════").cyan());
        println!("{}", style("                         SUMMARY                           ").cyan().bold());
        println!("{}", style("═══════════════════════════════════════════════════════════").cyan());
        println!("  Total Time:      {:.2}s", elapsed.as_secs_f64());
        println!("  Total Attempts:  {}", format_number(attempts));
        println!("  Average Rate:    {:.0} keys/sec", rate);
        println!("  Keys Found:      {}", found_keys.len());
        if args.verify {
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

fn detect_cpu_cores() -> usize {
    #[cfg(target_os = "macos")]
    {
        // Try to detect performance cores on Apple Silicon
        if let Ok(output) = std::process::Command::new("sysctl")
            .args(["-n", "hw.perflevel0.physicalcpu"])
            .output()
        {
            if let Ok(cores) = String::from_utf8_lossy(&output.stdout).trim().parse::<usize>() {
                return cores;
            }
        }
    }
    
    // Fallback to num_cpus
    let cores = num_cpus::get();
    // Use 75% of cores like the Python version
    std::cmp::max(2, (cores as f64 * 0.75).round() as usize)
}

/// Load existing public keys from the output directory to avoid duplicates
fn load_existing_keys(output_dir: &PathBuf) -> HashSet<String> {
    let mut keys = HashSet::new();
    
    if let Ok(entries) = fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Only look at public key files
                if name.starts_with("meshcore_") && name.contains("_public.txt") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let key = content.trim().to_lowercase();
                        // Validate it looks like a hex public key
                        if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
                            keys.insert(key);
                        }
                    }
                }
            }
        }
    }
    
    keys
}

fn save_key(key: &KeyInfo, output_dir: &PathBuf, index: usize) -> Option<(String, String)> {
    let pattern_id = &key.public_hex[..8].to_uppercase();
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    
    let pub_filename = format!("meshcore_{}_{}_{}_public.txt", pattern_id, index, timestamp);
    let priv_filename = format!("meshcore_{}_{}_{}_private.txt", pattern_id, index, timestamp);
    
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
    assert!(valid_count > 90, "Expected >90% valid keys, got {}", valid_count);
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
    assert!(!pattern::matches_pattern("CD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12345678", &config));
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
