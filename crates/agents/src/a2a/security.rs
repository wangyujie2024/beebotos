use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use p256::{EncodedPoint, PublicKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub struct A2ASecurity {
    signing_key: SigningKey,
    trusted_keys: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub timestamp: u64,
}

/// Encrypted message using ECIES-like scheme
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMessage {
    /// Ephemeral public key for key derivation
    pub ephemeral_public_key: Vec<u8>,
    /// AES-GCM ciphertext
    pub ciphertext: Vec<u8>,
    /// AES-GCM nonce
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SecurityError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Expired message")]
    ExpiredMessage,
    #[error("Untrusted key")]
    UntrustedKey,
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("No trusted keys configured")]
    NoTrustedKeysConfigured,
    #[error("Invalid public key")]
    InvalidPublicKey,
}

impl A2ASecurity {
    pub fn generate_key_pair() -> Result<Self, SecurityError> {
        let signing_key = SigningKey::random(&mut OsRng);
        Ok(Self {
            signing_key,
            trusted_keys: vec![],
        })
    }

    /// Get the public key for this security instance
    pub fn public_key(&self) -> Vec<u8> {
        self.signing_key
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec()
    }

    pub fn sign_message(
        &self,
        payload: &[u8],
        timestamp: u64,
    ) -> Result<SignedMessage, SecurityError> {
        let signature: Signature = self.signing_key.sign(payload);

        let verifying_key = self.signing_key.verifying_key();
        let public_key_bytes = verifying_key.to_encoded_point(false).as_bytes().to_vec();

        Ok(SignedMessage {
            payload: payload.to_vec(),
            signature: signature.to_bytes().to_vec(),
            public_key: public_key_bytes,
            timestamp,
        })
    }

    /// Sign arbitrary data and return just the signature
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SecurityError> {
        let signature: Signature = self.signing_key.sign(data);
        Ok(signature.to_bytes().to_vec())
    }

    /// Verify a signature against data and a public key
    pub fn verify_signature(
        &self,
        data: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), SecurityError> {
        let encoded_point =
            EncodedPoint::from_bytes(public_key).map_err(|_e| SecurityError::InvalidPublicKey)?;
        let verifying_key = VerifyingKey::from_encoded_point(&encoded_point)
            .map_err(|_| SecurityError::InvalidPublicKey)?;

        let sig = Signature::from_slice(signature).map_err(|_| SecurityError::InvalidSignature)?;

        verifying_key
            .verify(data, &sig)
            .map_err(|_| SecurityError::InvalidSignature)
    }

    pub fn verify_message(&self, signed_msg: &SignedMessage) -> Result<bool, SecurityError> {
        if self.trusted_keys.is_empty() {
            return Err(SecurityError::NoTrustedKeysConfigured);
        }

        if !self
            .trusted_keys
            .iter()
            .any(|k| k == &signed_msg.public_key)
        {
            return Err(SecurityError::UntrustedKey);
        }

        let encoded_point = EncodedPoint::from_bytes(&signed_msg.public_key)
            .map_err(|e| SecurityError::DecryptionFailed(format!("{:?}", e)))?;
        let verifying_key = VerifyingKey::from_encoded_point(&encoded_point)
            .map_err(|e| SecurityError::DecryptionFailed(format!("{:?}", e)))?;

        let signature = Signature::from_slice(&signed_msg.signature)
            .map_err(|_| SecurityError::InvalidSignature)?;

        verifying_key
            .verify(&signed_msg.payload, &signature)
            .map_err(|_| SecurityError::InvalidSignature)?;

        let current_time = chrono::Utc::now().timestamp() as u64;
        if current_time - signed_msg.timestamp > 300 {
            return Err(SecurityError::ExpiredMessage);
        }

        Ok(true)
    }

    /// Encrypt data for a specific recipient using their public key
    ///
    /// Uses an ECIES-like scheme:
    /// 1. Generate ephemeral key pair
    /// 2. Derive shared secret using ECDH
    /// 3. Use HKDF to derive AES-256-GCM key
    /// 4. Encrypt with AES-256-GCM
    pub fn encrypt_for_recipient(
        &self,
        plaintext: &[u8],
        recipient_public_key: &[u8],
    ) -> Result<EncryptedMessage, SecurityError> {
        // Parse recipient public key
        let encoded_point = EncodedPoint::from_bytes(recipient_public_key)
            .map_err(|_| SecurityError::InvalidPublicKey)?;
        let recipient_key: PublicKey = Option::from(PublicKey::from_encoded_point(&encoded_point))
            .ok_or(SecurityError::InvalidPublicKey)?;

        // Generate ephemeral key pair
        let ephemeral_secret = p256::SecretKey::random(&mut OsRng);
        let ephemeral_public = ephemeral_secret.public_key();

        // Derive shared secret (ECDH)
        let shared_secret = p256::ecdh::diffie_hellman(
            ephemeral_secret.to_nonzero_scalar(),
            recipient_key.as_affine(),
        );

        // Use HKDF to derive AES key
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
        let mut aes_key = [0u8; 32];
        hkdf.expand(b"a2a-encryption-v1", &mut aes_key)
            .map_err(|e| SecurityError::EncryptionFailed(e.to_string()))?;

        // Encrypt with AES-256-GCM
        // SECURITY FIX: Use random nonce for each encryption
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&aes_key));
        let nonce_bytes = rand::random::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SecurityError::EncryptionFailed(e.to_string()))?;

        Ok(EncryptedMessage {
            ephemeral_public_key: ephemeral_public.to_encoded_point(false).as_bytes().to_vec(),
            ciphertext,
            nonce: nonce_bytes.to_vec(),
        })
    }

    /// Decrypt data encrypted for us
    ///
    /// Uses our private key to derive the shared secret and decrypt
    pub fn decrypt(&self, encrypted: &EncryptedMessage) -> Result<Vec<u8>, SecurityError> {
        // Parse ephemeral public key
        let encoded_point = EncodedPoint::from_bytes(&encrypted.ephemeral_public_key)
            .map_err(|_| SecurityError::InvalidPublicKey)?;
        let ephemeral_public: PublicKey =
            Option::from(PublicKey::from_encoded_point(&encoded_point))
                .ok_or(SecurityError::InvalidPublicKey)?;

        // Derive shared secret using our private key
        let shared_secret = p256::ecdh::diffie_hellman(
            self.signing_key.as_nonzero_scalar(),
            ephemeral_public.as_affine(),
        );

        // Derive AES key
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.raw_secret_bytes());
        let mut aes_key = [0u8; 32];
        hkdf.expand(b"a2a-encryption-v1", &mut aes_key)
            .map_err(|e| SecurityError::DecryptionFailed(e.to_string()))?;

        // Decrypt
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&aes_key));
        let nonce = Nonce::from_slice(&encrypted.nonce);
        let plaintext = cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|e| SecurityError::DecryptionFailed(e.to_string()))?;

        Ok(plaintext)
    }

    pub fn add_trusted_key(&mut self, key: Vec<u8>) {
        self.trusted_keys.push(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let alice = A2ASecurity::generate_key_pair().unwrap();
        let bob = A2ASecurity::generate_key_pair().unwrap();

        let plaintext = b"Hello, secure world!";
        let bob_public_key = bob.public_key();

        // Alice encrypts for Bob
        let encrypted = alice
            .encrypt_for_recipient(plaintext, &bob_public_key)
            .unwrap();

        // Bob decrypts
        let decrypted = bob.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_signature_verification() {
        let security = A2ASecurity::generate_key_pair().unwrap();
        let data = b"Test message";

        let signature = security.sign(data).unwrap();
        let public_key = security.public_key();

        // Verify with correct key
        assert!(security
            .verify_signature(data, &signature, &public_key)
            .is_ok());

        // Verify with wrong data should fail
        assert!(security
            .verify_signature(b"Wrong data", &signature, &public_key)
            .is_err());
    }
}
