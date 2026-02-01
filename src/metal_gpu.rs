//! Metal GPU Acceleration for Key Generation
//!
//! Uses Apple's Metal API to accelerate key generation on macOS.
//! Implements full Ed25519 key generation on GPU including:
//! - Random seed generation
//! - SHA-512 hashing  
//! - Scalar clamping
//! - Ed25519 scalar multiplication (the expensive part!)
//!
//! This allows massive parallelism on Apple GPUs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;
use metal::*;

use crate::keygen::KeyInfo;
use crate::pattern::{matches_pattern_bytes, PatternConfig};

/// Number of keys to generate per GPU dispatch - large batch for GPU efficiency
const GPU_BATCH_SIZE: usize = 262144; // 256K keys per batch for high GPU utilization

/// Metal compute shader implementing full Ed25519 key generation
/// This includes SHA-512, scalar clamping, and Ed25519 point multiplication
const METAL_SHADER: &str = r#"
#include <metal_stdlib>
using namespace metal;

// ============================================================================
// SHA-512 Implementation
// ============================================================================

constant uint64_t K[80] = {
    0x428a2f98d728ae22UL, 0x7137449123ef65cdUL, 0xb5c0fbcfec4d3b2fUL, 0xe9b5dba58189dbbcUL,
    0x3956c25bf348b538UL, 0x59f111f1b605d019UL, 0x923f82a4af194f9bUL, 0xab1c5ed5da6d8118UL,
    0xd807aa98a3030242UL, 0x12835b0145706fbeUL, 0x243185be4ee4b28cUL, 0x550c7dc3d5ffb4e2UL,
    0x72be5d74f27b896fUL, 0x80deb1fe3b1696b1UL, 0x9bdc06a725c71235UL, 0xc19bf174cf692694UL,
    0xe49b69c19ef14ad2UL, 0xefbe4786384f25e3UL, 0x0fc19dc68b8cd5b5UL, 0x240ca1cc77ac9c65UL,
    0x2de92c6f592b0275UL, 0x4a7484aa6ea6e483UL, 0x5cb0a9dcbd41fbd4UL, 0x76f988da831153b5UL,
    0x983e5152ee66dfabUL, 0xa831c66d2db43210UL, 0xb00327c898fb213fUL, 0xbf597fc7beef0ee4UL,
    0xc6e00bf33da88fc2UL, 0xd5a79147930aa725UL, 0x06ca6351e003826fUL, 0x142929670a0e6e70UL,
    0x27b70a8546d22ffcUL, 0x2e1b21385c26c926UL, 0x4d2c6dfc5ac42aedUL, 0x53380d139d95b3dfUL,
    0x650a73548baf63deUL, 0x766a0abb3c77b2a8UL, 0x81c2c92e47edaee6UL, 0x92722c851482353bUL,
    0xa2bfe8a14cf10364UL, 0xa81a664bbc423001UL, 0xc24b8b70d0f89791UL, 0xc76c51a30654be30UL,
    0xd192e819d6ef5218UL, 0xd69906245565a910UL, 0xf40e35855771202aUL, 0x106aa07032bbd1b8UL,
    0x19a4c116b8d2d0c8UL, 0x1e376c085141ab53UL, 0x2748774cdf8eeb99UL, 0x34b0bcb5e19b48a8UL,
    0x391c0cb3c5c95a63UL, 0x4ed8aa4ae3418acbUL, 0x5b9cca4f7763e373UL, 0x682e6ff3d6b2b8a3UL,
    0x748f82ee5defb2fcUL, 0x78a5636f43172f60UL, 0x84c87814a1f0ab72UL, 0x8cc702081a6439ecUL,
    0x90befffa23631e28UL, 0xa4506cebde82bde9UL, 0xbef9a3f7b2c67915UL, 0xc67178f2e372532bUL,
    0xca273eceea26619cUL, 0xd186b8c721c0c207UL, 0xeada7dd6cde0eb1eUL, 0xf57d4f7fee6ed178UL,
    0x06f067aa72176fbaUL, 0x0a637dc5a2c898a6UL, 0x113f9804bef90daeUL, 0x1b710b35131c471bUL,
    0x28db77f523047d84UL, 0x32caab7b40c72493UL, 0x3c9ebe0a15c9bebcUL, 0x431d67c49c100d4cUL,
    0x4cc5d4becb3e42b6UL, 0x597f299cfc657e2aUL, 0x5fcb6fab3ad6faecUL, 0x6c44198c4a475817UL
};

inline uint64_t rotr64(uint64_t x, uint64_t n) {
    return (x >> n) | (x << (64 - n));
}

void sha512_32bytes(thread const uchar* input, thread uchar* output) {
    uint64_t H[8] = {
        0x6a09e667f3bcc908UL, 0xbb67ae8584caa73bUL,
        0x3c6ef372fe94f82bUL, 0xa54ff53a5f1d36f1UL,
        0x510e527fade682d1UL, 0x9b05688c2b3e6c1fUL,
        0x1f83d9abfb41bd6bUL, 0x5be0cd19137e2179UL
    };
    
    uint64_t W[80];
    for (int i = 0; i < 4; i++) {
        W[i] = 0;
        for (int j = 0; j < 8; j++) {
            W[i] = (W[i] << 8) | input[i * 8 + j];
        }
    }
    
    W[4] = 0x8000000000000000UL;
    for (int i = 5; i < 15; i++) W[i] = 0;
    W[15] = 256;
    
    for (int i = 16; i < 80; i++) {
        uint64_t s0 = rotr64(W[i-15], 1) ^ rotr64(W[i-15], 8) ^ (W[i-15] >> 7);
        uint64_t s1 = rotr64(W[i-2], 19) ^ rotr64(W[i-2], 61) ^ (W[i-2] >> 6);
        W[i] = W[i-16] + s0 + W[i-7] + s1;
    }
    
    uint64_t a = H[0], b = H[1], c = H[2], d = H[3];
    uint64_t e = H[4], f = H[5], g = H[6], h = H[7];
    
    for (int i = 0; i < 80; i++) {
        uint64_t S1 = rotr64(e, 14) ^ rotr64(e, 18) ^ rotr64(e, 41);
        uint64_t ch = (e & f) ^ (~e & g);
        uint64_t temp1 = h + S1 + ch + K[i] + W[i];
        uint64_t S0 = rotr64(a, 28) ^ rotr64(a, 34) ^ rotr64(a, 39);
        uint64_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint64_t temp2 = S0 + maj;
        
        h = g; g = f; f = e; e = d + temp1;
        d = c; c = b; b = a; a = temp1 + temp2;
    }
    
    H[0] += a; H[1] += b; H[2] += c; H[3] += d;
    H[4] += e; H[5] += f; H[6] += g; H[7] += h;
    
    for (int i = 0; i < 8; i++) {
        for (int j = 0; j < 8; j++) {
            output[i * 8 + j] = (H[i] >> (56 - j * 8)) & 0xFF;
        }
    }
}

// ============================================================================
// Field arithmetic for Ed25519 (mod 2^255-19)
// Using 5 limbs of 51 bits each
// ============================================================================

// Metal uses int64_t for 64-bit signed integers
typedef int64_t int64;
typedef uint64_t uint64;

struct fe {
    int64 v[5];
};

inline fe fe_zero() {
    fe r;
    for (int i = 0; i < 5; i++) r.v[i] = 0;
    return r;
}

inline fe fe_one() {
    fe r = fe_zero();
    r.v[0] = 1;
    return r;
}

// Reduce to canonical form
inline void fe_reduce(thread fe& f) {
    int64 c;
    for (int i = 0; i < 4; i++) {
        c = f.v[i] >> 51;
        f.v[i] &= 0x7ffffffffffffLL;
        f.v[i+1] += c;
    }
    c = f.v[4] >> 51;
    f.v[4] &= 0x7ffffffffffffLL;
    f.v[0] += c * 19;
    
    c = f.v[0] >> 51;
    f.v[0] &= 0x7ffffffffffffLL;
    f.v[1] += c;
}

inline fe fe_add(fe a, fe b) {
    fe r;
    for (int i = 0; i < 5; i++) r.v[i] = a.v[i] + b.v[i];
    return r;
}

inline fe fe_sub(fe a, fe b) {
    fe r;
    // Add 2p to avoid negative values
    r.v[0] = a.v[0] - b.v[0] + 0xfffffffffffda;
    r.v[1] = a.v[1] - b.v[1] + 0xffffffffffffe;
    r.v[2] = a.v[2] - b.v[2] + 0xffffffffffffe;
    r.v[3] = a.v[3] - b.v[3] + 0xffffffffffffe;
    r.v[4] = a.v[4] - b.v[4] + 0xffffffffffffe;
    fe_reduce(r);
    return r;
}

inline fe fe_mul(fe a, fe b) {
    // Schoolbook multiplication with delayed reduction
    int64 a0 = a.v[0], a1 = a.v[1], a2 = a.v[2], a3 = a.v[3], a4 = a.v[4];
    int64 b0 = b.v[0], b1 = b.v[1], b2 = b.v[2], b3 = b.v[3], b4 = b.v[4];
    
    // Multiply with 19 for reduction
    int64 b1_19 = b1 * 19, b2_19 = b2 * 19, b3_19 = b3 * 19, b4_19 = b4 * 19;
    
    fe r;
    r.v[0] = a0*b0 + a1*b4_19 + a2*b3_19 + a3*b2_19 + a4*b1_19;
    r.v[1] = a0*b1 + a1*b0 + a2*b4_19 + a3*b3_19 + a4*b2_19;
    r.v[2] = a0*b2 + a1*b1 + a2*b0 + a3*b4_19 + a4*b3_19;
    r.v[3] = a0*b3 + a1*b2 + a2*b1 + a3*b0 + a4*b4_19;
    r.v[4] = a0*b4 + a1*b3 + a2*b2 + a3*b1 + a4*b0;
    
    fe_reduce(r);
    fe_reduce(r);
    return r;
}

inline fe fe_sq(fe a) {
    return fe_mul(a, a);
}

// Compute a^(2^n)
inline fe fe_sq_n(fe a, int n) {
    fe r = a;
    for (int i = 0; i < n; i++) r = fe_sq(r);
    return r;
}

// Compute inverse using Fermat's little theorem: a^(-1) = a^(p-2)
fe fe_inv(fe a) {
    fe t0 = fe_sq(a);           // a^2
    fe t1 = fe_sq_n(t0, 2);     // a^8
    t1 = fe_mul(t1, a);         // a^9
    t0 = fe_mul(t0, t1);        // a^11
    fe t2 = fe_sq(t0);          // a^22
    t1 = fe_mul(t1, t2);        // a^31 = a^(2^5-1)
    t2 = fe_sq_n(t1, 5);        // a^(2^10-32)
    t1 = fe_mul(t1, t2);        // a^(2^10-1)
    t2 = fe_sq_n(t1, 10);       // a^(2^20-1024)
    t2 = fe_mul(t2, t1);        // a^(2^20-1)
    fe t3 = fe_sq_n(t2, 20);    // a^(2^40-2^20)
    t2 = fe_mul(t3, t2);        // a^(2^40-1)
    t2 = fe_sq_n(t2, 10);       // a^(2^50-1024)
    t1 = fe_mul(t2, t1);        // a^(2^50-1)
    t2 = fe_sq_n(t1, 50);       // a^(2^100-2^50)
    t2 = fe_mul(t2, t1);        // a^(2^100-1)
    t3 = fe_sq_n(t2, 100);      // a^(2^200-2^100)
    t2 = fe_mul(t3, t2);        // a^(2^200-1)
    t2 = fe_sq_n(t2, 50);       // a^(2^250-2^50)
    t1 = fe_mul(t2, t1);        // a^(2^250-1)
    t1 = fe_sq_n(t1, 5);        // a^(2^255-32)
    return fe_mul(t1, t0);       // a^(2^255-21) = a^(p-2)
}

inline fe fe_from_bytes(thread const uchar* s) {
    fe r;
    r.v[0] = ((int64)s[0]) | ((int64)s[1] << 8) | ((int64)s[2] << 16) | 
             ((int64)s[3] << 24) | ((int64)s[4] << 32) | ((int64)s[5] << 40) |
             ((int64)(s[6] & 0x07) << 48);
    r.v[1] = ((int64)(s[6] >> 3)) | ((int64)s[7] << 5) | ((int64)s[8] << 13) |
             ((int64)s[9] << 21) | ((int64)s[10] << 29) | ((int64)s[11] << 37) |
             ((int64)(s[12] & 0x3f) << 45);
    r.v[2] = ((int64)(s[12] >> 6)) | ((int64)s[13] << 2) | ((int64)s[14] << 10) |
             ((int64)s[15] << 18) | ((int64)s[16] << 26) | ((int64)s[17] << 34) |
             ((int64)s[18] << 42) | ((int64)(s[19] & 0x01) << 50);
    r.v[3] = ((int64)(s[19] >> 1)) | ((int64)s[20] << 7) | ((int64)s[21] << 15) |
             ((int64)s[22] << 23) | ((int64)s[23] << 31) | ((int64)s[24] << 39) |
             ((int64)(s[25] & 0x0f) << 47);
    r.v[4] = ((int64)(s[25] >> 4)) | ((int64)s[26] << 4) | ((int64)s[27] << 12) |
             ((int64)s[28] << 20) | ((int64)s[29] << 28) | ((int64)s[30] << 36) |
             ((int64)(s[31] & 0x7f) << 44);
    return r;
}

inline void fe_to_bytes(thread uchar* s, fe f) {
    fe_reduce(f);
    fe_reduce(f);
    
    // Final reduction
    int64 c = (f.v[0] + 19) >> 51;
    c = (f.v[1] + c) >> 51;
    c = (f.v[2] + c) >> 51;
    c = (f.v[3] + c) >> 51;
    c = (f.v[4] + c) >> 51;
    f.v[0] += 19 * c;
    
    c = f.v[0] >> 51; f.v[0] &= 0x7ffffffffffffLL; f.v[1] += c;
    c = f.v[1] >> 51; f.v[1] &= 0x7ffffffffffffLL; f.v[2] += c;
    c = f.v[2] >> 51; f.v[2] &= 0x7ffffffffffffLL; f.v[3] += c;
    c = f.v[3] >> 51; f.v[3] &= 0x7ffffffffffffLL; f.v[4] += c;
    f.v[4] &= 0x7ffffffffffffLL;
    
    s[0]  = f.v[0] & 0xff;
    s[1]  = (f.v[0] >> 8) & 0xff;
    s[2]  = (f.v[0] >> 16) & 0xff;
    s[3]  = (f.v[0] >> 24) & 0xff;
    s[4]  = (f.v[0] >> 32) & 0xff;
    s[5]  = (f.v[0] >> 40) & 0xff;
    s[6]  = ((f.v[0] >> 48) & 0x07) | ((f.v[1] & 0x1f) << 3);
    s[7]  = (f.v[1] >> 5) & 0xff;
    s[8]  = (f.v[1] >> 13) & 0xff;
    s[9]  = (f.v[1] >> 21) & 0xff;
    s[10] = (f.v[1] >> 29) & 0xff;
    s[11] = (f.v[1] >> 37) & 0xff;
    s[12] = ((f.v[1] >> 45) & 0x3f) | ((f.v[2] & 0x03) << 6);
    s[13] = (f.v[2] >> 2) & 0xff;
    s[14] = (f.v[2] >> 10) & 0xff;
    s[15] = (f.v[2] >> 18) & 0xff;
    s[16] = (f.v[2] >> 26) & 0xff;
    s[17] = (f.v[2] >> 34) & 0xff;
    s[18] = (f.v[2] >> 42) & 0xff;
    s[19] = ((f.v[2] >> 50) & 0x01) | ((f.v[3] & 0x7f) << 1);
    s[20] = (f.v[3] >> 7) & 0xff;
    s[21] = (f.v[3] >> 15) & 0xff;
    s[22] = (f.v[3] >> 23) & 0xff;
    s[23] = (f.v[3] >> 31) & 0xff;
    s[24] = (f.v[3] >> 39) & 0xff;
    s[25] = ((f.v[3] >> 47) & 0x0f) | ((f.v[4] & 0x0f) << 4);
    s[26] = (f.v[4] >> 4) & 0xff;
    s[27] = (f.v[4] >> 12) & 0xff;
    s[28] = (f.v[4] >> 20) & 0xff;
    s[29] = (f.v[4] >> 28) & 0xff;
    s[30] = (f.v[4] >> 36) & 0xff;
    s[31] = (f.v[4] >> 44) & 0xff;
}

// ============================================================================
// Ed25519 Point Operations (Extended Coordinates)
// ============================================================================

struct ge {
    fe X, Y, Z, T;  // Extended coordinates: x=X/Z, y=Y/Z, x*y=T/Z
};

// Ed25519 constant d = -121665/121666
constant int64 D_VALS[5] = {
    0x34dca135978a3, 0x1a8283b156ebd, 0x5e7a26001c029,
    0x739c663a03cbb, 0x52036cee2b6ff
};

// 2*d
constant int64 D2_VALS[5] = {
    0x69b9426b2f159, 0x35050762add7a, 0x3cf44c0038052,
    0x6738cc7407977, 0x2406d9dc56dff
};

inline fe get_d() {
    fe r;
    for (int i = 0; i < 5; i++) r.v[i] = D_VALS[i];
    return r;
}

inline fe get_d2() {
    fe r;
    for (int i = 0; i < 5; i++) r.v[i] = D2_VALS[i];
    return r;
}

// Ed25519 base point
inline ge ge_base() {
    ge r;
    // Base point x coordinate
    r.X.v[0] = 0x62d608f25d51a; r.X.v[1] = 0x412a4b4f6592a;
    r.X.v[2] = 0x75b7171a4b31d; r.X.v[3] = 0x1ff60527118fe;
    r.X.v[4] = 0x216936d3cd6e5;
    // Base point y coordinate = 4/5
    r.Y.v[0] = 0x6666666666658; r.Y.v[1] = 0x4cccccccccccc;
    r.Y.v[2] = 0x1999999999999; r.Y.v[3] = 0x3333333333333;
    r.Y.v[4] = 0x6666666666666;
    r.Z = fe_one();
    r.T = fe_mul(r.X, r.Y);
    return r;
}

inline ge ge_zero() {
    ge r;
    r.X = fe_zero();
    r.Y = fe_one();
    r.Z = fe_one();
    r.T = fe_zero();
    return r;
}

// Point doubling
ge ge_double(ge p) {
    fe A = fe_sq(p.X);
    fe B = fe_sq(p.Y);
    fe C = fe_sq(p.Z);
    C = fe_add(C, C);
    fe D = fe_sub(fe_zero(), A);  // -a*X^2 where a=-1
    
    fe E = fe_add(p.X, p.Y);
    E = fe_sq(E);
    E = fe_sub(E, A);
    E = fe_sub(E, B);
    
    fe G = fe_add(D, B);
    fe F = fe_sub(G, C);
    fe H = fe_sub(D, B);
    
    ge r;
    r.X = fe_mul(E, F);
    r.Y = fe_mul(G, H);
    r.T = fe_mul(E, H);
    r.Z = fe_mul(F, G);
    return r;
}

// Point addition
ge ge_add(ge p, ge q) {
    fe A = fe_mul(fe_sub(p.Y, p.X), fe_sub(q.Y, q.X));
    fe B = fe_mul(fe_add(p.Y, p.X), fe_add(q.Y, q.X));
    fe C = fe_mul(fe_mul(p.T, q.T), get_d2());
    fe D = fe_add(p.Z, p.Z);
    D = fe_mul(D, q.Z);
    
    fe E = fe_sub(B, A);
    fe F = fe_sub(D, C);
    fe G = fe_add(D, C);
    fe H = fe_add(B, A);
    
    ge r;
    r.X = fe_mul(E, F);
    r.Y = fe_mul(G, H);
    r.T = fe_mul(E, H);
    r.Z = fe_mul(F, G);
    return r;
}

// Scalar multiplication using double-and-add
ge ge_scalarmult(ge base, thread const uchar* scalar) {
    ge result = ge_zero();
    ge temp = base;
    
    for (int i = 0; i < 256; i++) {
        int byte_idx = i / 8;
        int bit_idx = i % 8;
        uchar bit = (scalar[byte_idx] >> bit_idx) & 1;
        
        if (bit) {
            result = ge_add(result, temp);
        }
        temp = ge_double(temp);
    }
    
    return result;
}

// Convert point to compressed form (32 bytes)
void ge_to_bytes(thread uchar* s, ge p) {
    fe recip = fe_inv(p.Z);
    fe x = fe_mul(p.X, recip);
    fe y = fe_mul(p.Y, recip);
    
    fe_to_bytes(s, y);
    
    // Get the sign of x and encode it in the top bit
    uchar x_bytes[32];
    fe_to_bytes(x_bytes, x);
    s[31] |= (x_bytes[0] & 1) << 7;
}

// ============================================================================
// Main Kernel - Full Ed25519 Key Generation on GPU
// ============================================================================

kernel void generate_ed25519_keys(
    device const uint* random_state [[buffer(0)]],
    device const uint* batch_offset [[buffer(1)]],
    device uchar* output_public_keys [[buffer(2)]],
    device uchar* output_private_keys [[buffer(3)]],
    uint id [[thread_position_in_grid]]
) {
    uint global_id = id + batch_offset[0];
    
    // Generate random seed using multiple sources of entropy
    uint state0 = random_state[0] ^ (global_id * 2654435761u);
    uint state1 = random_state[1] ^ (global_id * 2246822519u);
    uint state2 = random_state[2] ^ (global_id * 3266489917u);
    uint state3 = random_state[3] ^ (global_id * 668265263u);
    
    // xorshift128 for seed generation
    uchar seed[32];
    for (int i = 0; i < 8; i++) {
        uint t = state0 ^ (state0 << 11);
        state0 = state1; state1 = state2; state2 = state3;
        state3 = state3 ^ (state3 >> 19) ^ t ^ (t >> 8);
        
        seed[i*4]   = state3 & 0xFF;
        seed[i*4+1] = (state3 >> 8) & 0xFF;
        seed[i*4+2] = (state3 >> 16) & 0xFF;
        seed[i*4+3] = (state3 >> 24) & 0xFF;
    }
    
    // SHA-512 hash the seed
    uchar hash[64];
    sha512_32bytes(seed, hash);
    
    // Clamp scalar (first 32 bytes of hash)
    uchar scalar[32];
    for (int i = 0; i < 32; i++) scalar[i] = hash[i];
    scalar[0] &= 248;
    scalar[31] &= 63;
    scalar[31] |= 64;
    
    // Scalar multiplication to get public key
    ge base = ge_base();
    ge public_point = ge_scalarmult(base, scalar);
    
    // Convert to compressed form
    uchar public_key[32];
    ge_to_bytes(public_key, public_point);
    
    // Write outputs
    for (int i = 0; i < 32; i++) {
        output_public_keys[id * 32 + i] = public_key[i];
        output_private_keys[id * 64 + i] = scalar[i];
        output_private_keys[id * 64 + 32 + i] = hash[32 + i];
    }
}
"#;

/// GPU worker for Metal-accelerated key generation
use std::sync::Arc;

pub fn gpu_worker_loop(
    pattern_config: &PatternConfig,
    result_sender: &Sender<KeyInfo>,
    total_attempts: &AtomicU64,
    gpu_attempts: Option<Arc<AtomicU64>>,
    should_stop: &AtomicBool,
) -> Result<(), String> {
    // Initialize Metal
    let device = Device::system_default().ok_or("No Metal device found")?;

    eprintln!("Metal GPU: Initializing on '{}'", device.name());
    eprintln!(
        "Metal GPU: Batch size = {} keys per dispatch",
        GPU_BATCH_SIZE
    );

    // Compile the shader
    let options = CompileOptions::new();
    let library = device
        .new_library_with_source(METAL_SHADER, &options)
        .map_err(|e| format!("Failed to compile Metal shader: {}", e))?;

    let kernel = library
        .get_function("generate_ed25519_keys", None)
        .map_err(|e| format!("Failed to get kernel function: {}", e))?;

    let pipeline = device
        .new_compute_pipeline_state_with_function(&kernel)
        .map_err(|e| format!("Failed to create compute pipeline: {}", e))?;

    let command_queue = device.new_command_queue();

    eprintln!(
        "Metal GPU: Pipeline ready, max threads per group = {}",
        pipeline.max_total_threads_per_threadgroup()
    );

    // Create buffers
    let random_buffer = device.new_buffer(16, MTLResourceOptions::StorageModeShared);
    let offset_buffer = device.new_buffer(4, MTLResourceOptions::StorageModeShared);
    let public_keys_buffer = device.new_buffer(
        (GPU_BATCH_SIZE * 32) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let private_keys_buffer = device.new_buffer(
        (GPU_BATCH_SIZE * 64) as u64,
        MTLResourceOptions::StorageModeShared,
    );

    let mut rng = rand::thread_rng();
    let mut batch_number: u32 = 0;

    // Use optimal thread group size
    let thread_group_size =
        std::cmp::min(pipeline.max_total_threads_per_threadgroup() as usize, 256);
    let num_thread_groups = GPU_BATCH_SIZE / thread_group_size;

    eprintln!(
        "Metal GPU: Using {} thread groups of {} threads each",
        num_thread_groups, thread_group_size
    );

    loop {
        if should_stop.load(Ordering::Relaxed) {
            break;
        }

        // Update random state and batch offset
        {
            use rand::RngCore;
            let random_ptr = random_buffer.contents() as *mut u32;
            let offset_ptr = offset_buffer.contents() as *mut u32;
            unsafe {
                for i in 0..4 {
                    *random_ptr.add(i) = rng.next_u32();
                }
                *offset_ptr = batch_number.wrapping_mul(GPU_BATCH_SIZE as u32);
            }
            batch_number = batch_number.wrapping_add(1);
        }

        // Dispatch GPU compute
        let command_buffer = command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&pipeline);
        encoder.set_buffer(0, Some(&random_buffer), 0);
        encoder.set_buffer(1, Some(&offset_buffer), 0);
        encoder.set_buffer(2, Some(&public_keys_buffer), 0);
        encoder.set_buffer(3, Some(&private_keys_buffer), 0);

        let tg_size = MTLSize::new(thread_group_size as u64, 1, 1);
        let num_tg = MTLSize::new(num_thread_groups as u64, 1, 1);

        encoder.dispatch_thread_groups(num_tg, tg_size);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Check for completion status
        let status = command_buffer.status();
        if status == MTLCommandBufferStatus::Error {
            eprintln!("Metal GPU: Command buffer error");
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Process results - check patterns and send matches
        let public_ptr = public_keys_buffer.contents() as *const u8;
        let private_ptr = private_keys_buffer.contents() as *const u8;

        for i in 0..GPU_BATCH_SIZE {
            if should_stop.load(Ordering::Relaxed) {
                return Ok(());
            }

            // Read public key
            let mut public_bytes = [0u8; 32];
            unsafe {
                std::ptr::copy_nonoverlapping(
                    public_ptr.add(i * 32),
                    public_bytes.as_mut_ptr(),
                    32,
                );
            }

            // Check pattern
            if matches_pattern_bytes(&public_bytes, pattern_config) {
                // Read private key
                let mut private_bytes = [0u8; 64];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        private_ptr.add(i * 64),
                        private_bytes.as_mut_ptr(),
                        64,
                    );
                }

                let key = KeyInfo {
                    public_hex: hex::encode(public_bytes),
                    private_hex: hex::encode(private_bytes),
                    public_bytes,
                    private_bytes,
                };

                if result_sender.send(key).is_err() {
                    return Ok(());
                }
            }
        }

        total_attempts.fetch_add(GPU_BATCH_SIZE as u64, Ordering::Relaxed);
        if let Some(counter) = &gpu_attempts {
            counter.fetch_add(GPU_BATCH_SIZE as u64, Ordering::Relaxed);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Metal device availability - skips gracefully if no Metal device
    #[test]
    fn test_metal_device_available() {
        match Device::system_default() {
            Some(device) => {
                println!("Metal device found: {}", device.name());
                assert!(!device.name().is_empty(), "Device name should not be empty");
            }
            None => {
                println!("SKIP: No Metal device available (expected on non-macOS or in CI)");
            }
        }
    }

    /// Test GPU batch size is reasonable
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_gpu_batch_size() {
        // Runtime check suppressed for clippy as GPU_BATCH_SIZE is a compile-time constant.
        assert!(
            GPU_BATCH_SIZE >= 1024,
            "GPU batch size should be at least 1024 for efficiency"
        );
        assert!(
            GPU_BATCH_SIZE <= 1_000_000,
            "GPU batch size should not be excessively large"
        );
    }
}
