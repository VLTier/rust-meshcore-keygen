# Key Pair Storage Feature - Implementation Guide

## Overview

This document describes the implementation of a comprehensive key pair storage system for the MeshCore keygen tool. The storage system allows saving all generated keys (not just pattern matches), searching for existing keys, tagging, and various management operations.

## Current Status

### ✅ Completed
- Storage module structure created (`src/storage.rs`)
- CLI arguments added to main.rs
- JSON-based file storage implementation (portable, no external DB dependencies)
- Core storage operations:
  - Store individual keys
  - Batch store keys
  - Pattern-based search
  - Prefix-based search
  - Tagging system
  - In-use flag management
  - Statistics collection
  - Database verification and optimization

### ⏳ Pending (Network Issues)
Due to network connectivity issues preventing dependency downloads, the following tasks are pending:

1. **Build and Test**: Need to compile and run tests once network is restored
2. **Integration with main.rs**: Connect storage to the key generation workflow
3. **CLI Implementation**: Implement command handlers for storage operations
4. **Documentation**: Update README with storage feature documentation

## Architecture

### Storage Backend

**Current Implementation**: JSON-based file storage
- **Format**: Single JSON file with indexed structure
- **Pros**: 
  - No external dependencies
  - Human-readable
  - Easy to backup/share
  - Cross-platform compatible
- **Cons**:
  - Not suitable for millions of keys
  - Entire file loaded into memory
  - Manual index management

**Future Enhancement**: SQLite database
- Add `rusqlite` dependency when network available
- Maintains same public API
- Better performance for large datasets
- True ACID transactions

### Data Model

```rust
KeyPairMetadata {
    id: String,              // public_key (unique identifier)
    private_key: String,     // 128 hex chars
    public_key: String,      // 64 hex chars
    node_id: String,         // first 2 chars
    first_8_chars: String,   // for vanity matching
    last_8_chars: String,    // for vanity matching
    created_at: DateTime,    // timestamp
    machine_hash: String,    // machine identifier
    pattern_matched: Option<String>, // pattern if matched
    attempts_count: Option<u64>, // generation attempts
    tags: Vec<String>,       // user-defined tags
    in_use: bool,            // marked as in use
}
```

### Indexes

For efficient searching, the storage maintains these indexes:
- **Pattern Index**: `pattern_matched -> [public_keys]`
- **Prefix Index**: `prefix -> [public_keys]` (for 2, 4, 6, 8 char prefixes)
- **Tag Index**: `tag -> [public_keys]`

## Usage Scenarios

### 1. Basic Key Generation with Storage

```bash
# Generate keys and store all of them
./meshcore-keygen --storage --store-all --pattern 4 -n 10

# Store only pattern matches
./meshcore-keygen --storage --pattern 4 -n 10
```

### 2. Check Storage Before Generating

```bash
# Check if pattern already exists in storage before generating
./meshcore-keygen --storage --check-storage --pattern 4 -n 1
```

### 3. Background Key Generation

```bash
# Generate keys continuously in background (no pattern matching)
./meshcore-keygen --storage --background --powersave
```

### 4. Tagging

```bash
# Generate keys with a tag
./meshcore-keygen --storage --pattern 4 -n 5 --tag "production"
```

### 5. Statistics

```bash
# Display storage statistics
./meshcore-keygen --stats
```

## Integration Points

### In main.rs

1. **Initialization**:
```rust
let storage = if args.storage {
    Some(KeyStorage::new(&args.db_path)?)
} else {
    None
};
```

2. **Check Storage** (before generation):
```rust
if args.check_storage && storage.is_some() {
    let existing = storage.unwrap().find_by_pattern(&pattern_desc)?;
    if !existing.is_empty() {
        // Display existing keys, optionally skip generation
    }
}
```

3. **Store Keys** (after generation):
```rust
if let Some(ref storage) = storage {
    if args.store_all {
        // Store every generated key
    } else {
        // Store only pattern matches
    }
    
    if let Some(ref tag) = args.tag {
        storage.add_tag(&key_id, tag)?;
    }
}
```

4. **Statistics Display**:
```rust
if args.stats {
    let storage = KeyStorage::new(&args.db_path)?;
    let stats = storage.get_stats()?;
    display_stats(&stats);
    return;
}
```

## Performance Considerations

### Hot vs Cold Storage

**Current**: Single-file approach (suitable for < 100k keys)

**Future Optimization**:
- **Hot Storage**: Recent keys, frequently accessed patterns (in-memory cache)
- **Warm Storage**: Current session keys (main JSON file)
- **Cold Storage**: Archived keys (compressed, separate files by date)

### Batch Writes

The `store_keys_batch()` method already supports efficient batch writes:
- Single file write operation
- Index updates batched
- Suitable for background generation mode

### Compression

**Evaluation**:
- JSON is text-based, compresses well (60-70% reduction with gzip)
- Trade-off: CPU for compression vs disk space
- Recommendation: Compress cold storage archives only

## Database Management

### Export/Import

```rust
// Export (to be implemented)
pub fn export(&self, path: &Path, exclude_in_use: bool) -> StorageResult<()> {
    let db = self.db.lock().unwrap();
    let keys_to_export: Vec<_> = db.keys.values()
        .filter(|k| !exclude_in_use || !k.in_use)
        .collect();
    // Write to file
}

// Import/Merge (to be implemented)
pub fn import_merge(&mut self, path: &Path) -> StorageResult<usize> {
    // Load external database
    // Merge unique keys
    // Update indexes
}
```

### Verification

Already implemented: `verify()` method checks index consistency

### Housekeeping

Already implemented: `optimize()` rebuilds all indexes

## Security Considerations

### Private Key Storage

**⚠️ CRITICAL**: Storage file contains PRIVATE KEYS

**Recommendations**:
1. Set restrictive file permissions (0600)
2. Store in encrypted volume if possible
3. Regular backups to secure location
4. Consider encrypting the storage file at rest
5. Never commit storage files to version control

### Implementation:

```rust
#[cfg(unix)]
fn set_secure_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600); // Owner read/write only
    fs::set_permissions(path, perms)?;
    Ok(())
}
```

## Testing Strategy

### Unit Tests
- Storage creation
- Key insertion (single and batch)
- Search operations (pattern, prefix, tag)
- Index consistency
- Statistics accuracy

### Integration Tests
- Full workflow: generate -> store -> search
- Multi-session: append to existing storage
- Export/import/merge operations
- Performance with large datasets

### Benchmark Tests
- Insertion rate (keys/second)
- Search performance
- Memory usage with large datasets
- File size growth patterns

## CLI Commands Summary

| Flag | Description | Example |
|------|-------------|---------|
| `--storage` | Enable key storage | `--storage` |
| `--db-path` | Database file path | `--db-path keys.json` |
| `--store-all` | Store all generated keys | `--store-all` |
| `--check-storage` | Check existing keys before generating | `--check-storage` |
| `--stats` | Display storage statistics | `--stats` |
| `--background` | Background continuous generation | `--background` |
| `--tag` | Add tag to generated keys | `--tag production` |

## Future Enhancements

### Phase 1: Complete Current Implementation
1. Build and test storage module
2. Integrate with main generation workflow
3. Add file permission management
4. Basic documentation

### Phase 2: Advanced Features
1. Compression support (optional flag)
2. Export/import commands
3. Database merge utility
4. Web UI for browsing stored keys

### Phase 3: Optimization
1. Migrate to SQLite backend
2. Hot/cold storage separation
3. Distributed key generation (network sync)
4. Key expiration/archival policies

## Best Practices

1. **Regular Backups**: Backup storage file frequently
2. **Compression**: Compress old storage files
3. **Tagging**: Use consistent tagging scheme
4. **In-Use Marking**: Mark keys when deployed
5. **Statistics**: Monitor storage growth
6. **Verification**: Run verify() periodically
7. **Optimization**: Run optimize() after bulk operations

## Migration Path

### From File-Based to SQLite

When ready to migrate:

1. Add rusqlite dependency
2. Implement SQLite backend with same API
3. Create migration tool:
```rust
fn migrate_json_to_sqlite(json_path: &Path, db_path: &Path) -> Result<()> {
    let json_storage = KeyStorage::new(json_path)?;
    let sql_storage = KeyStorageSQLite::new(db_path)?;
    
    // Read all keys from JSON
    // Write to SQLite in batches
    // Verify migration
}
```
4. Maintain JSON support as fallback

## Questions & Decisions

### Compression
- **Decision**: Implement as optional feature, off by default
- **Rationale**: Extra complexity, not needed for typical usage (< 100k keys)

### Hot/Cold Storage
- **Decision**: Single file for now, implement tiering when needed
- **Rationale**: Simpler implementation, sufficient for most use cases

### Database Choice
- **Decision**: JSON initially, SQLite as enhancement
- **Rationale**: No external dependencies, easier deployment

### Key Expiration
- **Decision**: Not implemented initially
- **Rationale**: Keys don't expire, users can manually archive

## Troubleshooting

### Large File Size
- Run `optimize()` to rebuild indexes
- Consider exporting old keys to archive
- Implement compression

### Slow Searches
- Check index consistency with `verify()`
- Run `optimize()` to rebuild indexes
- Consider migrating to SQLite

### Corruption
- Use `verify()` to check integrity
- Restore from backup
- Re-index if needed

## Conclusion

This implementation provides a solid foundation for key pair storage with room for future enhancements. The JSON-based approach is simple, portable, and sufficient for most use cases, while the architecture allows easy migration to more sophisticated backends when needed.
