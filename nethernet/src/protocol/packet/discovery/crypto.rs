//! Encryption and decryption utilities for LAN discovery packets.
//!
//! Discovery packets are encrypted using AES-ECB with PKCS7 padding,
//! and authenticated with HMAC-SHA256 checksums.

use crate::error::{NethernetError, Result};
use aes::Aes256;
use aes::cipher::{Block, BlockDecrypt, BlockEncrypt, KeyInit};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// The encryption key used for packets transmitted during LAN discovery.
/// This is the SHA-256 hash of 0xdeadbeef (also referenced as Application ID).
/// Computed once and cached for all subsequent calls.
static ENCRYPTION_KEY: LazyLock<[u8; 32]> = LazyLock::new(|| {
    let mut hasher = Sha256::new();
    hasher.update(0xdeadbeef_u64.to_le_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
});

/// Access the module's static 32-byte encryption key.
///
/// The key is computed once at initialization and cached; this function returns
/// a static reference to that 32-byte key for use in encryption and HMAC.
///
/// # Examples
///
/// ```
/// let k1 = encryption_key();
/// let k2 = encryption_key();
/// // Both calls yield the same 32-byte key content and the same static reference.
/// assert_eq!(k1, k2);
/// assert_eq!(k1.len(), 32);
/// ```
pub(crate) fn encryption_key() -> &'static [u8; 32] {
    &ENCRYPTION_KEY
}

/// Encrypts the given bytes using AES-256 in ECB mode with PKCS#7 padding.
///
/// The input is padded to a 16-byte boundary and each block is encrypted in place; the returned
/// `Vec<u8>` contains the ciphertext whose length is a multiple of 16 bytes.
///
/// # Examples
///
/// ```
/// let plaintext = b"nethernet";
/// let ciphertext = crate::protocol::packet::discovery::crypto::encrypt(plaintext).unwrap();
/// assert!(ciphertext.len() % 16 == 0);
/// assert_ne!(ciphertext, plaintext);
/// ```
pub(crate) fn encrypt(data: &[u8]) -> Result<Vec<u8>> {
    let key = encryption_key();
    let cipher = Aes256::new(key.into());

    // Apply PKCS7 padding
    let block_size = 16;
    let padding_len = block_size - (data.len() % block_size);
    let mut padded = data.to_vec();
    padded.extend(vec![padding_len as u8; padding_len]);

    // Encrypt each block
    for chunk in padded.chunks_exact_mut(block_size) {
        let block = Block::<Aes256>::from_mut_slice(chunk);
        cipher.encrypt_block(block);
    }

    Ok(padded)
}

/// Decrypts a byte slice that was encrypted with AES-256-ECB and PKCS#7 padding.
///
/// Returns the decrypted plaintext on success. Returns an error if the input length is zero or not a multiple of 16, or if PKCS#7 padding is invalid.
///
/// # Examples
///
/// ```
/// let plaintext = b"example";
/// let ciphertext = super::encrypt(plaintext).unwrap();
/// let decrypted = super::decrypt(&ciphertext).unwrap();
/// assert_eq!(decrypted, plaintext);
/// ```
pub(crate) fn decrypt(data: &[u8]) -> Result<Vec<u8>> {
    let key = encryption_key();
    let cipher = Aes256::new(key.into());

    if data.is_empty() || data.len() % 16 != 0 {
        return Err(NethernetError::Other(
            "Invalid encrypted data length".to_string(),
        ));
    }

    // Decrypt each block
    let mut decrypted = data.to_vec();
    for chunk in decrypted.chunks_exact_mut(16) {
        let block = Block::<Aes256>::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }

    // Remove PKCS7 padding
    if let Some(&padding_len) = decrypted.last() {
        if padding_len > 0 && padding_len <= 16 {
            let data_len = decrypted.len();
            if data_len >= padding_len as usize {
                // Verify padding (constant-time)
                let padding_start = data_len - padding_len as usize;
                let mut mismatched: u8 = 0;
                for &byte in &decrypted[padding_start..] {
                    mismatched |= byte ^ padding_len;
                }
                if mismatched == 0 {
                    decrypted.truncate(padding_start);
                    return Ok(decrypted);
                }
            }
        }
    }

    Err(NethernetError::Other("Invalid padding".to_string()))
}

/// Computes an HMAC-SHA256 checksum of the provided data using the module's static encryption key.
///
/// Returns a 32-byte HMAC-SHA256 value.
///
/// # Examples
///
/// ```
/// let sum = nethernet::protocol::packet::discovery::crypto::compute_checksum(b"hello");
/// assert_eq!(sum.len(), 32);
/// ```
pub(crate) fn compute_checksum(data: &[u8]) -> [u8; 32] {
    let key = encryption_key();
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    let mut checksum = [0u8; 32];
    checksum.copy_from_slice(&result.into_bytes());
    checksum
}

/// Verifies that `data` matches the given HMAC-SHA256 `expected` checksum using the module's encryption key.
///
/// # Examples
///
/// ```
/// let data = b"hello";
/// let expected = compute_checksum(data);
/// assert!(verify_checksum(data, &expected));
/// let mut wrong = expected;
/// wrong[0] ^= 0xff;
/// assert!(!verify_checksum(data, &wrong));
/// ```
///
/// # Returns
///
/// `true` if `expected` matches the HMAC-SHA256 of `data`, `false` otherwise.
pub(crate) fn verify_checksum(data: &[u8], expected: &[u8; 32]) -> bool {
    let key = encryption_key();
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    mac.verify_slice(expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let data = b"Hello, NetherNet!";
        let encrypted = encrypt(data).unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(data.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_checksum() {
        let data = b"Test data for checksum";
        let checksum = compute_checksum(data);
        assert!(verify_checksum(data, &checksum));

        let wrong_checksum = [0u8; 32];
        assert!(!verify_checksum(data, &wrong_checksum));
    }

    #[test]
    fn test_encryption_key() {
        // The key should be deterministic
        let key1 = encryption_key();
        let key2 = encryption_key();
        assert_eq!(key1, key2);
    }
}
