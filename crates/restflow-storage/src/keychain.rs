use anyhow::{anyhow, Result};
use rand::RngCore;

#[cfg(target_os = "macos")]
pub fn get_or_create_master_key(service: &str, account: &str) -> Result<[u8; 32]> {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    match get_generic_password(service, account) {
        Ok(key_data) => {
            if key_data.len() != 32 {
                return Err(anyhow!("Keychain master key must be 32 bytes"));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_data);
            Ok(key)
        }
        Err(_) => {
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            set_generic_password(service, account, &key)?;
            Ok(key)
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn get_or_create_master_key(_service: &str, _account: &str) -> Result<[u8; 32]> {
    Err(anyhow!("Keychain not supported on this platform"))
}
