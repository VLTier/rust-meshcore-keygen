# Key Pair Storage Feature

## Overview

The Key Pair Storage feature allows you to save all generated Ed25519 key pairs to a persistent database for later use. This is particularly useful for:

- **Building a key library**: Generate keys in advance and store them for future use
- **Pattern discovery**: Keep searching for keys even after finding pattern matches
- **Collaboration**: Share key databases with team members
- **Backup**: Never lose generated keys
- **Efficiency**: Check if desired patterns already exist before generating

## Use Cases

### 1. Build a Key Library

Generate keys continuously in the background and store them all:

```bash
# Generate keys in background with low CPU usage
./meshcore-keygen --storage --background --powersave --store-all
```

This mode generates keys indefinitely without pattern matching, storing everything for later pattern searches.

### 2. Check Before Generate

Before spending time generating a rare pattern, check if it already exists:

```bash
# Check storage first, then generate if needed
./meshcore-keygen --storage --check-storage --pattern 6 -n 1
```

### 3. Tag Generated Keys

Organize your keys with tags:

```bash
# Generate production keys with tag
./meshcore-keygen --storage --pattern 4 -n 5 --tag "production-server"

# Generate test keys with different tag
./meshcore-keygen --storage --pattern 4 -n 5 --tag "test-environment"
```

### 4. Track Statistics

Monitor your key generation progress:

```bash
# Display storage statistics
./meshcore-keygen --stats --db-path meshcore-keys.json
```

Output example:
```
═══════════════════════════════════════════════════════════
                    Storage Statistics
═══════════════════════════════════════════════════════════
Total Keys:           152,847
Keys In Use:          23
Storage Size:         45.2 MB
Oldest Key:           2026-02-01 10:23:45 UTC
Newest Key:           2026-02-03 22:48:12 UTC

Keys by Pattern:
  pattern_4:          1,247 keys
  pattern_6:          89 keys
  pattern_8:          3 keys
  none:               151,508 keys

Keys by Tag:
  production:         18 keys
  test:               12 keys
  backup:             5 keys
═══════════════════════════════════════════════════════════
```

## Storage Format

### File-Based JSON Storage

By default, keys are stored in a JSON file (default: `meshcore-keys.json`). This format is:

- **Portable**: Easy to copy, backup, and share
- **Human-readable**: Can inspect with any text editor
- **Version-controllable**: Can track changes (though avoid committing private keys!)
- **Cross-platform**: Works on Windows, macOS, and Linux

### Data Stored per Key

Each key pair entry includes:

- **Private Key**: Full 128-character hex string
- **Public Key**: Full 64-character hex string
- **Node ID**: First 2 characters of public key
- **Pattern Info**: First 8 and last 8 characters
- **Timestamp**: When the key was generated
- **Machine Hash**: Identifier of the generating machine
- **Pattern Matched**: Which pattern (if any) this key matches
- **Attempts Count**: How many attempts before finding this key
- **Tags**: User-defined labels
- **In-Use Flag**: Whether this key is currently deployed

## CLI Options

### Storage Control

| Option | Description | Default |
|--------|-------------|---------|
| `--storage` | Enable key storage | `false` |
| `--db-path <PATH>` | Database file location | `meshcore-keys.json` |
| `--store-all` | Store ALL generated keys, not just matches | `false` |
| `--check-storage` | Check for existing keys before generating | `false` |

### Background Generation

| Option | Description | Default |
|--------|-------------|---------|
| `--background` | Generate keys continuously without pattern | `false` |

Use with `--storage --store-all` for best results.

### Organization

| Option | Description | Default |
|--------|-------------|---------|
| `--tag <TAG>` | Add tag to generated keys | none |

### Monitoring

| Option | Description | Default |
|--------|-------------|---------|
| `--stats` | Display storage statistics and exit | `false` |

## Workflows

### Workflow 1: Standard Generation with Storage

```bash
# Enable storage, find 10 keys with 4-char pattern
./meshcore-keygen --storage --pattern 4 -n 10

# Keys matching pattern are automatically stored
# Can add tags for organization
./meshcore-keygen --storage --pattern 4 -n 10 --tag "server-keys"
```

### Workflow 2: Background Library Building

```bash
# Start background generation (runs indefinitely)
./meshcore-keygen --storage --store-all --background --powersave

# Let it run for hours/days
# Check progress periodically
./meshcore-keygen --stats
```

### Workflow 3: Search-First Pattern Matching

```bash
# First check if pattern exists
./meshcore-keygen --storage --check-storage --pattern 6

# If not found, generate with storage
./meshcore-keygen --storage --pattern 6 -n 1
```

### Workflow 4: Database Management

```bash
# View statistics
./meshcore-keygen --stats --db-path my-keys.json

# Verify database integrity
./meshcore-keygen --storage --verify --db-path my-keys.json

# Optimize (rebuild indexes)
./meshcore-keygen --storage --optimize --db-path my-keys.json
```

## Performance Considerations

### Memory Usage

- JSON storage loads entire database into memory
- Suitable for < 100,000 keys (~50 MB)
- For larger datasets, consider future SQLite upgrade

### Disk Space

- Approximately 500 bytes per key (uncompressed)
- 10,000 keys ≈ 5 MB
- 100,000 keys ≈ 50 MB
- Compress archived databases with gzip for 60-70% savings

### Search Performance

- Pattern search: O(1) with indexes
- Prefix search: O(1) with indexes
- Tag search: O(1) with indexes
- Memory-based, very fast (< 1ms for typical queries)

## Security Considerations

### ⚠️ CRITICAL: Private Key Protection

The storage file contains **PRIVATE KEYS**. Protect it carefully:

1. **File Permissions**: Set restrictive permissions
   ```bash
   chmod 600 meshcore-keys.json
   ```

2. **Encryption**: Store on encrypted volume if possible

3. **Backups**: Backup to secure location only
   ```bash
   # Good - encrypted backup
   gpg -c meshcore-keys.json
   
   # Bad - unencrypted cloud storage
   cp meshcore-keys.json ~/Dropbox/  # DON'T DO THIS
   ```

4. **Version Control**: **NEVER** commit to git
   ```bash
   # Add to .gitignore
   echo "*.json" >> .gitignore
   echo "meshcore-keys.*" >> .gitignore
   ```

5. **Sharing**: Encrypt before sending
   ```bash
   # Encrypt for sharing
   gpg --encrypt --recipient user@example.com meshcore-keys.json
   ```

### Machine Hash

Each key records which machine generated it (hashed). This helps:
- Track key origin
- Identify which machine found rare patterns
- Audit key generation history

## Database Operations

### Export/Import (Future)

```bash
# Export keys (exclude in-use)
./meshcore-keygen --export --exclude-in-use -o backup.json

# Import and merge databases
./meshcore-keygen --import --merge source.json --db-path dest.json
```

### Tagging Operations (Future)

```bash
# Add tag to existing key
./meshcore-keygen --add-tag "production" --key <PUBLIC_KEY>

# Remove tag
./meshcore-keygen --remove-tag "test" --key <PUBLIC_KEY>

# Mark as in-use
./meshcore-keygen --mark-in-use --key <PUBLIC_KEY>
```

## Best Practices

### 1. Regular Backups

```bash
# Daily backup script
#!/bin/bash
DATE=$(date +%Y%m%d)
cp meshcore-keys.json backups/meshcore-keys-$DATE.json
gzip backups/meshcore-keys-$DATE.json
```

### 2. Separate Databases

Use different databases for different purposes:

```bash
# Production keys
./meshcore-keygen --storage --db-path prod-keys.json --tag production

# Test keys
./meshcore-keygen --storage --db-path test-keys.json --tag test

# Background generation
./meshcore-keygen --storage --db-path library.json --background
```

### 3. Tagging Convention

Establish consistent tagging:
- **Environment**: `production`, `staging`, `test`, `dev`
- **Purpose**: `api-server`, `web-server`, `database`, `cache`
- **Status**: `active`, `backup`, `archived`, `deprecated`
- **Project**: `project-alpha`, `project-beta`

### 4. Monitor Growth

```bash
# Check storage size regularly
ls -lh meshcore-keys.json

# View statistics
./meshcore-keygen --stats

# If growing too large (> 100 MB), consider:
# - Archiving old unused keys
# - Splitting into multiple databases
# - Upgrading to SQLite backend (future)
```

### 5. Mark Keys as In-Use

When deploying a key:

```bash
# Mark key as in-use (future feature)
./meshcore-keygen --mark-in-use --key <PUBLIC_KEY> --tag "deployed-prod-01"
```

This prevents accidental reuse and helps track which keys are active.

## Troubleshooting

### "Database file is corrupted"

```bash
# Verify integrity
./meshcore-keygen --storage --verify

# If corrupted, restore from backup
cp backups/meshcore-keys-20260203.json meshcore-keys.json

# Rebuild indexes
./meshcore-keygen --storage --optimize
```

### "Out of memory"

Database too large for memory:

1. Archive old keys
2. Compress archived databases
3. Consider future SQLite upgrade

### "Slow searches"

```bash
# Rebuild indexes
./meshcore-keygen --storage --optimize
```

## Future Enhancements

### Phase 1 (Next Release)
- Export/import/merge commands
- Web UI for browsing keys
- Advanced filtering and search

### Phase 2 (Future)
- SQLite backend option
- Compression support
- Hot/cold storage tiers

### Phase 3 (Future)
- Distributed key generation
- Network synchronization
- Key expiration policies

## FAQ

### Q: Should I store all generated keys or just matches?

**A:** Depends on your use case:
- **Just matches** (`--storage` only): If you only care about pattern-matched keys
- **All keys** (`--storage --store-all`): If you're building a library for later searches

### Q: How many keys can I store?

**A:** Current JSON implementation handles ~100,000 keys comfortably. Beyond that, consider archiving old keys or waiting for SQLite upgrade.

### Q: Can I share my database with team members?

**A:** Yes, but **encrypt it first**! Use GPG or similar encryption before sharing.

### Q: What if I lose my storage file?

**A:** Keys are gone unless you have backups. **Always maintain encrypted backups**.

### Q: Can I search the database without generating new keys?

**A:** Currently via `--check-storage`. Future versions will add dedicated search commands.

### Q: How do I delete keys from storage?

**A:** Not yet implemented. For now, manually edit JSON file (carefully!) or use future delete commands.

### Q: Can I use multiple databases?

**A:** Yes! Use `--db-path` to specify different files for different purposes.

## Conclusion

The storage feature transforms meshcore-keygen from a one-time generator into a comprehensive key management system. Whether you're generating keys for immediate use or building a library for future needs, the storage system provides the flexibility and organization you need.

For technical details and implementation guide, see [IMPLEMENTATION.md](./IMPLEMENTATION.md).
