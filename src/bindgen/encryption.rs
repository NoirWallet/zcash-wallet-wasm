// use crate::Error;
// use age::secrecy::SecretString;
// use base64::{Engine, prelude::BASE64_STANDARD};
// use wasm_bindgen::prelude::*;

// /// Encrypt a string using age with a passphrase
// ///
// /// # Arguments
// ///
// /// * `plaintext` - The string to encrypt
// /// * `passphrase` - The passphrase to use for encryption
// ///
// /// # Returns
// ///
// /// Base64-encoded encrypted data
// ///
// /// # Example
// ///
// /// ```javascript
// /// const encrypted = encrypt_string("Hello, World!", "my-secret-password");
// /// console.log(encrypted); // Base64-encoded ciphertext
// /// ```
// #[wasm_bindgen]
// pub fn encrypt_string(plaintext: &str, passphrase: &str) -> Result<String, Error> {
//     let passphrase = SecretString::from(passphrase);
//     let recipient = age::scrypt::Recipient::new(passphrase.clone());
//     let encrypted = age::encrypt(&recipient, plaintext.as_bytes())
//         .map_err(|e| Error::Encryption(e.to_string()))?;

//     // Return base64-encoded encrypted data
//     Ok(BASE64_STANDARD.encode(encrypted))
// }

// /// Decrypt a string using age with a passphrase
// ///
// /// # Arguments
// ///
// /// * `ciphertext` - Base64-encoded encrypted data
// /// * `passphrase` - The passphrase to use for decryption
// ///
// /// # Returns
// ///
// /// The decrypted plaintext string
// ///
// /// # Example
// ///
// /// ```javascript
// /// const decrypted = decrypt_string(encrypted, "my-secret-password");
// /// console.log(decrypted); // "Hello, World!"
// /// ```
// #[wasm_bindgen]
// pub fn decrypt_string(ciphertext: &str, passphrase: &str) -> Result<String, Error> {
//     // Decode base64
//     let encrypted = BASE64_STANDARD
//         .decode(ciphertext)
//         .map_err(|e| Error::Decryption(format!("Failed to decode base64: {}", e)))?;

//     let passphrase = SecretString::from(passphrase);
//     let identity = age::scrypt::Identity::new(passphrase);
//     let decrypted = age::decrypt(&identity, &encrypted)
//         .map_err(|e| Error::Decryption(format!("Failed to decrypt (wrong passphrase?): {}", e)))?;

//     String::from_utf8(decrypted)
//         .map_err(|e| Error::Decryption(format!("Decrypted data is not valid UTF-8: {}", e)))
// }
