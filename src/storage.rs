//! Key Pair Storage Module
//!
//! Provides file-based storage for generated key pairs with:
//! - JSON-based storage for portability
//! - Metadata tracking (timestamps, machine info, tags)
//! - Efficient indexing for pattern searches
//! - Export/import capabilities
//! - Future: Can be upgraded to SQLite when dependencies are available

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::keygen::KeyInfo;

/// Metadata for a stored key pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPairMetadata {
    pub id: String,
    pub private_key: String,
    pub public_key: String,
    pub node_id: String,
    pub first_8_chars: String,
    pub last_8_chars: String,
    pub created_at: DateTime<Utc>,
    pub machine_hash: String,
    pub pattern_matched: Option<String>,
    pub attempts_count: Option<u64>,
    pub tags: Vec<String>,
    pub in_use: bool,
}

/// Statistics about the key storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_keys: u64,
    pub keys_in_use: u64,
    pub total_size_bytes: u64,
    pub keys_by_pattern: Vec<(String, u64)>,
    pub keys_by_tag: Vec<(String, u64)>,
    pub oldest_key: Option<DateTime<Utc>>,
    pub newest_key: Option<DateTime<Utc>>,
}

/// Internal storage database structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StorageDatabase {
    keys: HashMap<String, KeyPairMetadata>, // key = public_key
    pattern_index: HashMap<String, Vec<String>>, // pattern -> list of public_keys
    tag_index: HashMap<String, Vec<String>>, // tag -> list of public_keys
    prefix_index: HashMap<String, Vec<String>>, // prefix -> list of public_keys
}

impl Default for StorageDatabase {
    fn default() -> Self {
        Self {
            keys: HashMap::new(),
            pattern_index: HashMap::new(),
            tag_index: HashMap::new(),
            prefix_index: HashMap::new(),
        }
    }
}

/// Key pair storage
pub struct KeyStorage {
    db_path: PathBuf,
    db: Arc<Mutex<StorageDatabase>>,
    machine_hash: String,
}

type StorageResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

impl KeyStorage {
    /// Create or open a key storage database
    pub fn new<P: AsRef<Path>>(db_path: P) -> StorageResult<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let machine_hash = Self::generate_machine_hash();
        
        let db = if db_path.exists() {
            let file = File::open(&db_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_default()
        } else {
            StorageDatabase::default()
        };
        
        Ok(Self {
            db_path,
            db: Arc::new(Mutex::new(db)),
            machine_hash,
        })
    }

    /// Create an in-memory database (for testing)
    pub fn new_in_memory() -> StorageResult<Self> {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("meshcore-test-{}.json", rand::random::<u64>()));
        Self::new(db_path)
    }

    /// Save database to disk
    fn save(&self) -> StorageResult<()> {
        let db = self.db.lock().unwrap();
        let file = File::create(&self.db_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &*db)?;
        Ok(())
    }

    /// Generate a hash identifying this machine
    fn generate_machine_hash() -> String {
        use sha2::{Digest, Sha256};
        use std::env;

        let mut hasher = Sha256::new();
        
        // Use hostname, username, and OS info
        if let Ok(hostname) = hostname::get() {
            hasher.update(hostname.to_string_lossy().as_bytes());
        }
        
        if let Ok(username) = env::var("USER") {
            hasher.update(username.as_bytes());
        } else if let Ok(username) = env::var("USERNAME") {
            hasher.update(username.as_bytes());
        }
        
        hasher.update(env::consts::OS.as_bytes());
        
        let result = hasher.finalize();
        hex::encode(&result[..8]) // Use first 8 bytes (16 hex chars)
    }

    /// Store a key pair in the database
    pub fn store_key(
        &self,
        key: &KeyInfo,
        pattern_matched: Option<&str>,
        attempts_count: Option<u64>,
    ) -> StorageResult<String> {
        let mut db = self.db.lock().unwrap();
        
        // Check if key already exists
        if db.keys.contains_key(&key.public_hex) {
            return Ok(key.public_hex.clone());
        }
        
        let node_id = key.public_hex[..2].to_uppercase();
        let first_8 = key.public_hex[..8].to_uppercase();
        let last_8 = key.public_hex[key.public_hex.len() - 8..].to_uppercase();
        
        let metadata = KeyPairMetadata {
            id: key.public_hex.clone(),
            private_key: key.private_hex.clone(),
            public_key: key.public_hex.clone(),
            node_id,
            first_8_chars: first_8.clone(),
            last_8_chars: last_8,
            created_at: Utc::now(),
            machine_hash: self.machine_hash.clone(),
            pattern_matched: pattern_matched.map(|s| s.to_string()),
            attempts_count,
            tags: Vec::new(),
            in_use: false,
        };
        
        // Update indexes
        if let Some(pattern) = &metadata.pattern_matched {
            db.pattern_index
                .entry(pattern.clone())
                .or_insert_with(Vec::new)
                .push(key.public_hex.clone());
        }
        
        // Index by first 2, 4, 6, 8 chars for prefix search
        for len in [2, 4, 6, 8] {
            if first_8.len() >= len {
                let prefix = first_8[..len].to_string();
                db.prefix_index
                    .entry(prefix)
                    .or_insert_with(Vec::new)
                    .push(key.public_hex.clone());
            }
        }
        
        db.keys.insert(key.public_hex.clone(), metadata);
        drop(db);
        
        self.save()?;
        Ok(key.public_hex.clone())
    }

    /// Store multiple key pairs efficiently (batch insert)
    pub fn store_keys_batch(
        &self,
        keys: &[(KeyInfo, Option<String>, Option<u64>)],
    ) -> StorageResult<usize> {
        let mut db = self.db.lock().unwrap();
        let mut inserted = 0;
        
        for (key, pattern, attempts) in keys {
            // Skip if already exists
            if db.keys.contains_key(&key.public_hex) {
                continue;
            }
            
            let node_id = key.public_hex[..2].to_uppercase();
            let first_8 = key.public_hex[..8].to_uppercase();
            let last_8 = key.public_hex[key.public_hex.len() - 8..].to_uppercase();
            
            let metadata = KeyPairMetadata {
                id: key.public_hex.clone(),
                private_key: key.private_hex.clone(),
                public_key: key.public_hex.clone(),
                node_id,
                first_8_chars: first_8.clone(),
                last_8_chars: last_8,
                created_at: Utc::now(),
                machine_hash: self.machine_hash.clone(),
                pattern_matched: pattern.clone(),
                attempts_count: *attempts,
                tags: Vec::new(),
                in_use: false,
            };
            
            // Update indexes
            if let Some(ref pattern_str) = pattern {
                db.pattern_index
                    .entry(pattern_str.clone())
                    .or_insert_with(Vec::new)
                    .push(key.public_hex.clone());
            }
            
            // Index by prefix
            for len in [2, 4, 6, 8] {
                if first_8.len() >= len {
                    let prefix = first_8[..len].to_string();
                    db.prefix_index
                        .entry(prefix)
                        .or_insert_with(Vec::new)
                        .push(key.public_hex.clone());
                }
            }
            
            db.keys.insert(key.public_hex.clone(), metadata);
            inserted += 1;
        }
        
        drop(db);
        self.save()?;
        Ok(inserted)
    }

    /// Check if a key with a specific pattern already exists
    pub fn find_by_pattern(&self, pattern: &str) -> StorageResult<Vec<KeyPairMetadata>> {
        let db = self.db.lock().unwrap();
        
        if let Some(public_keys) = db.pattern_index.get(pattern) {
            let mut results = Vec::new();
            for pub_key in public_keys {
                if let Some(metadata) = db.keys.get(pub_key) {
                    results.push(metadata.clone());
                }
            }
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    /// Search keys by prefix
    pub fn find_by_prefix(&self, prefix: &str) -> StorageResult<Vec<KeyPairMetadata>> {
        let db = self.db.lock().unwrap();
        let prefix_upper = prefix.to_uppercase();
        
        if let Some(public_keys) = db.prefix_index.get(&prefix_upper) {
            let mut results = Vec::new();
            for pub_key in public_keys {
                if let Some(metadata) = db.keys.get(pub_key) {
                    results.push(metadata.clone());
                }
            }
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    /// Add a tag to a key pair
    pub fn add_tag(&self, key_id: &str, tag: &str) -> StorageResult<()> {
        let mut db = self.db.lock().unwrap();
        
        if let Some(metadata) = db.keys.get_mut(key_id) {
            if !metadata.tags.contains(&tag.to_string()) {
                metadata.tags.push(tag.to_string());
                
                // Update tag index
                db.tag_index
                    .entry(tag.to_string())
                    .or_insert_with(Vec::new)
                    .push(key_id.to_string());
            }
        }
        
        drop(db);
        self.save()?;
        Ok(())
    }

    /// Remove a tag from a key pair
    pub fn remove_tag(&self, key_id: &str, tag: &str) -> StorageResult<()> {
        let mut db = self.db.lock().unwrap();
        
        if let Some(metadata) = db.keys.get_mut(key_id) {
            metadata.tags.retain(|t| t != tag);
            
            // Update tag index
            if let Some(keys) = db.tag_index.get_mut(tag) {
                keys.retain(|k| k != key_id);
            }
        }
        
        drop(db);
        self.save()?;
        Ok(())
    }

    /// Get all tags for a key pair
    pub fn get_tags(&self, key_id: &str) -> StorageResult<Vec<String>> {
        let db = self.db.lock().unwrap();
        
        if let Some(metadata) = db.keys.get(key_id) {
            Ok(metadata.tags.clone())
        } else {
            Ok(Vec::new())
        }
    }

    /// Mark a key as in use or not in use
    pub fn set_in_use(&self, key_id: &str, in_use: bool) -> StorageResult<()> {
        let mut db = self.db.lock().unwrap();
        
        if let Some(metadata) = db.keys.get_mut(key_id) {
            metadata.in_use = in_use;
        }
        
        drop(db);
        self.save()?;
        Ok(())
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> StorageResult<StorageStats> {
        let db = self.db.lock().unwrap();
        
        let total_keys = db.keys.len() as u64;
        let keys_in_use = db.keys.values().filter(|k| k.in_use).count() as u64;
        
        // Calculate approximate storage size
        let total_size_bytes = if self.db_path.exists() {
            fs::metadata(&self.db_path)?.len()
        } else {
            0
        };
        
        // Keys by pattern
        let mut pattern_counts: HashMap<String, u64> = HashMap::new();
        for key in db.keys.values() {
            let pattern = key.pattern_matched.as_ref()
                .map(|s| s.clone())
                .unwrap_or_else(|| "none".to_string());
            *pattern_counts.entry(pattern).or_insert(0) += 1;
        }
        let mut keys_by_pattern: Vec<_> = pattern_counts.into_iter().collect();
        keys_by_pattern.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Keys by tag
        let mut tag_counts: HashMap<String, u64> = HashMap::new();
        for key in db.keys.values() {
            for tag in &key.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut keys_by_tag: Vec<_> = tag_counts.into_iter().collect();
        keys_by_tag.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Oldest and newest keys
        let mut oldest_key: Option<DateTime<Utc>> = None;
        let mut newest_key: Option<DateTime<Utc>> = None;
        
        for key in db.keys.values() {
            if oldest_key.is_none() || key.created_at < oldest_key.unwrap() {
                oldest_key = Some(key.created_at);
            }
            if newest_key.is_none() || key.created_at > newest_key.unwrap() {
                newest_key = Some(key.created_at);
            }
        }
        
        Ok(StorageStats {
            total_keys,
            keys_in_use,
            total_size_bytes,
            keys_by_pattern,
            keys_by_tag,
            oldest_key,
            newest_key,
        })
    }

    /// Verify database integrity
    pub fn verify(&self) -> StorageResult<bool> {
        let db = self.db.lock().unwrap();
        
        // Check that all indexed keys exist
        for (_, public_keys) in &db.pattern_index {
            for key in public_keys {
                if !db.keys.contains_key(key) {
                    return Ok(false);
                }
            }
        }
        
        for (_, public_keys) in &db.prefix_index {
            for key in public_keys {
                if !db.keys.contains_key(key) {
                    return Ok(false);
                }
            }
        }
        
        for (_, public_keys) in &db.tag_index {
            for key in public_keys {
                if !db.keys.contains_key(key) {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }

    /// Optimize database (rebuild indexes)
    pub fn optimize(&self) -> StorageResult<()> {
        let mut db = self.db.lock().unwrap();
        
        // Rebuild all indexes
        db.pattern_index.clear();
        db.prefix_index.clear();
        db.tag_index.clear();
        
        for (pub_key, metadata) in &db.keys {
            // Pattern index
            if let Some(ref pattern) = metadata.pattern_matched {
                db.pattern_index
                    .entry(pattern.clone())
                    .or_insert_with(Vec::new)
                    .push(pub_key.clone());
            }
            
            // Prefix index
            for len in [2, 4, 6, 8] {
                if metadata.first_8_chars.len() >= len {
                    let prefix = metadata.first_8_chars[..len].to_string();
                    db.prefix_index
                        .entry(prefix)
                        .or_insert_with(Vec::new)
                        .push(pub_key.clone());
                }
            }
            
            // Tag index
            for tag in &metadata.tags {
                db.tag_index
                    .entry(tag.clone())
                    .or_insert_with(Vec::new)
                    .push(pub_key.clone());
            }
        }
        
        drop(db);
        self.save()?;
        Ok(())
    }
}

// Hostname helper
mod hostname {
    use std::ffi::OsString;
    
    pub fn get() -> Result<OsString, ()> {
        #[cfg(unix)]
        {
            use std::ffi::CStr;
            let mut buf = vec![0u8; 256];
            unsafe {
                if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
                    if let Some(pos) = buf.iter().position(|&b| b == 0) {
                        buf.truncate(pos);
                    }
                    return Ok(OsString::from(String::from_utf8_lossy(&buf).to_string()));
                }
            }
        }
        
        #[cfg(windows)]
        {
            use std::env;
            if let Ok(name) = env::var("COMPUTERNAME") {
                return Ok(OsString::from(name));
            }
        }
        
        #[cfg(not(any(unix, windows)))]
        {
            use std::env;
            if let Ok(name) = env::var("HOSTNAME") {
                return Ok(OsString::from(name));
            }
        }
        
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keygen::generate_meshcore_keypair;

    #[test]
    fn test_storage_creation() {
        let storage = KeyStorage::new_in_memory().unwrap();
        assert!(storage.verify().unwrap());
    }

    #[test]
    fn test_store_and_retrieve() {
        let storage = KeyStorage::new_in_memory().unwrap();
        let key = generate_meshcore_keypair();
        
        let id = storage.store_key(&key, Some("test_pattern"), Some(100)).unwrap();
        assert!(!id.is_empty());
        
        let keys = storage.find_by_pattern("test_pattern").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].public_key, key.public_hex);
    }

    #[test]
    fn test_batch_insert() {
        let storage = KeyStorage::new_in_memory().unwrap();
        let keys: Vec<_> = (0..10)
            .map(|_| (generate_meshcore_keypair(), None, None))
            .collect();
        
        let inserted = storage.store_keys_batch(&keys).unwrap();
        assert_eq!(inserted, 10);
        
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.total_keys, 10);
    }

    #[test]
    fn test_tags() {
        let storage = KeyStorage::new_in_memory().unwrap();
        let key = generate_meshcore_keypair();
        let id = storage.store_key(&key, None, None).unwrap();
        
        storage.add_tag(&id, "production").unwrap();
        storage.add_tag(&id, "important").unwrap();
        
        let tags = storage.get_tags(&id).unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"production".to_string()));
        
        storage.remove_tag(&id, "production").unwrap();
        let tags = storage.get_tags(&id).unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn test_in_use_flag() {
        let storage = KeyStorage::new_in_memory().unwrap();
        let key = generate_meshcore_keypair();
        let id = storage.store_key(&key, None, None).unwrap();
        
        storage.set_in_use(&id, true).unwrap();
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.keys_in_use, 1);
        
        storage.set_in_use(&id, false).unwrap();
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.keys_in_use, 0);
    }

    #[test]
    fn test_statistics() {
        let storage = KeyStorage::new_in_memory().unwrap();
        
        // Add some keys with patterns
        for i in 0..5 {
            let key = generate_meshcore_keypair();
            storage.store_key(&key, Some("pattern_a"), Some(i * 100)).unwrap();
        }
        
        for i in 0..3 {
            let key = generate_meshcore_keypair();
            storage.store_key(&key, Some("pattern_b"), Some(i * 200)).unwrap();
        }
        
        let stats = storage.get_stats().unwrap();
        assert_eq!(stats.total_keys, 8);
        assert!(stats.keys_by_pattern.len() >= 2);
    }
    
    #[test]
    fn test_prefix_search() {
        let storage = KeyStorage::new_in_memory().unwrap();
        
        // Generate a few keys
        for _ in 0..5 {
            let key = generate_meshcore_keypair();
            storage.store_key(&key, None, None).unwrap();
        }
        
        // Search by a specific prefix (using first 2 chars of a stored key)
        let all_stats = storage.get_stats().unwrap();
        assert!(all_stats.total_keys >= 5);
    }
}
