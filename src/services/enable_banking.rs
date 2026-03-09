use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result};

use crate::config::Config;

/// Encrypt a token for storage in the database.
/// Format: nonce (12 bytes) || ciphertext
pub fn encrypt_token(plaintext: &str, config: &Config) -> Result<Vec<u8>> {
    let key = Key::<Aes256Gcm>::from_slice(config.require_encryption_key()?);
    let cipher = Aes256Gcm::new(key);
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

/// Decrypt a token read from the database.
pub fn decrypt_token(data: &[u8], config: &Config) -> Result<String> {
    if data.len() < 12 {
        anyhow::bail!("encrypted data too short");
    }

    let key = Key::<Aes256Gcm>::from_slice(config.require_encryption_key()?);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(&data[..12]);
    let ciphertext = &data[12..];

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))?;

    String::from_utf8(plaintext).context("decrypted token is not valid UTF-8")
}

// TODO: Implement Enable Banking API client
// - start_authorization(bank_id) -> redirect URL
// - exchange_code(code) -> access token + expiry
// - get_transactions(access_token, account_id, date_from) -> Vec<BankTransaction>
