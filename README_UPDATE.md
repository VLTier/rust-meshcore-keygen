# README Update for Storage Feature

## Add this section to README.md after the "Features" section

---

## Key Pair Storage

Build a library of generated keys for later use, search existing keys, and manage your key collection:

### Quick Start

```bash
# Store keys as you find them
./meshcore-keygen --storage --pattern 4 -n 10

# Build a key library in the background
./meshcore-keygen --storage --store-all --background --powersave

# Check if a pattern already exists before generating
./meshcore-keygen --storage --check-storage --pattern 6

# View your key collection statistics
./meshcore-keygen --stats
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

### Common Workflows

#### Build a Key Library

Generate keys in the background and store them all for later searches:

```bash
# Start background generation with low CPU usage
./meshcore-keygen --storage --store-all --background --powersave

# Let it run for hours or days, then check progress
./meshcore-keygen --stats
```

#### Search Before Generate

Check your existing keys before spending time on a rare pattern:

```bash
# Check if pattern exists
./meshcore-keygen --storage --check-storage --pattern 8

# If not found, generate and store
./meshcore-keygen --storage --pattern 8 -n 1
```

#### Organize with Tags

Tag keys for different purposes:

```bash
# Production server keys
./meshcore-keygen --storage --pattern 4 -n 5 --tag "production"

# Test environment keys
./meshcore-keygen --storage --pattern 4 -n 5 --tag "testing"

# View statistics by tag
./meshcore-keygen --stats
```

### Security Considerations

⚠️ **IMPORTANT**: The storage file contains PRIVATE KEYS!

- Set restrictive file permissions: `chmod 600 meshcore-keys.db`
- Store on encrypted volume if possible
- Backup to secure location only
- Never commit to version control
- Encrypt before sharing: `gpg -c meshcore-keys.db`

The `.gitignore` file is pre-configured to prevent accidental commits.

### Storage Statistics

View comprehensive statistics about your key collection:

```bash
./meshcore-keygen --stats
```

Example output:
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
  pattern_4             1,247 keys
  pattern_6             89 keys
  pattern_8             3 keys
  none                  151,508 keys

Keys by Tag:
  production            18 keys
  testing               12 keys
═══════════════════════════════════════════════════════════
```

### Performance

- **Storage**: ~500 bytes per key
- **Search**: < 1ms (indexed lookups)
- **Capacity**: Optimized for < 100,000 keys
- **Memory**: Database loaded into RAM for fast searches

### Documentation

For complete documentation, see:
- [STORAGE_FEATURE.md](./STORAGE_FEATURE.md) - User guide with detailed examples
- [IMPLEMENTATION.md](./IMPLEMENTATION.md) - Technical details and architecture

---

## Add this section to the "Technical Details" area

### Storage Format

Keys are stored in JSON format with comprehensive metadata:

```json
{
  "id": "public_key_hex",
  "private_key": "128_char_hex_string",
  "public_key": "64_char_hex_string",
  "node_id": "first_2_chars",
  "first_8_chars": "ABCD1234",
  "last_8_chars": "5678EFGH",
  "created_at": "2026-02-03T22:48:12.123Z",
  "machine_hash": "abc123def456",
  "pattern_matched": "pattern_4",
  "attempts_count": 45123,
  "tags": ["production", "server-01"],
  "in_use": false
}
```

The storage system maintains efficient indexes for:
- Pattern matching (e.g., "pattern_4")
- Prefix searching (e.g., "AB", "ABCD")
- Tag queries (e.g., "production")

All searches are O(1) using hash-based indexes.
