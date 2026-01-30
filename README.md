# MeshCore Ed25519 Vanity Key Generator (Rust)

A high-performance Ed25519 vanity key generator for MeshCore nodes, written in Rust with CPU multi-threading and Apple Metal GPU acceleration support.

## Features

- **🚀 Blazing Fast**: Up to 500,000+ keys/second on modern hardware
- **🔧 Multi-threaded**: Automatic CPU core detection with optimal worker allocation
- **🎮 GPU Acceleration**: Metal GPU support for Apple Silicon (M1/M2/M3)
- **🎯 Pattern Matching**: Multiple vanity pattern modes
- **📁 Automatic Key Saving**: Creates speaking filenames for each found key
- **✅ MeshCore Compatible**: Generates keys in the exact format MeshCore expects

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

# Enable Metal GPU acceleration (macOS only)
./target/release/meshcore-keygen --pattern 4 -n 10 --gpu
```

### Command Line Options

```
Options:
  -n, --target-keys <N>    Number of keys to find [default: 1]
  -w, --workers <N>        Number of worker threads (auto-detected if not set)
      --gpu                Enable Metal GPU acceleration (macOS only)
      --pattern <2-8>      Pattern mode: first N chars match last N chars
      --prefix <HEX>       Search for keys starting with this hex prefix
      --vanity <2-8>       First N chars match last N chars
  -o, --output <DIR>       Output directory for key files [default: .]
      --max-time <SECS>    Maximum time to run in seconds (0 = unlimited)
  -v, --verbose            Verbose output
      --test               Run built-in tests
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

Typical performance on different hardware:

| Hardware     | CPU Only          | With Metal GPU    |
| ------------ | ----------------- | ----------------- |
| Apple M3 Pro | ~480,000 keys/sec | ~500,000 keys/sec |
| Apple M1     | ~300,000 keys/sec | ~350,000 keys/sec |
| Intel i7     | ~200,000 keys/sec | N/A               |

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

⚠️ **Keep your private keys secure and never share them!**

- Generated keys are cryptographically random
- Each run produces different results
- Backup your keys in a secure location
- Test keys before using in production

## License

MIT License

## Credits

Inspired by [meshcore-keygen](https://github.com/agessaman/meshcore-keygen) (Python version).
Rewritten in Rust for maximum performance.
