//! AES-256-GCM 加解密 + 常量时间比较 + HMAC 签名（SPEC §11）。
//!
//! 主密钥来自 `TODO_MASTER_KEY`（32 字节 base64），缺失拒启动（main.rs）。
//! 密文格式：`nonce(12 字节随机) || ciphertext`，直接存 `secrets.value_enc`。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use rand::RngCore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("加密失败")]
    Encrypt,
    #[error("解密失败")]
    Decrypt,
    #[error("主密钥无效：须为 32 字节 base64")]
    BadKey,
}

pub fn decode_master_key(encoded: &str) -> Result<[u8; 32], CryptoError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| CryptoError::BadKey)?;
    let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadKey)?;
    Ok(arr)
}

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Encrypt)?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < 12 {
        return Err(CryptoError::Decrypt);
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::Decrypt)
}

/// 常量时间比较：401 不区分"令牌不存在 / 不匹配"，比较本身也不泄漏长度差以外的信息。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Webhook 签名：`sha256=<hex(hmac-sha256(secret, body))>`（SPEC §10）。
pub fn hmac_sha256_hex(secret: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac: Hmac<Sha256> = Mac::new_from_slice(secret).expect("HMAC 密钥长度任意，总能构造");
    mac.update(body);
    format!("{:x}", mac.finalize().into_bytes())
}

/// 随机令牌（Admin session 用）：32 字节 base64url。
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [7u8; 32];
        let blob = encrypt(&key, b"sync-token-123").unwrap();
        assert_eq!(decrypt(&key, &blob).unwrap(), b"sync-token-123");
        assert!(decrypt(&[8u8; 32], &blob).is_err());
    }

    #[test]
    fn constant_time_eq_behaves() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn hmac_is_deterministic() {
        let a = hmac_sha256_hex(b"secret", b"body");
        let b = hmac_sha256_hex(b"secret", b"body");
        assert_eq!(a, b);
        assert_ne!(a, hmac_sha256_hex(b"other", b"body"));
    }
}
