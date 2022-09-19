use chacha20::{
	cipher::{KeyIvInit, StreamCipher},
	ChaCha20,
};
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha512;

new_type!(secret HmacKey(128););

new_type!(public SIV(32););


#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SivEncryptionKeys {
	/// Used to calculate the siv for plaintext
	siv_key: HmacKey,
	/// The cipher key
	cipher_key: HmacKey,
}

impl SivEncryptionKeys {
	pub(crate) fn encrypt(&self, aad: &[u8], plaintext: &[u8]) -> (SIV, Vec<u8>) {
		let siv = self.calculate_siv(aad, plaintext);
		let ciphertext = self.cipher(&siv, plaintext);

		(siv, ciphertext)
	}

	pub(crate) fn decrypt(&self, aad: &[u8], siv: &SIV, ciphertext: &[u8]) -> Option<Vec<u8>> {
		let plaintext = self.cipher(siv, ciphertext);
		let expected_siv = self.calculate_siv(aad, &plaintext);

		if siv != &expected_siv {
			return None;
		}

		Some(plaintext)
	}

	/// Encrypts or decrypts data using the combination of self.cipher_key and nonce.
	/// First derives an encryption key using HMAC-SHA-512 (cipher_key, nonce)
	/// and then performs ChaCha20 (derived_key, data).
	fn cipher(&self, nonce: &SIV, data: &[u8]) -> Vec<u8> {
		let mut result = data.to_vec();

		let big_key = {
			let mut hmac = Hmac::<Sha512>::new_from_slice(&self.cipher_key[..]).expect("unexpected");
			hmac.update(&nonce[..]);
			hmac.finalize().into_bytes()
		};
		let (chacha_key, chacha_nonce) = big_key.split_at(32);

		// Using slice notation here so this code panics in case we accidentally didn't derive the right size big_key
		let mut cipher = ChaCha20::new_from_slices(&chacha_key[..32], &chacha_nonce[..12]).expect("unexpected");
		cipher.apply_keystream(&mut result);
		result
	}

	/// Calculate the unique SIV for the combination of self.siv_key, aad, and plaintext.
	/// Equivalent to HMAC-SHA-512-256 (siv_key, aad || plaintext || le64(aad.length) || le64(plaintext.length))
	fn calculate_siv(&self, aad: &[u8], plaintext: &[u8]) -> SIV {
		let mut hmac = Hmac::<Sha512>::new_from_slice(&self.siv_key[..]).expect("unexpected");
		hmac.update(aad);
		hmac.update(plaintext);
		hmac.update(&u64::try_from(aad.len()).expect("length did not fit into u64").to_le_bytes());
		hmac.update(&u64::try_from(plaintext.len()).expect("length did not fit into u64").to_le_bytes());

		// Truncate to 256 bits
		SIV::from_slice(&hmac.finalize().into_bytes()[..32]).expect("unexpected")
	}

	pub(crate) fn from_slice(bs: &[u8]) -> Option<Self> {
		if bs.len() != 256 {
			return None;
		}

		let (siv_key, cipher_key) = bs.split_at(128);

		Some(Self {
			siv_key: HmacKey::from_slice(siv_key).expect("unexpected"),
			cipher_key: HmacKey::from_slice(cipher_key).expect("unexpected"),
		})
	}
}


#[cfg(test)]
mod tests {
	use crate::siv::{HmacKey, SivEncryptionKeys};
	use rand::{OsRng, Rng};

	// Test that the encryption functions are deterministic
	#[test]
	fn test_deterministic_encryption() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let keys = SivEncryptionKeys {
			siv_key: HmacKey::from_rng(&mut rng),
			cipher_key: HmacKey::from_rng(&mut rng),
		};

		let (siv1, ciphertext1) = keys.encrypt(&id, &data);
		let (siv2, ciphertext2) = keys.encrypt(&id, &data);

		let plaintext = keys.decrypt(&id, &siv1, &ciphertext1).expect("decryption failed");

		assert_eq!(plaintext, data);
		assert_eq!(ciphertext1, ciphertext2);
		assert_eq!(siv1, siv2);
	}

	// Test that the encryption functions use different keys for different inputs
	#[test]
	fn test_different_keys() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id1: [u8; 32] = rng.gen();
		let id2: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let keys = SivEncryptionKeys {
			siv_key: HmacKey::from_rng(&mut rng),
			cipher_key: HmacKey::from_rng(&mut rng),
		};

		let (siv1, ciphertext1) = keys.encrypt(&id1, &data);
		let (siv2, ciphertext2) = keys.encrypt(&id2, &data);

		assert_ne!(ciphertext1, ciphertext2);
		assert_ne!(siv1, siv2);

		// Subtly change the data
		let mut data2 = data.clone();
		data2[data.len() - 1] ^= 1;
		let (siv3, ciphertext3) = keys.encrypt(&id1, &data2);

		assert_ne!(ciphertext1, ciphertext3);
		assert_ne!(siv1, siv3);

		// If it were using the same key, the ciphertext would differ by only the last byte
		assert_ne!(&ciphertext1[..ciphertext1.len() - 1], &ciphertext3[..ciphertext3.len() - 1]);
	}

	// Make sure it is verifying integrity of the ciphertext
	#[test]
	fn test_integrity() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let keys = SivEncryptionKeys {
			siv_key: HmacKey::from_rng(&mut rng),
			cipher_key: HmacKey::from_rng(&mut rng),
		};

		let (siv, mut ciphertext) = keys.encrypt(&id, &data);

		// Make sure it is verifying the ciphertext
		*rng.choose_mut(&mut ciphertext).unwrap() ^= 1;
		assert!(keys.decrypt(&id, &siv, &ciphertext).is_none());

		// Make sure it is verifying the siv
		let mut siv2 = siv.clone();
		siv2.0[rng.gen_range(0, siv2.0.len())] ^= 1;
		assert!(keys.decrypt(&id, &siv2, &ciphertext).is_none());
	}

	// Make sure it is verifying the id
	#[test]
	fn test_id() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id: [u8; 32] = rng.gen();
		let bad_id: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let keys = SivEncryptionKeys {
			siv_key: HmacKey::from_rng(&mut rng),
			cipher_key: HmacKey::from_rng(&mut rng),
		};

		let (siv, ciphertext) = keys.encrypt(&id, &data);

		assert!(keys.decrypt(&bad_id, &siv, &ciphertext).is_none());
	}
}
