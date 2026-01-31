//! Ed25519 Key Generation for MeshCore
//!
//! Implements the exact algorithm that MeshCore uses:
//! 1. Generate 32-byte random seed
//! 2. SHA512 hash the seed
//! 3. Clamp the first 32 bytes (scalar clamping)
//! 4. Use scalar multiplication to get public key
//! 5. Private key = [clamped_scalar][sha512_prefix]

use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::scalar::Scalar;
use rand::RngCore;
use sha2::{Digest, Sha512};

/// Contains the generated key information
#[derive(Clone, Debug)]
pub struct KeyInfo {
    pub public_hex: String,
    pub private_hex: String,
    pub public_bytes: [u8; 32],
    pub private_bytes: [u8; 64],
}

/// Generate a MeshCore-compatible Ed25519 keypair
///
/// This uses the exact algorithm that MeshCore expects:
/// 1. Generate 32-byte random seed
/// 2. SHA512 hash the seed
/// 3. Clamp the first 32 bytes according to Ed25519 rules
/// 4. Use the clamped scalar to derive the public key
/// 5. Private key = clamped_scalar || sha512_second_half
#[inline]
pub fn generate_meshcore_keypair() -> KeyInfo {
    let mut rng = rand::thread_rng();

    // Step 1: Generate 32-byte random seed
    let mut seed = [0u8; 32];
    rng.fill_bytes(&mut seed);

    // Step 2: SHA512 hash the seed
    let mut hasher = Sha512::new();
    hasher.update(&seed);
    let digest: [u8; 64] = hasher.finalize().into();

    // Step 3: Clamp the first 32 bytes
    let mut clamped = [0u8; 32];
    clamped.copy_from_slice(&digest[..32]);
    clamp_scalar(&mut clamped);

    // Step 4: Derive public key using scalar multiplication
    // The clamped bytes represent a scalar that we multiply with the basepoint
    let scalar = Scalar::from_bytes_mod_order(clamped);
    let public_point = &scalar * ED25519_BASEPOINT_TABLE;
    let public_bytes: [u8; 32] = public_point.compress().to_bytes();

    // Step 5: Create 64-byte private key [clamped_scalar][sha512_second_half]
    let mut private_bytes = [0u8; 64];
    private_bytes[..32].copy_from_slice(&clamped);
    private_bytes[32..].copy_from_slice(&digest[32..64]);

    KeyInfo {
        public_hex: hex::encode(public_bytes),
        private_hex: hex::encode(private_bytes),
        public_bytes,
        private_bytes,
    }
}

/// Generate a keypair from a specific seed (for testing/determinism)
#[allow(dead_code)]
pub fn generate_from_seed(seed: &[u8; 32]) -> KeyInfo {
    // Step 2: SHA512 hash the seed
    let mut hasher = Sha512::new();
    hasher.update(seed);
    let digest: [u8; 64] = hasher.finalize().into();

    // Step 3: Clamp the first 32 bytes
    let mut clamped = [0u8; 32];
    clamped.copy_from_slice(&digest[..32]);
    clamp_scalar(&mut clamped);

    // Step 4: Derive public key
    let scalar = Scalar::from_bytes_mod_order(clamped);
    let public_point = &scalar * ED25519_BASEPOINT_TABLE;
    let public_bytes: [u8; 32] = public_point.compress().to_bytes();

    // Step 5: Create private key
    let mut private_bytes = [0u8; 64];
    private_bytes[..32].copy_from_slice(&clamped);
    private_bytes[32..].copy_from_slice(&digest[32..64]);

    KeyInfo {
        public_hex: hex::encode(public_bytes),
        private_hex: hex::encode(private_bytes),
        public_bytes,
        private_bytes,
    }
}

/// Clamp a scalar according to Ed25519 rules
/// This ensures the scalar is valid for Ed25519 operations
#[inline(always)]
fn clamp_scalar(scalar: &mut [u8; 32]) {
    scalar[0] &= 248; // Clear bottom 3 bits (divisible by 8)
    scalar[31] &= 63; // Clear top 2 bits
    scalar[31] |= 64; // Set bit 6 (ensure proper range)
}

/// Verify that a private key produces the expected public key
pub fn verify_key(key: &KeyInfo) -> bool {
    // Extract the clamped scalar from private key
    let mut clamped = [0u8; 32];
    clamped.copy_from_slice(&key.private_bytes[..32]);

    // Regenerate public key
    let scalar = Scalar::from_bytes_mod_order(clamped);
    let public_point = &scalar * ED25519_BASEPOINT_TABLE;
    let derived_public: [u8; 32] = public_point.compress().to_bytes();

    // Compare
    derived_public == key.public_bytes
}

/// MeshCore validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub reason: Option<String>,
}

/// Validate that a key is compatible with MeshCore
///
/// MeshCore has specific requirements:
/// 1. Public key must NOT start with 0x00 or 0xFF
/// 2. ECDH key exchange must work correctly
/// 3. Shared secret must not be all zeros
pub fn validate_for_meshcore(key: &KeyInfo) -> ValidationResult {
    // Check 1: Public key must not start with 0x00 or 0xFF
    if key.public_bytes[0] == 0x00 {
        return ValidationResult {
            valid: false,
            reason: Some("Public key starts with 0x00 (reserved in MeshCore)".to_string()),
        };
    }
    if key.public_bytes[0] == 0xFF {
        return ValidationResult {
            valid: false,
            reason: Some("Public key starts with 0xFF (reserved in MeshCore)".to_string()),
        };
    }

    // Check 2 & 3: Verify ECDH key exchange works with a test keypair
    // Using the same test keypair that MeshCore uses for validation
    let test_client_prv: [u8; 64] = [
        0x70, 0x65, 0xe1, 0x8f, 0xd9, 0xfa, 0xbb, 0x70, 0xc1, 0xed, 0x90, 0xdc, 0xa1, 0x99, 0x07,
        0xde, 0x69, 0x8c, 0x88, 0xb7, 0x09, 0xea, 0x14, 0x6e, 0xaf, 0xd9, 0x3d, 0x9b, 0x83, 0x0c,
        0x7b, 0x60, 0xc4, 0x68, 0x11, 0x93, 0xc7, 0x9b, 0xbc, 0x39, 0x94, 0x5b, 0xa8, 0x06, 0x41,
        0x04, 0xbb, 0x61, 0x8f, 0x8f, 0xd7, 0xa8, 0x4a, 0x0a, 0xf6, 0xf5, 0x70, 0x33, 0xd6, 0xe8,
        0xdd, 0xcd, 0x64, 0x71,
    ];
    let test_client_pub: [u8; 32] = [
        0x1e, 0xc7, 0x71, 0x75, 0xb0, 0x91, 0x8e, 0xd2, 0x06, 0xf9, 0xae, 0x04, 0xec, 0x13, 0x6d,
        0x6d, 0x5d, 0x43, 0x15, 0xbb, 0x26, 0x30, 0x54, 0x27, 0xf6, 0x45, 0xb4, 0x92, 0xe9, 0x35,
        0x0c, 0x10,
    ];

    // Calculate shared secret: our private key + test client's public key
    let ss1 = ecdh_key_exchange(&key.private_bytes, &test_client_pub);

    // Calculate shared secret: test client's private key + our public key
    let ss2 = ecdh_key_exchange(&test_client_prv, &key.public_bytes);

    // Check that both shared secrets match
    if ss1 != ss2 {
        return ValidationResult {
            valid: false,
            reason: Some("ECDH key exchange produces mismatched shared secrets".to_string()),
        };
    }

    // Check that shared secret is not all zeros
    if ss1.iter().all(|&b| b == 0) {
        return ValidationResult {
            valid: false,
            reason: Some("ECDH produces all-zero shared secret".to_string()),
        };
    }

    ValidationResult {
        valid: true,
        reason: None,
    }
}

/// Perform X25519 ECDH key exchange (Ed25519 key exchange as used by MeshCore)
/// Uses the private key scalar and the other party's public key to derive shared secret
fn ecdh_key_exchange(private_key: &[u8; 64], other_public: &[u8; 32]) -> [u8; 32] {
    use curve25519_dalek::edwards::CompressedEdwardsY;

    // Get the scalar from private key (first 32 bytes)
    let mut scalar_bytes = [0u8; 32];
    scalar_bytes.copy_from_slice(&private_key[..32]);
    let scalar = Scalar::from_bytes_mod_order(scalar_bytes);

    // Decompress the other party's public key (Ed25519 point)
    let compressed = CompressedEdwardsY::from_slice(other_public).unwrap();

    if let Some(point) = compressed.decompress() {
        // Convert Ed25519 point to Montgomery form for X25519
        let montgomery = point.to_montgomery();

        // Perform scalar multiplication
        let shared = scalar * montgomery;

        shared.to_bytes()
    } else {
        // If decompression fails, return zeros (will fail validation)
        [0u8; 32]
    }
}

/// Quick check if a public key is valid for MeshCore (fast path)
/// Only checks the prefix byte, not full ECDH validation
#[inline(always)]
pub fn is_valid_meshcore_prefix(public_bytes: &[u8; 32]) -> bool {
    public_bytes[0] != 0x00 && public_bytes[0] != 0xFF
}

/// Verify a key from hex strings
#[allow(dead_code)]
pub fn verify_key_hex(private_hex: &str, expected_public_hex: &str) -> bool {
    let private_bytes = match hex::decode(private_hex) {
        Ok(bytes) if bytes.len() == 64 => bytes,
        _ => return false,
    };

    let mut clamped = [0u8; 32];
    clamped.copy_from_slice(&private_bytes[..32]);

    let scalar = Scalar::from_bytes_mod_order(clamped);
    let public_point = &scalar * ED25519_BASEPOINT_TABLE;
    let derived_public = hex::encode(public_point.compress().to_bytes());

    derived_public == expected_public_hex.to_lowercase()
}

/// Batch generate multiple keypairs for efficiency
#[allow(dead_code)]
#[inline]
pub fn generate_batch(count: usize) -> Vec<KeyInfo> {
    let mut rng = rand::thread_rng();
    let mut results = Vec::with_capacity(count);

    for _ in 0..count {
        let mut seed = [0u8; 32];
        rng.fill_bytes(&mut seed);

        let mut hasher = Sha512::new();
        hasher.update(&seed);
        let digest: [u8; 64] = hasher.finalize().into();

        let mut clamped = [0u8; 32];
        clamped.copy_from_slice(&digest[..32]);
        clamp_scalar(&mut clamped);

        let scalar = Scalar::from_bytes_mod_order(clamped);
        let public_point = &scalar * ED25519_BASEPOINT_TABLE;
        let public_bytes: [u8; 32] = public_point.compress().to_bytes();

        let mut private_bytes = [0u8; 64];
        private_bytes[..32].copy_from_slice(&clamped);
        private_bytes[32..].copy_from_slice(&digest[32..64]);

        results.push(KeyInfo {
            public_hex: hex::encode(public_bytes),
            private_hex: hex::encode(private_bytes),
            public_bytes,
            private_bytes,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let key = generate_meshcore_keypair();
        assert_eq!(key.public_hex.len(), 64);
        assert_eq!(key.private_hex.len(), 128);
        assert_eq!(key.public_bytes.len(), 32);
        assert_eq!(key.private_bytes.len(), 64);
    }

    #[test]
    fn test_key_verification() {
        let key = generate_meshcore_keypair();
        assert!(verify_key(&key));
    }

    #[test]
    fn test_deterministic_generation() {
        let seed = [42u8; 32];
        let key1 = generate_from_seed(&seed);
        let key2 = generate_from_seed(&seed);
        assert_eq!(key1.public_hex, key2.public_hex);
        assert_eq!(key1.private_hex, key2.private_hex);
    }

    #[test]
    fn test_key_uniqueness() {
        let key1 = generate_meshcore_keypair();
        let key2 = generate_meshcore_keypair();
        assert_ne!(key1.public_hex, key2.public_hex);
    }

    #[test]
    fn test_clamping() {
        let seed = [0u8; 32];
        let key = generate_from_seed(&seed);

        // Check clamping was applied
        let first_byte = key.private_bytes[0];
        let last_byte = key.private_bytes[31];

        assert_eq!(first_byte & 7, 0); // Bottom 3 bits cleared
        assert_eq!(last_byte & 192, 64); // Top 2 bits: bit 7 clear, bit 6 set
    }

    #[test]
    fn test_batch_generation() {
        let batch = generate_batch(10);
        assert_eq!(batch.len(), 10);

        for key in &batch {
            assert!(verify_key(key));
        }

        // Check uniqueness within batch
        for i in 0..batch.len() {
            for j in (i + 1)..batch.len() {
                assert_ne!(batch[i].public_hex, batch[j].public_hex);
            }
        }
    }
}
