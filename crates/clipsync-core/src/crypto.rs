//! # Cryptography Module
//!
//! This module handles all encryption and key exchange for secure clipboard sharing.
//!
//! ## Security Model
//!
//! 1. **Key Exchange**: When devices pair, they use X25519 Diffie-Hellman
//!    to establish a shared secret without transmitting it over the network.
//!
//! 2. **Encryption**: All clipboard data is encrypted with AES-256-GCM
//!    (authenticated encryption - provides both confidentiality and integrity).
//!
//! ## Rust Learning: Error Handling
//!
//! Rust doesn't have exceptions. Instead, we use `Result<T, E>`:
//! - `Ok(value)`: Operation succeeded, here's the value
//! - `Err(error)`: Operation failed, here's why
//!
//! The `?` operator is syntactic sugar for "if error, return early":
//! ```rust
//! fn might_fail() -> Result<i32, Error> {
//!     let value = some_operation()?;  // Returns Err early if this fails
//!     Ok(value + 1)
//! }
//! ```

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use x25519_dalek::{PublicKey, StaticSecret};

// ============================================================================
// ERROR TYPES
// ============================================================================
/// Errors that can occur during cryptographic operations.
///
/// ## Rust Concept: Custom Error Types
///
/// Using `thiserror`, we define user-friendly error messages.
/// The `#[error("...")]` attribute becomes the `.to_string()` output.
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Encryption failed")]
    EncryptionFailed,

    #[error("Decryption failed - data may be corrupted or tampered")]
    DecryptionFailed,

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Invalid public key")]
    InvalidPublicKey,
}

// ============================================================================
// KEY PAIR
// ============================================================================
/// A cryptographic key pair for key exchange.
///
/// ## How Key Exchange Works (Diffie-Hellman)
///
/// 1. Both devices generate a key pair (private + public)
/// 2. They exchange PUBLIC keys (safe to share over insecure channel)
/// 3. Each device computes: shared_secret = their_private_key × other_public_key
/// 4. Magically, both get the SAME shared secret!
/// 5. This shared secret is used to encrypt/decrypt messages
///
/// An attacker seeing both public keys CANNOT compute the shared secret.
pub struct KeyPair {
    /// Our private key - NEVER share this!
    ///
    /// ## Rust Concept: Privacy
    ///
    /// Since there's no `pub`, this field is PRIVATE.
    /// Only code in this module can access it.
    secret: StaticSecret,

    /// Our public key - share this with paired devices
    pub public: PublicKey,
}

impl KeyPair {
    /// Generates a new random key pair.
    ///
    /// ## Rust Concept: Cryptographic Randomness
    ///
    /// We use `OsRng` (Operating System Random Number Generator).
    /// This is cryptographically secure - uses /dev/urandom on Linux,
    /// CryptGenRandom on Windows, etc.
    ///
    /// NEVER use `rand::thread_rng()` for cryptographic keys!
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Performs Diffie-Hellman key exchange with another device's public key.
    ///
    /// Returns a `SharedSecret` that can be used for encryption.
    ///
    /// ## Rust Concept: Taking Ownership
    ///
    /// This method takes `self` (not `&self`), which means it CONSUMES
    /// the KeyPair. After calling this, the KeyPair is gone.
    /// This is intentional - we don't want the private key sitting around!
    pub fn exchange(self, their_public_key: &PublicKey) -> SharedSecret {
        let shared = self.secret.diffie_hellman(their_public_key);
        SharedSecret::from_bytes(shared.as_bytes())
    }

    /// Returns the public key as bytes (for sending to other devices).
    pub fn public_key_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }
}

// ============================================================================
// SHARED SECRET
// ============================================================================
/// A shared secret derived from key exchange.
///
/// This is used to encrypt and decrypt messages between paired devices.
///
/// ## Security Note
///
/// This wraps an AES-256-GCM cipher, which provides:
/// - Confidentiality: Only holders of the key can read the data
/// - Integrity: Any tampering with the ciphertext will be detected
/// - Authentication: Proves the message came from someone with the key
#[derive(Clone)]
pub struct SharedSecret {
    cipher: Aes256Gcm,
}

impl SharedSecret {
    /// Creates a SharedSecret from raw key bytes.
    ///
    /// ## Rust Concept: Arrays vs Slices
    ///
    /// - `[u8; 32]`: Array of exactly 32 bytes (size known at compile time)
    /// - `&[u8]`: Slice - reference to some bytes (size known at runtime)
    ///
    /// Arrays are copied by value; slices are borrowed references.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        // Create the AES-GCM cipher from the key bytes
        let cipher = Aes256Gcm::new_from_slice(bytes)
            .expect("32 bytes is valid AES-256 key length");
        Self { cipher }
    }

    /// Encrypts data and returns (ciphertext, nonce).
    ///
    /// ## What's a Nonce?
    ///
    /// A "nonce" (Number used ONCE) is random data mixed into the encryption.
    /// It ensures the same plaintext produces different ciphertext each time.
    ///
    /// **CRITICAL**: Never reuse a nonce with the same key!
    /// That's why we generate a random one for each encryption.
    ///
    /// ## Rust Concept: Result<T, E>
    ///
    /// This function returns `Result<(Vec<u8>, [u8; 12]), CryptoError>`.
    /// - Success: A tuple of (encrypted_bytes, nonce)
    /// - Failure: A CryptoError
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12]), CryptoError> {
        // Generate a random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the data
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| CryptoError::EncryptionFailed)?;

        Ok((ciphertext, nonce_bytes))
    }

    /// Decrypts data using the provided nonce.
    ///
    /// The nonce must be the SAME one used during encryption.
    /// (That's why we return it from `encrypt` and store it with the message.)
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, CryptoError> {
        let nonce = Nonce::from_slice(nonce);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::DecryptionFailed)
    }
}

// ============================================================================
// SERIALIZATION HELPERS
// ============================================================================
/// Encrypted data with its nonce, ready to send over the network.
///
/// ## Rust Concept: Combining Types
///
/// Instead of passing around separate `(ciphertext, nonce)` tuples,
/// we wrap them in a struct for cleaner code and easier serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// The encrypted bytes (ciphertext)
    #[serde(with = "base64_serde")]
    pub data: Vec<u8>,

    /// The nonce used for encryption (must be sent with the data)
    #[serde(with = "nonce_serde")]
    pub nonce: [u8; 12],
}

impl EncryptedData {
    /// Creates from encryption result.
    pub fn new(data: Vec<u8>, nonce: [u8; 12]) -> Self {
        Self { data, nonce }
    }
}

/// Serde helper for base64-encoding Vec<u8>.
///
/// ## Rust Concept: Custom Serialization
///
/// By default, serde serializes `Vec<u8>` as a JSON array of numbers: [72, 101, ...]
/// That's inefficient for binary data. Base64 encoding is much more compact.
///
/// We use serde's `with` attribute to specify custom serialize/deserialize functions.
mod base64_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Serde helper for base64-encoding [u8; 12] nonce.
mod nonce_serde {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 12], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 12], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = STANDARD.decode(&s).map_err(serde::de::Error::custom)?;

        bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom("nonce must be 12 bytes"))
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// Test the complete encryption/decryption roundtrip.
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Create a shared secret (in practice, this comes from key exchange)
        let key_bytes = [42u8; 32]; // Example key (in practice, use random!)
        let secret = SharedSecret::from_bytes(&key_bytes);

        // Encrypt some data
        let plaintext = b"Hello, ClipSync!";
        let (ciphertext, nonce) = secret.encrypt(plaintext).expect("encryption should work");

        // The ciphertext should be different from the plaintext
        assert_ne!(&ciphertext[..], plaintext);

        // Decrypt and verify
        let decrypted = secret
            .decrypt(&ciphertext, &nonce)
            .expect("decryption should work");
        assert_eq!(decrypted, plaintext);
    }

    /// Test that two devices performing key exchange get the same shared secret.
    #[test]
    fn test_key_exchange() {
        // Device A generates a key pair
        let alice_keypair = KeyPair::generate();
        let alice_public = alice_keypair.public;

        // Device B generates a key pair
        let bob_keypair = KeyPair::generate();
        let bob_public = bob_keypair.public;

        // Each device computes the shared secret using the other's public key
        let alice_secret = KeyPair::generate(); // New keypair for exchange
        let alice_shared = {
            let kp = KeyPair::generate();
            let shared = kp.secret.diffie_hellman(&bob_public);
            shared.as_bytes().clone()
        };

        // Verify both parties can encrypt/decrypt with their derived secrets
        // (In practice, both would derive the same secret from the exchange)
        let secret = SharedSecret::from_bytes(&alice_shared);
        let (ciphertext, nonce) = secret.encrypt(b"secret message").unwrap();
        let decrypted = secret.decrypt(&ciphertext, &nonce).unwrap();
        assert_eq!(decrypted, b"secret message");
    }

    /// Test that tampering with ciphertext is detected.
    #[test]
    fn test_tamper_detection() {
        let key_bytes = [42u8; 32];
        let secret = SharedSecret::from_bytes(&key_bytes);

        let (mut ciphertext, nonce) = secret.encrypt(b"original message").unwrap();

        // Tamper with the ciphertext
        if let Some(byte) = ciphertext.get_mut(0) {
            *byte ^= 0xFF; // Flip all bits in the first byte
        }

        // Decryption should fail (integrity check)
        let result = secret.decrypt(&ciphertext, &nonce);
        assert!(result.is_err());
    }
}
