/// Feature 12: `encrypt(data)` / `decrypt(data)` — AES-256-GCM E2E encryption
///
/// Key derived from env var ENCRYPT_KEY via SHA-256 if not exactly 32 bytes.
/// Output: base64(nonce + ciphertext)

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use aes_gcm::aead::rand_core::RngCore;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use sha2::{Sha256, Digest};

use crate::error::{GravError, GravResult};
use crate::value::Value;

/// Derive a 32-byte key from the ENCRYPT_KEY env var using SHA-256.
fn get_key() -> [u8; 32] {
    let raw = std::env::var("ENCRYPT_KEY").unwrap_or_else(|_| "default-insecure-key".to_string());
    let bytes = raw.as_bytes();
    if bytes.len() == 32 {
        let mut k = [0u8; 32];
        k.copy_from_slice(bytes);
        k
    } else {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        let mut k = [0u8; 32];
        k.copy_from_slice(&result);
        k
    }
}

/// AES-256-GCM encrypt.  Returns base64(12-byte nonce + ciphertext).
pub fn encrypt(plaintext: &str) -> GravResult<String> {
    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| GravError::Runtime(format!("encrypt: key error: {e}")))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| GravError::Runtime(format!("encrypt: aes-gcm error: {e}")))?;

    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(B64.encode(combined))
}

/// AES-256-GCM decrypt.  Input: base64(12-byte nonce + ciphertext).
pub fn decrypt(encoded: &str) -> GravResult<String> {
    let key = get_key();
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| GravError::Runtime(format!("decrypt: key error: {e}")))?;

    let combined = B64.decode(encoded)
        .map_err(|e| GravError::Runtime(format!("decrypt: base64 error: {e}")))?;

    if combined.len() < 12 {
        return Err(GravError::Runtime("decrypt: ciphertext too short".into()));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| GravError::Runtime(format!("decrypt: decryption failed: {e}")))?;

    String::from_utf8(plaintext)
        .map_err(|e| GravError::Runtime(format!("decrypt: utf8 error: {e}")))
}

/// Dispatch `encrypt` and `decrypt` builtins.
pub fn call_crypto_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    match name {
        "encrypt" => {
            let data = args.first().map(|v| v.to_string()).unwrap_or_default();
            let result = encrypt(&data)?;
            Ok(Some(Value::make_str(result)))
        }
        "decrypt" => {
            let data = args.first().map(|v| v.to_string()).unwrap_or_default();
            let result = decrypt(&data)?;
            Ok(Some(Value::make_str(result)))
        }
        _ => Ok(None),
    }
}
