use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::Result;
use rand::Rng;

const NONCE_SIZE: usize = 12;

pub struct SecretEncryptor {
    cipher: Aes256Gcm,
}

impl SecretEncryptor {
    pub fn new(master_key: &[u8]) -> Result<Self> {
        if master_key.len() != 32 {
            return Err(anyhow::anyhow!(
                "Master key must be 32 bytes, got {}",
                master_key.len()
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|err| anyhow::anyhow!("Invalid master key length: {:?}", err))?;

        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let mut ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|err| anyhow::anyhow!("Failed to encrypt payload: {:?}", err))?;
        let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.append(&mut ciphertext);
        Ok(output)
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < NONCE_SIZE {
            return Err(anyhow::anyhow!("Ciphertext is too short"));
        }

        let (nonce_bytes, payload) = ciphertext.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, payload)
            .map_err(|err| anyhow::anyhow!("Failed to decrypt payload: {:?}", err))?;
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [0xAB; 32]
    }

    #[test]
    fn roundtrip() {
        let encryptor = SecretEncryptor::new(&test_key()).unwrap();
        let plaintext = b"hello world";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_size_31() {
        let key = [0u8; 31];
        let result = SecretEncryptor::new(&key);
        let err = result.err().expect("should fail with 31-byte key");
        let msg = err.to_string();
        assert!(
            msg.contains("32"),
            "error should mention expected size 32: {msg}"
        );
    }

    #[test]
    fn wrong_key_size_33() {
        let key = [0u8; 33];
        let result = SecretEncryptor::new(&key);
        let err = result.err().expect("should fail with 33-byte key");
        let msg = err.to_string();
        assert!(
            msg.contains("32"),
            "error should mention expected size 32: {msg}"
        );
    }

    #[test]
    fn tampered_ciphertext() {
        let encryptor = SecretEncryptor::new(&test_key()).unwrap();
        let plaintext = b"sensitive data";
        let mut ciphertext = encryptor.encrypt(plaintext).unwrap();

        // Flip a byte in the authenticated ciphertext portion (after the nonce)
        let idx = NONCE_SIZE + 1;
        assert!(ciphertext.len() > idx, "ciphertext too short to tamper");
        ciphertext[idx] ^= 0xFF;

        let result = encryptor.decrypt(&ciphertext);
        assert!(
            result.is_err(),
            "decrypting tampered ciphertext should fail"
        );
    }

    #[test]
    fn different_key_decrypt() {
        let key_a = [0x11; 32];
        let key_b = [0x22; 32];
        let encryptor_a = SecretEncryptor::new(&key_a).unwrap();
        let encryptor_b = SecretEncryptor::new(&key_b).unwrap();

        let ciphertext = encryptor_a.encrypt(b"secret").unwrap();
        let result = encryptor_b.decrypt(&ciphertext);
        assert!(
            result.is_err(),
            "decrypting with a different key should fail"
        );
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let encryptor = SecretEncryptor::new(&test_key()).unwrap();
        let plaintext: &[u8] = b"";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        // Ciphertext should still contain nonce + auth tag even for empty plaintext
        assert!(ciphertext.len() > NONCE_SIZE);
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn nonce_uniqueness() {
        let encryptor = SecretEncryptor::new(&test_key()).unwrap();
        let plaintext = b"same input twice";
        let ct1 = encryptor.encrypt(plaintext).unwrap();
        let ct2 = encryptor.encrypt(plaintext).unwrap();
        assert_ne!(
            ct1, ct2,
            "encrypting the same plaintext twice should produce different ciphertexts due to random nonces"
        );
    }
}
