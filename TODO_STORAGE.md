# TODO: Complete Storage Feature Implementation

## Current Status

Phase 1 has been completed with the storage module structure, CLI arguments, and comprehensive documentation. However, **network connectivity issues** are preventing the build and integration phases.

## Immediate Next Steps (When Network Available)

### 1. Build and Verify Compilation

```bash
# Clean build
cargo clean
cargo build --release

# Run tests
cargo test

# Check for warnings
cargo clippy
```

**Expected Issues to Fix:**
- None anticipated - code follows Rust best practices
- May need minor syntax adjustments

### 2. Integration with Main Workflow

In `src/main.rs`, add these integrations:

#### A. Initialize Storage (after argument parsing)

```rust
// Around line 150, after args parsing
let storage = if args.storage {
    match storage::KeyStorage::new(&args.db_path) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Failed to initialize storage: {}", e);
            if !args.json {
                eprintln!("Continuing without storage...");
            }
            None
        }
    }
} else {
    None
};
```

#### B. Handle --stats Flag (early in main())

```rust
// After args parsing, before other operations
if args.stats {
    let storage = match storage::KeyStorage::new(&args.db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening storage: {}", e);
            std::process::exit(1);
        }
    };
    
    match storage.get_stats() {
        Ok(stats) => display_storage_stats(&stats),
        Err(e) => {
            eprintln!("Error getting stats: {}", e);
            std::process::exit(1);
        }
    }
    return;
}
```

#### C. Check Storage Before Generation

```rust
// Before starting worker pool (around line 300)
if args.check_storage {
    if let Some(ref storage) = storage {
        let pattern_desc = if let Some(p) = args.pattern.or(args.vanity) {
            format!("pattern_{}", p)
        } else if let Some(ref pre) = args.prefix {
            pre.clone()
        } else {
            "any".to_string()
        };
        
        match storage.find_by_pattern(&pattern_desc) {
            Ok(existing) if !existing.is_empty() => {
                println!("\n✓ Found {} existing keys matching pattern '{}'", 
                    existing.len(), pattern_desc);
                
                if !args.json {
                    println!("\nFirst few matches:");
                    for (i, key) in existing.iter().take(5).enumerate() {
                        println!("  {}. {} ({})", i + 1, 
                            key.public_key, 
                            key.created_at.format("%Y-%m-%d %H:%M"));
                    }
                    
                    if existing.len() > 5 {
                        println!("  ... and {} more", existing.len() - 5);
                    }
                }
                
                // Optionally exit or continue based on user preference
                println!("\nContinuing with generation...\n");
            }
            Ok(_) => {
                println!("ℹ No existing keys found for pattern '{}'\n", pattern_desc);
            }
            Err(e) => {
                eprintln!("Warning: Failed to check storage: {}", e);
            }
        }
    }
}
```

#### D. Store Keys After Generation

Find the section where keys are found (around line 400-500, in the result handling loop):

```rust
// After a key is found and verified
if let Some(ref storage) = storage {
    let should_store = args.store_all || 
        matches_pattern(&key.public_hex, &pattern_config);
    
    if should_store {
        let pattern_matched = if matches_pattern(&key.public_hex, &pattern_config) {
            Some(pattern_config.description())
        } else {
            None
        };
        
        match storage.store_key(&key, pattern_matched.as_deref(), Some(total_attempts)) {
            Ok(key_id) => {
                if args.verbose {
                    println!("  Stored to database: {}", key_id);
                }
                
                // Add tag if specified
                if let Some(ref tag) = args.tag {
                    if let Err(e) = storage.add_tag(&key_id, tag) {
                        eprintln!("  Warning: Failed to add tag: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("  Warning: Failed to store key: {}", e);
            }
        }
    }
}
```

#### E. Background Generation Mode

```rust
// Add special handling for --background flag
if args.background {
    if !args.storage {
        eprintln!("Error: --background requires --storage flag");
        std::process::exit(1);
    }
    
    if !args.store_all {
        println!("Note: --background automatically enables --store-all");
    }
    
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║         Background Key Generation Mode                     ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");
    println!("ℹ Generating keys continuously without pattern matching");
    println!("ℹ Press Ctrl+C to stop\n");
    
    // Run indefinitely
    // Implement continuous generation loop
}
```

### 3. Add Helper Functions

Add to `src/main.rs`:

```rust
use crate::storage::StorageStats;

/// Display storage statistics
fn display_storage_stats(stats: &StorageStats) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("                  Storage Statistics");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total Keys:           {:>12}", format_number(stats.total_keys));
    println!("Keys In Use:          {:>12}", format_number(stats.keys_in_use));
    println!("Storage Size:         {:>12}", format_size(stats.total_size_bytes));
    
    if let Some(oldest) = stats.oldest_key {
        println!("Oldest Key:           {}", oldest.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    if let Some(newest) = stats.newest_key {
        println!("Newest Key:           {}", newest.format("%Y-%m-%d %H:%M:%S UTC"));
    }
    
    if !stats.keys_by_pattern.is_empty() {
        println!("\nKeys by Pattern:");
        for (pattern, count) in stats.keys_by_pattern.iter().take(10) {
            println!("  {:20} {:>12} keys", pattern, format_number(*count));
        }
    }
    
    if !stats.keys_by_tag.is_empty() {
        println!("\nKeys by Tag:");
        for (tag, count) in stats.keys_by_tag.iter().take(10) {
            println!("  {:20} {:>12} keys", tag, format_number(*count));
        }
    }
    
    println!("═══════════════════════════════════════════════════════════\n");
}

fn format_number(n: u64) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap()
        .join(",")
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}
```

### 4. Testing Checklist

Once building successfully:

```bash
# Unit tests
cargo test --lib

# Integration test - basic storage
./target/release/meshcore-keygen --storage --pattern 4 -n 5 --db-path test.json

# Check stats
./target/release/meshcore-keygen --stats --db-path test.json

# Test tagging
./target/release/meshcore-keygen --storage --pattern 4 -n 3 --tag "test" --db-path test.json

# Test check-storage
./target/release/meshcore-keygen --storage --check-storage --pattern 4 --db-path test.json

# Test store-all mode
./target/release/meshcore-keygen --storage --store-all --benchmark -n 1000 --db-path test.json

# Cleanup
rm test.json
```

### 5. Update README.md

Add a new section about the storage feature:

```markdown
## Key Pair Storage

Generate and store keys for later use:

```bash
# Store generated keys
./meshcore-keygen --storage --pattern 4 -n 10

# Build a key library in background
./meshcore-keygen --storage --store-all --background --powersave

# Check for existing patterns
./meshcore-keygen --storage --check-storage --pattern 6

# View statistics
./meshcore-keygen --stats
```

See [STORAGE_FEATURE.md](./STORAGE_FEATURE.md) for detailed documentation.
```

### 6. Final Validation

Before considering complete:

- [ ] Code compiles without errors
- [ ] All tests pass
- [ ] CLI flags work as expected
- [ ] Storage file is created correctly
- [ ] Statistics display correctly
- [ ] Documentation is accurate
- [ ] README updated
- [ ] Security considerations verified

### 7. Known Issues to Address

None identified yet. Will update as issues are discovered during testing.

### 8. Performance Testing

Once working:

```bash
# Generate 10k keys and store
time ./meshcore-keygen --storage --store-all --benchmark -n 10000

# Check file size
ls -lh meshcore-keys.json

# Test search performance
time ./meshcore-keygen --check-storage --pattern 4

# Test stats performance
time ./meshcore-keygen --stats
```

### 9. Security Audit

- [ ] Verify file permissions set correctly (600)
- [ ] Confirm .gitignore prevents accidental commits
- [ ] Document key protection in README
- [ ] Add warnings in CLI output about private key storage

### 10. Optional Enhancements (Future)

Consider if time permits:
- Export command implementation
- Import/merge command implementation
- Dedicated search command (not just --check-storage)
- Key deletion command
- Advanced filtering options

## Questions to Address

1. **Should --background mode require explicit confirmation?**
   - It runs indefinitely, might want to confirm

2. **Should --check-storage automatically skip generation if keys found?**
   - Currently continues, could add --skip-if-found flag

3. **Default behavior for --storage with no other flags?**
   - Store pattern matches only (current)
   - Or require explicit --store-all?

4. **File permissions on Windows?**
   - Unix has chmod 600, Windows needs different approach

5. **Maximum database size warning?**
   - Warn when approaching 100k keys?
   - Suggest archiving or SQLite upgrade?

## Completion Criteria

The storage feature is considered complete when:

1. ✅ Code compiles without errors
2. ✅ All unit tests pass
3. ✅ Integration tests verify end-to-end workflows
4. ✅ CLI commands work as documented
5. ✅ Statistics display correctly formatted
6. ✅ File permissions protect private keys
7. ✅ Documentation is comprehensive and accurate
8. ✅ README updated with storage examples
9. ✅ Performance is acceptable (< 1s for typical operations)
10. ✅ No data corruption or loss issues

## Estimated Time to Complete

- Integration coding: 2-3 hours
- Testing: 1-2 hours
- Documentation updates: 1 hour
- Performance testing: 30 minutes
- **Total: 5-7 hours** (after network is restored)

## Notes

- Network issues prevented compilation on 2026-02-03
- All code and documentation prepared
- Ready to proceed immediately when network available
- No blocking issues identified in design
