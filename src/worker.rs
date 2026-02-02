//! Worker Pool for Parallel Key Generation
//!
//! Manages multiple worker threads that generate and check keys in parallel.
//! Supports both CPU and Metal GPU acceleration.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::Sender;

use crate::keygen::{self, KeyInfo};
use crate::pattern::{matches_pattern_bytes, PatternConfig};

#[cfg(target_os = "macos")]
use crate::metal_gpu;

/// Batch size for key generation (number of keys per batch)
const BATCH_SIZE: usize = 10_000;

/// Worker pool manages parallel key generation
pub struct WorkerPool {
    num_workers: usize,
    pattern_config: PatternConfig,
    result_sender: Sender<KeyInfo>,
    total_attempts: Arc<AtomicU64>,
    should_stop: Arc<AtomicBool>,
    worker_handles: Vec<JoinHandle<()>>,
    #[cfg(target_os = "macos")]
    gpu_enabled: bool,
    // Per-worker attempt counters for live stats
    attempts_per_worker: Vec<Arc<AtomicU64>>,
    // Optional GPU attempts counter
    #[cfg(target_os = "macos")]
    gpu_attempts: Option<Arc<AtomicU64>>,
}

impl WorkerPool {
    /// Create a new worker pool
    pub fn new(
        num_workers: usize,
        pattern_config: PatternConfig,
        result_sender: Sender<KeyInfo>,
        total_attempts: Arc<AtomicU64>,
        should_stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            num_workers,
            pattern_config,
            result_sender,
            total_attempts,
            should_stop,
            worker_handles: Vec::new(),
            #[cfg(target_os = "macos")]
            gpu_enabled: false,
            attempts_per_worker: (0..num_workers)
                .map(|_| Arc::new(AtomicU64::new(0)))
                .collect(),
            #[cfg(target_os = "macos")]
            gpu_attempts: None,
        }
    }

    /// Enable GPU acceleration (macOS only)
    #[cfg(target_os = "macos")]
    pub fn enable_gpu(&mut self) {
        self.gpu_enabled = true;
    }

    /// Attach a GPU attempts counter so the main thread can sample GPU throughput
    #[cfg(target_os = "macos")]
    pub fn set_gpu_attempts(&mut self, counter: Arc<AtomicU64>) {
        self.gpu_attempts = Some(counter);
    }

    /// Snapshot of per-worker attempt counters (cloned Arcs)
    pub fn attempts_per_worker_snapshot(&self) -> Vec<Arc<AtomicU64>> {
        self.attempts_per_worker.clone()
    }

    #[cfg(not(target_os = "macos"))]
    #[allow(dead_code)]
    pub fn enable_gpu(&mut self) {
        eprintln!("Warning: GPU acceleration is only available on macOS");
    }

    /// Start all worker threads
    pub fn start(&mut self) {
        #[cfg(target_os = "macos")]
        if self.gpu_enabled {
            self.start_gpu_worker();
        }

        for worker_id in 0..self.num_workers {
            let handle = self.spawn_cpu_worker(worker_id);
            self.worker_handles.push(handle);
        }
    }

    /// Spawn a CPU worker thread
    fn spawn_cpu_worker(&self, worker_id: usize) -> JoinHandle<()> {
        let pattern_config = self.pattern_config.clone();
        let result_sender = self.result_sender.clone();
        let total_attempts = self.total_attempts.clone();
        let should_stop = self.should_stop.clone();
        let worker_attempts = self.attempts_per_worker[worker_id].clone();

        thread::Builder::new()
            .name(format!("keygen-worker-{}", worker_id))
            .spawn(move || {
                cpu_worker_loop(
                    worker_id,
                    &pattern_config,
                    &result_sender,
                    &total_attempts,
                    &worker_attempts,
                    &should_stop,
                );
            })
            .expect("Failed to spawn worker thread")
    }

    /// Start GPU worker (macOS only)
    #[cfg(target_os = "macos")]
    fn start_gpu_worker(&mut self) {
        let pattern_config = self.pattern_config.clone();
        let result_sender = self.result_sender.clone();
        let total_attempts = self.total_attempts.clone();
        let should_stop = self.should_stop.clone();
        let gpu_counter = self.gpu_attempts.clone();

        let handle = thread::Builder::new()
            .name("keygen-gpu-worker".to_string())
            .spawn(move || {
                if let Err(e) = metal_gpu::gpu_worker_loop(
                    &pattern_config,
                    &result_sender,
                    &total_attempts,
                    gpu_counter,
                    &should_stop,
                ) {
                    eprintln!("GPU worker error: {}", e);
                }
            })
            .expect("Failed to spawn GPU worker thread");

        self.worker_handles.push(handle);
    }

    /// Stop all worker threads
    pub fn stop(&mut self) {
        self.should_stop.store(true, Ordering::Relaxed);

        // Wait for all workers to finish
        for handle in self.worker_handles.drain(..) {
            let _ = handle.join();
        }
    }
}

/// CPU worker loop - generates and checks keys continuously
fn cpu_worker_loop(
    _worker_id: usize,
    pattern_config: &PatternConfig,
    result_sender: &Sender<KeyInfo>,
    total_attempts: &AtomicU64,
    worker_attempts: &Arc<AtomicU64>,
    should_stop: &AtomicBool,
) {
    let mut local_attempts: u64 = 0;

    loop {
        // Check if we should stop
        if should_stop.load(Ordering::Relaxed) {
            break;
        }

        // Generate and check a batch of keys
        for _ in 0..BATCH_SIZE {
            let key = keygen::generate_meshcore_keypair();

            if matches_pattern_bytes(&key.public_bytes, pattern_config) {
                // Found a matching key!
                if result_sender.send(key).is_err() {
                    return; // Channel closed
                }
            }

            local_attempts += 1;
        }

        // Update global counter and per-worker counter periodically (reduces contention)
        total_attempts.fetch_add(local_attempts, Ordering::Relaxed);
        worker_attempts.fetch_add(local_attempts, Ordering::Relaxed);
        local_attempts = 0;

        // Check stop condition after each batch
        if should_stop.load(Ordering::Relaxed) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::PatternMode;
    use std::time::Duration;

    #[test]
    fn test_worker_pool_creation() {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let attempts = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let config = PatternConfig::default();
        let pool = WorkerPool::new(4, config, tx, attempts, stop);

        assert_eq!(pool.num_workers, 4);
    }

    #[test]
    fn test_worker_pool_generates_keys() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let attempts = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        // Use a very easy pattern (any 2-char match is common)
        let config = PatternConfig {
            mode: PatternMode::Vanity,
            prefix: None,
            vanity_length: 2,
        };

        let mut pool = WorkerPool::new(2, config, tx, attempts.clone(), stop.clone());
        pool.start();

        // Wait for at least one key to be found
        let result = rx.recv_timeout(Duration::from_secs(10));

        stop.store(true, Ordering::Relaxed);
        pool.stop();

        assert!(
            result.is_ok(),
            "Should find a key with 2-char vanity pattern"
        );
        assert!(
            attempts.load(Ordering::Relaxed) > 0,
            "Should have made attempts"
        );
    }

    #[test]
    fn test_worker_pool_stop() {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let attempts = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let config = PatternConfig::default();
        let mut pool = WorkerPool::new(2, config, tx, attempts, stop.clone());

        pool.start();

        // Let it run briefly
        thread::sleep(Duration::from_millis(100));

        // Stop should complete without hanging
        pool.stop();

        assert!(stop.load(Ordering::Relaxed));
    }
}
