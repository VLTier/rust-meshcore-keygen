# MeshCore Ed25519 Vanity Key Generator (Rust)

> **This project was vibecoded** — built entirely through AI-assisted pair programming with GitHub Copilot (Claude), transforming ideas into code through natural conversation.

## Acknowledgments

This project is a Rust reimplementation inspired by and honoring [**meshcore-keygen**](https://github.com/agessaman/meshcore-keygen) by [@agessaman](https://github.com/agessaman).
Please check out the original Python project, since [@agessaman](https://github.com/agessaman) create this fantastic idea.

### What We Borrowed & Built Upon

The original Python project provided the foundation and inspiration for this Rust version
Since I've always wanted to try out Rust, I thought that this would be a great opportunity to learn both Rust and vibe coding ...

### New Features in This Rust Version

Beyond the original, this vibecoded version adds:

- **Apple Metal GPU Acceleration** — Native GPU compute shaders for M1/M2/M3 Macs
- **Cross-Platform GPU Detection** — Metal, CUDA, Vulkan, and OpenCL support with native-first priority
- **`--benchmark` Mode** — Measure raw key generation performance
- **`--powersave` Mode** — Reduce CPU usage for background operation
- **`--brutal` Mode** — Use maximum CPU cores for peak performance
- **`--beautiful` Mode** — Rich Unicode TUI with animated progress display
- **Key Pair Storage** — Store, search, and manage generated keys in a portable database
- **10-100x Performance** — From ~6,000 keys/sec (Python) to 500,000+ keys/sec (Rust)
- **Single Binary** — No Python interpreter or dependencies needed
- **CI/CD Pipeline** — Automated builds for Linux, macOS, and Windows

---

## Overview

A high-performance Ed25519 vanity key generator for MeshCore nodes, written in Rust with CPU multi-threading and GPU acceleration support.

## Features

- **Blazing Fast**: Up to 1,600,000+ keys/second with GPU acceleration
- **Multi-threaded**: Automatic CPU core detection with optimal worker allocation
- **GPU Acceleration**: Metal GPU support for Apple Silicon (M1/M2/M3)
- **Pattern Matching**: Multiple vanity pattern modes
- **Automatic Key Saving**: Creates speaking filenames for each found key
- **MeshCore Compatible**: Generates keys in the exact format MeshCore expects
- **Power Modes**: Choose between powersave, default, or brutal CPU usage
- **Key Pair Storage**: Store and manage generated keys in a searchable database

## Key Pair Storage

Build a library of generated keys for later use, search existing keys, and manage your key collection:

### Quick Start

```bash
# Store keys as you find them
./target/release/meshcore-keygen --storage --pattern 4 -n 10

# Build a key library in the background
./target/release/meshcore-keygen --storage --store-all --background --powersave

# Check if a pattern already exists before generating
./target/release/meshcore-keygen --storage --check-storage --pattern 6

# View your key collection statistics
./target/release/meshcore-keygen --stats
```

### Storage Features

- **Save All Keys**: Store every generated key, not just pattern matches
- **Fast Search**: O(1) lookups by pattern, prefix, or tag
- **Tagging System**: Organize keys with custom labels
- **Statistics**: Track generation progress and key distribution
- **Portable Format**: JSON file, easy to backup and share
- **Secure**: File permissions and encryption recommendations
- **Background Mode**: Generate keys continuously with low CPU impact

### Storage Options

```
  --storage                Enable key storage to database
  --db-path <PATH>         Database file path [default: meshcore-keys.db]
  --store-all              Store all generated keys (not just pattern matches)
  --check-storage          Check database for existing keys before generating
  --stats                  Display storage statistics
  --background             Continuous generation mode (use with --storage --store-all)
  --tag <TAG>              Add tag to generated keys
```

### Storage Security

⚠️ **IMPORTANT**: The storage file contains PRIVATE KEYS!

- Set restrictive file permissions: `chmod 600 meshcore-keys.db`
- Store on encrypted volume if possible
- Never commit to version control (pre-configured in .gitignore)
- Encrypt before sharing: `gpg -c meshcore-keys.db`

For detailed documentation, see [STORAGE_FEATURE.md](./STORAGE_FEATURE.md).

## Installation

### Prerequisites

- Rust 1.70 or higher
- macOS (for Metal GPU support) or Linux/Windows (CPU only)

### Build

```bash
# Clone or download the project
cd meshcore-keygen

# Build in release mode (optimized)
cargo build --release

# The binary will be at ./target/release/meshcore-keygen
```

## Usage

### Basic Usage

```bash
# Generate 1 key with default 8-char vanity pattern
./target/release/meshcore-keygen

# Generate 5 keys with 4-char vanity pattern
./target/release/meshcore-keygen --pattern 4 -n 5

# Generate keys starting with prefix "AB"
./target/release/meshcore-keygen --prefix AB -n 3

# Enable  GPU acceleration
./target/release/meshcore-keygen --pattern 4 -n 10 --gpu
```

### Command Line Options

```
Options:
  -n, --target-keys <N>    Number of keys to find [default: 1]
  -w, --workers <N>        Number of worker threads (auto-detected if not set)
      --gpu                Enable GPU acceleration
      --pattern <2-8>      Pattern mode: first N chars match last N chars
      --prefix <HEX>       Search for keys starting with this hex prefix
      --vanity <2-8>       First N chars match last N chars
  -o, --output <DIR>       Output directory for key files [default: .]
      --max-time <SECS>    Maximum time to run in seconds (0 = unlimited)
      --no-verify          Disable MeshCore verification (enabled by default)
      --skip-existing      Skip keys that already exist in the output directory
      --json               Output results as JSON instead of human-readable format
  -v, --verbose            Verbose output
      --brutal             Use maximum CPU cores for peak performance
      --powersave          Power-saving mode: fewer cores for background operation
      --benchmark          Benchmark mode: measure speed without saving keys
      --beautiful          Beautiful display mode with enhanced statistics
      --refresh-ms <MS>    Display refresh interval in milliseconds [default: 500]
      --test               Run built-in tests
      
Storage Options:
      --storage            Enable key storage to database
      --db-path <PATH>     Database file path [default: meshcore-keys.db]
      --store-all          Store all generated keys, not just pattern matches
      --check-storage      Check database for existing keys before generating
      --stats              Display storage statistics
      --background         Continuous generation mode (use with --storage --store-all)
      --tag <TAG>          Add tag to generated keys
      
  -h, --help               Print help
  -V, --version            Print version
```

### Pattern Modes

#### Vanity Pattern (--pattern or --vanity)

Finds keys where the first N hex characters match the last N hex characters:

```bash
# 4-char pattern: first 4 == last 4 (e.g., ABCD...ABCD)
./target/release/meshcore-keygen --pattern 4

# 6-char pattern (harder to find)
./target/release/meshcore-keygen --pattern 6

# 8-char pattern (very rare)
./target/release/meshcore-keygen --pattern 8
```

#### Prefix Pattern (--prefix)

Finds keys starting with a specific hex prefix:

```bash
# Keys starting with "F8"
./target/release/meshcore-keygen --prefix F8

# Keys starting with "ABCD"
./target/release/meshcore-keygen --prefix ABCD
```

#### Combined Patterns

You can combine prefix with vanity patterns:

```bash
# Keys starting with "AB" AND having 4-char vanity
./target/release/meshcore-keygen --prefix AB --pattern 4
```

## Output

### Key Files

For each found key, the tool creates two files:

- `meshcore_<PATTERN>_<INDEX>_<TIMESTAMP>_public.txt` - Public key (64 hex chars)
- `meshcore_<PATTERN>_<INDEX>_<TIMESTAMP>_private.txt` - Private key (128 hex chars)

Example:

```
meshcore_7B33BDB3_1_20260130_223639_public.txt
meshcore_7B33BDB3_1_20260130_223639_private.txt
```

### Console Output

```
╔════════════════════════════════════════════════════════════╗
║     MeshCore Ed25519 Vanity Key Generator (Rust)           ║
╚════════════════════════════════════════════════════════════╝

ℹ Detected 6 CPU cores, using 6 workers
ℹ Pattern: First 4 chars == Last 4 chars
ℹ Target: 2 key(s)
ℹ Metal GPU acceleration: ENABLED

════════════════════════════════════════════════════════════
✓ Found matching key #1
════════════════════════════════════════════════════════════
  Public Key:  c5accc0b95a941b3c6464b1fc43b66d93069fdb6a37603716dd1412380d6c5ac
  First 8:     c5accc0b
  Last 8:      80d6c5ac
  Node ID:     c5
  Saved to:
    Public:  meshcore_C5ACCC0B_1_20260130_223639_public.txt
    Private: meshcore_C5ACCC0B_1_20260130_223639_private.txt

═══════════════════════════════════════════════════════════
                         SUMMARY
═══════════════════════════════════════════════════════════
  Total Time:      0.28s
  Total Attempts:  120,000
  Average Rate:    432,787 keys/sec
  Keys Found:      2
```

## Performance

### Benchmarks (Apple M3 Pro)

Real-world benchmarks finding 100 keys with 4-char pattern matching:

| Mode            | Keys/sec   | Notes                        |
| --------------- | ---------- | ---------------------------- |
| Default (CPU)   | ~567,000   | Balanced CPU usage           |
| Powersave (CPU) | ~562,000   | Uses efficiency cores only   |
| Brutal (CPU)    | ~734,000   | Uses all cores minus one     |
| GPU (Metal)     | ~1,570,000 | Metal GPU acceleration       |
| GPU + Brutal    | ~1,689,000 | GPU + all CPU cores combined |

### Performance by Hardware

| Hardware     | CPU Only          | With Metal GPU      |
| ------------ | ----------------- | ------------------- |
| Apple M3 Pro | ~734,000 keys/sec | ~1,689,000 keys/sec |
| Apple M1     | ~400,000 keys/sec | ~800,000 keys/sec   |
| Intel i7     | ~200,000 keys/sec | N/A                 |

## Testing

```bash
# Run unit tests
cargo test

# Run built-in integration tests
./target/release/meshcore-keygen --test
```

## Technical Details

### Key Format

- **Private Key**: 64 bytes (128 hex characters)
  - First 32 bytes: Clamped Ed25519 scalar
  - Last 32 bytes: SHA-512 hash suffix (for RFC 8032 compatibility)
- **Public Key**: 32 bytes (64 hex characters)
  - Ed25519 compressed point

### Algorithm

The tool uses the exact Ed25519 algorithm that MeshCore expects:

1. Generate 32-byte random seed
2. SHA-512 hash the seed
3. Clamp the first 32 bytes (scalar clamping per RFC 8032)
4. Multiply clamped scalar by Ed25519 basepoint to get public key
5. Private key = `[clamped_scalar][sha512_suffix]`

### Dependencies

- `curve25519-dalek` - Ed25519 cryptography
- `sha2` - SHA-512 hashing
- `rand` - Cryptographically secure random number generation
- `clap` - Command line argument parsing
- `rayon` - Parallel processing
- `metal` (macOS only) - GPU compute acceleration

## Security Notes

**Keep your private keys secure and never share them!**

- Generated keys are cryptographically random
- Each run produces different results
- Backup your keys in a secure location
- Test keys before using in production

## License

MIT License

---

_Built through vibecoding_
