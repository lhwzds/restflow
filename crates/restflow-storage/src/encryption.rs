use anyhow::Result;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;

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

        let mut key = [0u8; 32];
        key.copy_from_slice(master_key);
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|err| anyhow::anyhow!("Invalid master key length: {:?}", err))?;

        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
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
