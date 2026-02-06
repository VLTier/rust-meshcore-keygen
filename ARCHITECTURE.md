# Architecture

## Overview

This repository implements a focused command-line tool that searches for
MeshCore-compatible Ed25519 keypairs subject to simple vanity constraints.
The code is organized as a single binary crate with the following logical
components (files under `src/`):

- `main.rs` — CLI, runtime coordination, progress reporting, file I/O and
  verification of found keys.
- `keygen.rs` — deterministic Ed25519 keypair generation (seed -> SHA-512 ->
  clamp -> scalar multiply) and `validate_for_meshcore` logic.
- `worker.rs` — worker pool and CPU worker loop that batch-generates keys and
  sends matches over a channel to the main thread.
- `pattern.rs` — hot-path pattern and prefix matching implemented both for
  hex strings and directly on public key bytes for speed.
- `metal_gpu.rs` — macOS Metal GPU worker and large Metal shader implementing
  full key generation on GPU (platform-specific, high-complexity code).
- `gpu_detect.rs` — runtime heuristics to select a best GPU backend (Metal,
  CUDA, Vulkan, OpenCL) — primarily detection code and fallbacks.

## Data flow

1. `main.rs` parses CLI args and builds a `PatternConfig`.
2. `main.rs` creates a `WorkerPool` with N CPU workers (and optionally a
   GPU worker on macOS).
3. Workers generate keys in tight loops (batched) using `keygen::generate_*`
   and test patterns via `pattern::matches_pattern_bytes`.
4. When a worker finds a candidate it sends a `KeyInfo` over a
   crossbeam channel to `main.rs`.
5. The main thread receives candidates, optionally validates with
   `keygen::validate_for_meshcore`, saves files (unless `--benchmark`), and
   updates counters / progress display.

## Key design constraints & invariants

- The key generation algorithm must be deterministic for a given seed and
  produce the exact private/public byte layout expected by MeshCore
  (private = 32-byte clamped scalar || 32-byte SHA512 suffix).
- Verification defaults ON: `validate_for_meshcore` tests prefix byte rules
  and an Ed25519-based ECDH check; disabling verification requires an explicit
  CLI flag (`--no-verify`).
- Performance-sensitive code paths are optimized to avoid allocations and
  unnecessary hex conversions (pattern matching works directly on bytes).
- Batching parameters are tuned for throughput (`BATCH_SIZE` in CPU workers
  and `GPU_BATCH_SIZE` in `metal_gpu.rs`) and are explicit constants.
- The GPU path is macOS-specific and contains a large hand-written Metal
  shader; changes here must be treated as high-risk for correctness and
  performance regressions.

## Stability vs experimental

- Stable / core: `keygen.rs`, `pattern.rs`, `worker.rs`, and `main.rs` contain
  unit tests and are the project's most relied-on code.
- Experimental / platform-specific: `metal_gpu.rs` (heavy GPU shader) and
  parts of `gpu_detect.rs` (which use system commands / heuristics) are
  platform-dependent and should be considered experimental.
