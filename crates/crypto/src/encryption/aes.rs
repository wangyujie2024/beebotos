use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};

use super::{EncryptedData, EncryptionAlgorithm, EncryptionError, EncryptionScheme};

pub struct AES256GCMScheme {
    cipher: Aes256Gcm,
}

impl AES256GCMScheme {
    pub fn new(key: &[u8]) -> Result<Self, EncryptionError> {
        if key.len() != 32 {
            return Err(EncryptionError::InvalidKey);
        }

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKey)?;

        Ok(Self { cipher })
    }

    fn generate_nonce() -> Vec<u8> {
        use rand::RngCore;
        let mut nonce = vec![0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        nonce
    }
}

impl EncryptionScheme for AES256GCMScheme {
    fn encrypt(
        &self,
        plaintext: &[u8],
        associated_data: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        let nonce = Self::generate_nonce();
        let nonce_slice = Nonce::from_slice(&nonce);

        let payload = aes_gcm::aead::Payload {
            msg: plaintext,
            aad: associated_data.unwrap_or(&[]),
        };

        let ciphertext = self
            .cipher
            .encrypt(nonce_slice, payload)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        Ok(EncryptedData {
            ciphertext,
            nonce,
            algorithm: EncryptionAlgorithm::AES256GCM,
        })
    }

    fn decrypt(
        &self,
        data: &EncryptedData,
        associated_data: Option<&[u8]>,
    ) -> Result<Vec<u8>, EncryptionError> {
        if data.algorithm != EncryptionAlgorithm::AES256GCM {
            return Err(EncryptionError::UnsupportedAlgorithm);
        }

        let nonce = Nonce::from_slice(&data.nonce);

        let payload = aes_gcm::aead::Payload {
            msg: &data.ciphertext,
            aad: associated_data.unwrap_or(&[]),
        };

        self.cipher
            .decrypt(nonce, payload)
            .map_err(|_| EncryptionError::DecryptionFailed)
    }

    fn algorithm(&self) -> EncryptionAlgorithm {
        EncryptionAlgorithm::AES256GCM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_encryption() {
        let key = vec![0u8; 32];
        let scheme = AES256GCMScheme::new(&key).unwrap();

        let plaintext = b"Hello, World!";
        let encrypted = scheme.encrypt(plaintext, None).unwrap();

        assert_eq!(encrypted.algorithm, EncryptionAlgorithm::AES256GCM);
        assert!(!encrypted.ciphertext.is_empty());

        let decrypted = scheme.decrypt(&encrypted, None).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
