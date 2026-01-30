//! Pattern Matching for Vanity Keys
//!
//! Supports various pattern modes:
//! - Prefix: Keys starting with specific hex prefix
//! - Vanity: First N chars match last N chars
//! - Pattern: Combined prefix and vanity matching
//! - PrefixVanity: Prefix AND vanity constraints

/// Pattern matching modes
#[derive(Clone, Debug, PartialEq)]
pub enum PatternMode {
    /// No pattern, accept any key
    #[allow(dead_code)]
    Any,
    /// Key starts with specific hex prefix
    Prefix,
    /// First N chars match last N chars (or palindromic)
    Vanity,
    /// Similar to Vanity but explicit pattern mode
    Pattern,
    /// Both prefix AND vanity must match
    PrefixVanity,
}

/// Configuration for pattern matching
#[derive(Clone, Debug)]
pub struct PatternConfig {
    pub mode: PatternMode,
    pub prefix: Option<String>,
    pub vanity_length: u8,
}

impl Default for PatternConfig {
    fn default() -> Self {
        Self {
            mode: PatternMode::Pattern,
            prefix: None,
            vanity_length: 8,
        }
    }
}

impl PatternConfig {
    /// Create a new config with prefix matching
    #[allow(dead_code)]
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            mode: PatternMode::Prefix,
            prefix: Some(prefix.to_uppercase()),
            vanity_length: 8,
        }
    }
    
    /// Create a new config with vanity matching
    #[allow(dead_code)]
    pub fn with_vanity(length: u8) -> Self {
        Self {
            mode: PatternMode::Vanity,
            prefix: None,
            vanity_length: length,
        }
    }
    
    /// Create a new config with both prefix and vanity
    #[allow(dead_code)]
    pub fn with_prefix_vanity(prefix: &str, vanity_length: u8) -> Self {
        Self {
            mode: PatternMode::PrefixVanity,
            prefix: Some(prefix.to_uppercase()),
            vanity_length,
        }
    }
    
    /// Get a human-readable description of the pattern
    pub fn description(&self) -> String {
        match &self.mode {
            PatternMode::Any => "Any key".to_string(),
            PatternMode::Prefix => {
                format!("Prefix '{}'", self.prefix.as_ref().unwrap_or(&"?".to_string()))
            }
            PatternMode::Vanity | PatternMode::Pattern => {
                format!("First {} chars == Last {} chars", self.vanity_length, self.vanity_length)
            }
            PatternMode::PrefixVanity => {
                format!(
                    "Prefix '{}' AND {}-char vanity",
                    self.prefix.as_ref().unwrap_or(&"?".to_string()),
                    self.vanity_length
                )
            }
        }
    }
    
    /// Estimate the probability of finding a match
    #[allow(dead_code)]
    pub fn estimated_probability(&self) -> f64 {
        match &self.mode {
            PatternMode::Any => 1.0,
            PatternMode::Prefix => {
                let prefix_len = self.prefix.as_ref().map(|p| p.len()).unwrap_or(0);
                1.0 / (16.0_f64.powi(prefix_len as i32))
            }
            PatternMode::Vanity | PatternMode::Pattern => {
                // First N chars matching last N chars
                // Plus palindrome chance (roughly doubles the probability)
                2.0 / (16.0_f64.powi(self.vanity_length as i32))
            }
            PatternMode::PrefixVanity => {
                let prefix_len = self.prefix.as_ref().map(|p| p.len()).unwrap_or(0);
                let prefix_prob = 1.0 / (16.0_f64.powi(prefix_len as i32));
                let vanity_prob = 2.0 / (16.0_f64.powi(self.vanity_length as i32));
                prefix_prob * vanity_prob
            }
        }
    }
}

/// Check if a hex string matches the pattern configuration
/// 
/// This is the hot path - optimized for speed
#[inline(always)]
pub fn matches_pattern(hex: &str, config: &PatternConfig) -> bool {
    let hex_upper = hex.to_uppercase();
    let hex_bytes = hex_upper.as_bytes();
    
    match &config.mode {
        PatternMode::Any => true,
        PatternMode::Prefix => {
            if let Some(prefix) = &config.prefix {
                hex_upper.starts_with(prefix)
            } else {
                true
            }
        }
        PatternMode::Vanity | PatternMode::Pattern => {
            check_vanity_pattern(hex_bytes, config.vanity_length as usize)
        }
        PatternMode::PrefixVanity => {
            if let Some(prefix) = &config.prefix {
                if !hex_upper.starts_with(prefix) {
                    return false;
                }
            }
            check_vanity_pattern(hex_bytes, config.vanity_length as usize)
        }
    }
}

/// Check if a hex string matches pattern using raw bytes (faster)
/// 
/// This is optimized to work directly with the public key bytes
/// without going through hex string conversion
#[inline(always)]
pub fn matches_pattern_bytes(public_bytes: &[u8; 32], config: &PatternConfig) -> bool {
    match &config.mode {
        PatternMode::Any => true,
        PatternMode::Prefix => {
            if let Some(prefix) = &config.prefix {
                matches_prefix_bytes(public_bytes, prefix)
            } else {
                true
            }
        }
        PatternMode::Vanity | PatternMode::Pattern => {
            check_vanity_pattern_bytes(public_bytes, config.vanity_length as usize)
        }
        PatternMode::PrefixVanity => {
            if let Some(prefix) = &config.prefix {
                if !matches_prefix_bytes(public_bytes, prefix) {
                    return false;
                }
            }
            check_vanity_pattern_bytes(public_bytes, config.vanity_length as usize)
        }
    }
}

/// Check if first N hex chars equal last N hex chars (or are palindromic)
#[inline(always)]
fn check_vanity_pattern(hex_bytes: &[u8], n: usize) -> bool {
    if hex_bytes.len() < n * 2 {
        return false;
    }
    
    let first_n = &hex_bytes[..n];
    let last_n = &hex_bytes[hex_bytes.len() - n..];
    
    // Check if first N == last N
    if first_n == last_n {
        return true;
    }
    
    // Check if first N is palindrome of last N
    first_n.iter().eq(last_n.iter().rev())
}

/// Check vanity pattern directly on bytes (faster than hex string)
#[inline(always)]
fn check_vanity_pattern_bytes(public_bytes: &[u8; 32], n_hex_chars: usize) -> bool {
    // Each byte = 2 hex chars
    // For n hex chars, we need n/2 bytes
    let n_bytes = (n_hex_chars + 1) / 2;
    
    match n_hex_chars {
        2 => {
            // Compare first byte with last byte
            let first = public_bytes[0];
            let last = public_bytes[31];
            // First byte (2 hex chars) == Last byte (2 hex chars)
            first == last ||
            // Palindrome: first nibbles reversed
            (first >> 4) == (last & 0x0F) && (first & 0x0F) == (last >> 4)
        }
        4 => {
            // Compare first 2 bytes with last 2 bytes
            let first = &public_bytes[..2];
            let last = &public_bytes[30..32];
            first == last ||
            // Palindrome at nibble level
            check_nibble_palindrome(first, last)
        }
        6 => {
            // Compare first 3 bytes with last 3 bytes
            let first = &public_bytes[..3];
            let last = &public_bytes[29..32];
            first == last ||
            check_nibble_palindrome(first, last)
        }
        8 => {
            // Compare first 4 bytes with last 4 bytes
            let first = &public_bytes[..4];
            let last = &public_bytes[28..32];
            first == last ||
            check_nibble_palindrome(first, last)
        }
        _ => {
            // General case
            let first = &public_bytes[..n_bytes];
            let last = &public_bytes[32 - n_bytes..];
            first == last || check_nibble_palindrome(first, last)
        }
    }
}

/// Check if two byte slices are palindromes at the nibble (hex char) level
#[inline(always)]
fn check_nibble_palindrome(first: &[u8], last: &[u8]) -> bool {
    if first.len() != last.len() {
        return false;
    }
    
    // Convert to nibbles and check palindrome
    let mut first_nibbles = Vec::with_capacity(first.len() * 2);
    let mut last_nibbles = Vec::with_capacity(last.len() * 2);
    
    for &b in first {
        first_nibbles.push(b >> 4);
        first_nibbles.push(b & 0x0F);
    }
    
    for &b in last {
        last_nibbles.push(b >> 4);
        last_nibbles.push(b & 0x0F);
    }
    
    first_nibbles.iter().eq(last_nibbles.iter().rev())
}

/// Check if public key bytes match a hex prefix
#[inline(always)]
fn matches_prefix_bytes(public_bytes: &[u8; 32], prefix: &str) -> bool {
    let prefix_upper = prefix.to_uppercase();
    let prefix_bytes = prefix_upper.as_bytes();
    
    for (i, &p) in prefix_bytes.iter().enumerate() {
        let byte_idx = i / 2;
        let is_high_nibble = i % 2 == 0;
        
        if byte_idx >= 32 {
            return false;
        }
        
        let nibble = if is_high_nibble {
            public_bytes[byte_idx] >> 4
        } else {
            public_bytes[byte_idx] & 0x0F
        };
        
        let expected = match p {
            b'0'..=b'9' => p - b'0',
            b'A'..=b'F' => p - b'A' + 10,
            b'a'..=b'f' => p - b'a' + 10,
            _ => return false,
        };
        
        if nibble != expected {
            return false;
        }
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_prefix_matching() {
        let config = PatternConfig::with_prefix("AB");
        assert!(matches_pattern("AB1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12345678", &config));
        assert!(matches_pattern("ab1234567890abcdef1234567890abcdef1234567890abcdef12345678", &config));
        assert!(!matches_pattern("CD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12345678", &config));
    }
    
    #[test]
    fn test_vanity_matching() {
        let config = PatternConfig::with_vanity(4);
        
        // First 4 == Last 4
        assert!(matches_pattern("ABCD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12ABCD", &config));
        
        // First 4 != Last 4
        assert!(!matches_pattern("ABCD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12WXYZ", &config));
    }
    
    #[test]
    fn test_vanity_palindrome() {
        let config = PatternConfig::with_vanity(4);
        
        // Palindrome: ABCD...DCBA
        assert!(matches_pattern("ABCD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12DCBA", &config));
    }
    
    #[test]
    fn test_prefix_vanity_combined() {
        let config = PatternConfig::with_prefix_vanity("AB", 4);
        
        // Matches prefix AND vanity
        assert!(matches_pattern("ABCD1234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12ABCD", &config));
        
        // Matches vanity but not prefix
        assert!(!matches_pattern("CD001234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12CD00", &config));
        
        // Matches prefix but not vanity
        assert!(!matches_pattern("AB001234567890ABCDEF1234567890ABCDEF1234567890ABCDEF12WXYZ", &config));
    }
    
    #[test]
    fn test_any_mode() {
        let config = PatternConfig {
            mode: PatternMode::Any,
            prefix: None,
            vanity_length: 8,
        };
        
        assert!(matches_pattern("ANYTHING1234567890ABCDEF1234567890ABCDEF1234567890RANDOM", &config));
    }
    
    #[test]
    fn test_bytes_prefix_matching() {
        let config = PatternConfig::with_prefix("AB");
        
        let mut bytes = [0u8; 32];
        bytes[0] = 0xAB;
        assert!(matches_pattern_bytes(&bytes, &config));
        
        bytes[0] = 0xCD;
        assert!(!matches_pattern_bytes(&bytes, &config));
    }
    
    #[test]
    fn test_bytes_vanity_matching() {
        let config = PatternConfig::with_vanity(4);
        
        let mut bytes = [0u8; 32];
        // Set first 2 bytes == last 2 bytes
        bytes[0] = 0xAB;
        bytes[1] = 0xCD;
        bytes[30] = 0xAB;
        bytes[31] = 0xCD;
        
        assert!(matches_pattern_bytes(&bytes, &config));
    }
    
    #[test]
    fn test_description() {
        let config = PatternConfig::with_prefix("AB");
        assert!(config.description().contains("AB"));
        
        let config = PatternConfig::with_vanity(6);
        assert!(config.description().contains("6"));
    }
    
    #[test]
    fn test_probability_estimation() {
        let config = PatternConfig::with_prefix("AB");
        let prob = config.estimated_probability();
        // 2 hex chars = 1/256
        assert!((prob - 1.0/256.0).abs() < 0.0001);
        
        let config = PatternConfig::with_vanity(4);
        let prob = config.estimated_probability();
        // 4 hex chars = ~2/65536 (including palindrome)
        assert!(prob > 0.0 && prob < 0.001);
    }
}
