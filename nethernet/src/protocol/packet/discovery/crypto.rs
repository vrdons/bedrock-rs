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

/// Pre-initialized AES-256 cipher for encryption and decryption.
static CIPHER: LazyLock<Aes256> = LazyLock::new(|| Aes256::new(ENCRYPTION_KEY.as_slice().into()));

/// Pre-initialized HMAC-SHA256 state to avoid re-computing the key schedule.
static HMAC_STATE: LazyLock<Hmac<Sha256>> = LazyLock::new(|| {
    <Hmac<Sha256> as Mac>::new_from_slice(ENCRYPTION_KEY.as_slice())
        .expect("HMAC can take key of any size")
});

/// Encrypts the given bytes using AES-256 in ECB mode with PKCS#7 padding.
///
/// The input is padded to a 16-byte boundary and each block is encrypted in place; the returned
/// [`Vec<u8>`] contains the ciphertext whose length is a multiple of 16 bytes.
pub(crate) fn encrypt(data: &[u8]) -> Result<Vec<u8>> {
    // Apply PKCS7 padding
    let block_size = 16;
    let padding_len = block_size - (data.len() % block_size);
    let mut padded = Vec::with_capacity(data.len() + padding_len);
    padded.extend_from_slice(data);
    padded.resize(data.len() + padding_len, padding_len as u8);

    // Encrypt blocks in parallel (if supported by hardware)
    // Safety: GenericArray/Block is repr(transparent) over [u8; 16]
    let blocks = unsafe {
        std::slice::from_raw_parts_mut(
            padded.as_mut_ptr() as *mut Block<Aes256>,
            padded.len() / block_size,
        )
    };
    CIPHER.encrypt_blocks(blocks);

    Ok(padded)
}

/// Decrypts a byte slice that was encrypted with AES-256-ECB and PKCS#7 padding.
///
/// Returns the decrypted plaintext on success. Returns an error if the input length is zero or not a multiple of 16, or if PKCS#7 padding is invalid.
pub(crate) fn decrypt(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() || data.len() % 16 != 0 {
        return Err(NethernetError::Other(
            "Invalid encrypted data length".to_string(),
        ));
    }

    // Decrypt blocks in parallel (if supported by hardware)
    let mut decrypted = data.to_vec();
    // Safety: GenericArray/Block is repr(transparent) over [u8; 16]
    let blocks = unsafe {
        std::slice::from_raw_parts_mut(
            decrypted.as_mut_ptr() as *mut Block<Aes256>,
            decrypted.len() / 16,
        )
    };
    CIPHER.decrypt_blocks(blocks);

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
pub(crate) fn compute_checksum(data: &[u8]) -> [u8; 32] {
    let mut mac = HMAC_STATE.clone();
    mac.update(data);
    let result = mac.finalize();
    result.into_bytes().into()
}

/// Verifies that `data` matches the given HMAC-SHA256 `expected` checksum using the module's encryption key.
///
/// # Returns
///
/// `true` if `expected` matches the HMAC-SHA256 of `data`, `false` otherwise.
pub(crate) fn verify_checksum(data: &[u8], expected: &[u8; 32]) -> bool {
    let mut mac = HMAC_STATE.clone();
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

}
