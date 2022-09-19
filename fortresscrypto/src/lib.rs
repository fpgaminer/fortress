#[macro_use]
mod newtype_macros;
mod error;
mod siv;

use byteorder::{LittleEndian, ReadBytesExt};
use error::CryptoError;
use hmac::{digest::CtOutput, Hmac, Mac};
use rand::{OsRng, Rng};
use sha2::{Digest, Sha256};
use siv::{SivEncryptionKeys, SIV};
use std::{
	io::{self, BufRead, Cursor, Read, Write},
	str,
};


new_type!(secret Key(32););
new_type!(public MacTag(32););
new_type!(secret LoginKey(32););


// For cloud storage, we want the user's password to be _extremely_ hard to crack.
// Since we don't care how long it initially takes to generate (e.g. the user won't notice it taking 5 minutes to initially sync to the cloud), we can use
// really big scrypt parameters.
// With the parameters set this way it should cost an attacker ~$50 million to crack a user's weak password using rented compute power (assuming a random, 8 character all lowercase password consisting of only a-z).
// It should take the average computer ~5 minutes to derive (less if more cores are used).
// NOTE: In debug mode, we use smaller parameters since debug builds are only for testing and run very slow.
#[cfg(debug_assertions)]
const NETWORK_SCRYPT_LOG_N: u8 = 14;
#[cfg(not(debug_assertions))]
const NETWORK_SCRYPT_LOG_N: u8 = 20;

const NETWORK_SCRYPT_R: u32 = 8;

#[cfg(debug_assertions)]
const NETWORK_SCRYPT_P: u32 = 1;
#[cfg(not(debug_assertions))]
const NETWORK_SCRYPT_P: u32 = 128;

// Fixed key used to derive salt from username
const NETWORK_USERNAME_SALT: Key = Key([
	0x51, 0xc3, 0xd0, 0x0b, 0xde, 0x2b, 0x32, 0x58, 0xca, 0x17, 0x92, 0x72, 0x15, 0x3e, 0xd0, 0xfd, 0x2e, 0x47, 0x56, 0x04, 0xda, 0x14, 0xba, 0xc2, 0xb7, 0xa3,
	0xb9, 0xbc, 0xb0, 0x50, 0x4f, 0xba,
]);

// Fixed key used to hash username for login (so the server doesn't know our real email)
// In case of a server breach, this makes it annoying for attackers to crack user data, because they don't know the usernames and thus can't derive the master key's salt.
const LOGIN_USERNAME_SALT: Key = Key([
	0x87, 0x65, 0x09, 0x06, 0xef, 0xda, 0x47, 0x65, 0x7a, 0x1f, 0x95, 0x36, 0x8f, 0x7a, 0xf7, 0x11, 0xc0, 0xd1, 0x0e, 0x51, 0x47, 0x35, 0x44, 0x3c, 0x0b, 0xdc,
	0xa4, 0x6e, 0x11, 0x81, 0xaa, 0xc4,
]);


pub fn hash_username_for_login(username: &[u8]) -> Vec<u8> {
	hmac(&LOGIN_USERNAME_SALT, username).into_bytes().to_vec()
}


pub struct EncryptedObject(pub Vec<u8>);


#[derive(Eq, PartialEq, Debug)]
pub struct NetworkKeySuite {
	encryption_keys: SivEncryptionKeys,
	login_key: LoginKey,
}

impl NetworkKeySuite {
	/// Derive from username and password using a very aggressive KDF.
	/// This function call will take a long time to finish (5 minutes or more).
	pub fn derive(username: &[u8], password: &[u8]) -> NetworkKeySuite {
		// Hide username behind hmac so salt is unique to this application.
		let salt = hmac(&NETWORK_USERNAME_SALT, username).into_bytes();
		let mut raw_keys = [0u8; 256 + 32];
		let scrypt_params = scrypt::Params::new(NETWORK_SCRYPT_LOG_N, NETWORK_SCRYPT_R, NETWORK_SCRYPT_P).expect("scrypt parameters should be valid");
		scrypt::scrypt(password, &salt, &scrypt_params, &mut raw_keys).expect("internal error");

		let (siv_keys, raw_keys) = raw_keys.split_at(256);
		let (login_key, _) = raw_keys.split_at(32);

		NetworkKeySuite {
			encryption_keys: SivEncryptionKeys::from_slice(siv_keys).expect("internal error"),
			login_key: LoginKey::from_slice(login_key).expect("internal error"),
		}
	}

	pub fn encrypt_object(&self, id: &[u8], data: &[u8]) -> EncryptedObject {
		let (siv, ciphertext) = self.encryption_keys.encrypt(id, data);
		EncryptedObject([siv.as_ref(), ciphertext.as_slice()].concat())
	}

	// Deterministically decrypt payload, after validating mac.  Returns plaintext.
	pub fn decrypt_object(&self, id: &[u8], encrypted_object: &EncryptedObject) -> Result<Vec<u8>, CryptoError> {
		if encrypted_object.0.len() < 32 {
			return Err(CryptoError::DecryptionError);
		}

		let (siv, ciphertext) = encrypted_object.0.split_at(32);
		let siv = SIV::from_slice(siv).expect("internal error");
		self.encryption_keys.decrypt(id, &siv, ciphertext).ok_or(CryptoError::DecryptionError)
	}
}


#[derive(Eq, PartialEq, Debug)]
pub struct FileKeySuite {
	encryption_keys: SivEncryptionKeys,
}

impl FileKeySuite {
	pub fn derive(password: &[u8], params: &EncryptionParameters) -> Result<FileKeySuite, CryptoError> {
		let mut raw_keys = [0u8; 256];

		let scrypt_params = scrypt::Params::new(params.log_n, params.r, params.p).map_err(|_| CryptoError::BadScryptParameters)?;
		scrypt::scrypt(password, &params.salt, &scrypt_params, &mut raw_keys).expect("internal error");

		Ok(FileKeySuite {
			encryption_keys: SivEncryptionKeys::from_slice(&raw_keys).expect("internal error"),
		})
	}

	fn encrypt_object(&self, data: &[u8]) -> Vec<u8> {
		let (siv, ciphertext) = self.encryption_keys.encrypt(&[], data);
		[siv.as_ref(), ciphertext.as_slice()].concat()
	}

	fn decrypt_object(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
		if data.len() < 32 {
			return Err(CryptoError::DecryptionError);
		}

		let (siv, ciphertext) = data.split_at(32);
		let siv = SIV::from_slice(siv).expect("internal error");
		self.encryption_keys.decrypt(&[], &siv, ciphertext).ok_or(CryptoError::DecryptionError)
	}
}


/// Decrypts a database stored on disk.  Returns the plaintext, EncryptionParameters that were used, and the FileKeySuite that was used.
pub fn decrypt_from_file<R: Read>(reader: &mut R, password: &[u8]) -> Result<(Vec<u8>, EncryptionParameters, FileKeySuite), CryptoError> {
	// Read file
	let mut filedata = Vec::new();
	reader.read_to_end(&mut filedata)?;

	// Check checksum
	if filedata.len() < 32 {
		return Err(CryptoError::TruncatedData);
	}

	let (filedata, checksum) = filedata.split_at(filedata.len() - 32);
	let calculated_checksum = sha256(filedata);

	if calculated_checksum != checksum {
		return Err(CryptoError::BadChecksum);
	}

	// Parse header
	let (params, payload) = parse_header(&filedata)?;

	// Derive keys
	let file_key_suite = FileKeySuite::derive(password, &params)?;

	// Decrypt
	let plaintext = file_key_suite.decrypt_object(payload)?;

	Ok((plaintext, params, file_key_suite))
}


/// Encrypts a database to disk.  Resulting file will contain a header, ciphertext, mac, and checksum.
pub fn encrypt_to_file<W: Write>(writer: &mut W, data: &[u8], params: &EncryptionParameters, key_suite: &FileKeySuite) -> io::Result<()> {
	let ciphertext = key_suite.encrypt_object(data);
	let header = build_header(params);
	let checksum = {
		let mut hash = [0u8; 32];
		let mut hasher = Sha256::new();
		hasher.update(&header);
		hasher.update(&ciphertext);
		hasher.finalize_into((&mut hash).into());
		hash
	};

	writer.write_all(&header)?;
	writer.write_all(&ciphertext)?;
	writer.write_all(&checksum)
}


fn build_header(params: &EncryptionParameters) -> Vec<u8> {
	let mut result = Vec::new();

	result.extend_from_slice(b"fortress2\0");
	result.extend_from_slice(&params.log_n.to_le_bytes());
	result.extend_from_slice(&params.r.to_le_bytes());
	result.extend_from_slice(&params.p.to_le_bytes());
	result.extend_from_slice(&params.salt);
	result
}


fn parse_header(data: &[u8]) -> Result<(EncryptionParameters, &[u8]), CryptoError> {
	let mut reader = Cursor::new(data);

	let mut header_string = Vec::new();
	reader.read_until(0, &mut header_string)?;

	// Only v2 is supported
	if str::from_utf8(&header_string).map_err(|_| CryptoError::UnsupportedVersion)? != "fortress2\0" {
		return Err(CryptoError::UnsupportedVersion);
	}

	let log_n = reader.read_u8()?;
	let r = reader.read_u32::<LittleEndian>()?;
	let p = reader.read_u32::<LittleEndian>()?;
	let mut scrypt_salt = [0u8; 32];
	reader.read_exact(&mut scrypt_salt)?;

	let pos = reader.position() as usize;

	Ok((
		EncryptionParameters {
			log_n,
			r,
			p,
			salt: scrypt_salt,
		},
		&reader.into_inner()[pos..],
	))
}


fn sha256(input: &[u8]) -> [u8; 32] {
	let mut hash = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.update(input);
	hasher.finalize_into((&mut hash).into());
	hash
}


fn hmac(key: &Key, data: &[u8]) -> CtOutput<Hmac<Sha256>> {
	let mut hmac = Hmac::<Sha256>::new_from_slice(&key[..]).expect("unexpected");
	hmac.update(data);
	hmac.finalize()
}


#[derive(Eq, PartialEq, Debug, Clone)]
pub struct EncryptionParameters {
	// Parameters for deriving file_key using scrypt
	pub log_n: u8,
	pub r: u32,
	pub p: u32,
	pub salt: [u8; 32],
}

// Default is N=18, r=8, p=1 (less N when in debug mode)
// Some sites suggested r=16 for modern systems, but I didn't see measurable benefit on my development machine.
impl Default for EncryptionParameters {
	#[cfg(debug_assertions)]
	fn default() -> EncryptionParameters {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		EncryptionParameters {
			log_n: 8,
			r: 8,
			p: 1,
			salt: rng.gen(),
		}
	}
	#[cfg(not(debug_assertions))]
	fn default() -> EncryptionParameters {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		EncryptionParameters {
			log_n: 18,
			r: 8,
			p: 1,
			salt: rng.gen(),
		}
	}
}


#[cfg(test)]
mod tests {
	use super::{decrypt_from_file, encrypt_to_file, sha256, FileKeySuite, NetworkKeySuite};
	use rand::{OsRng, Rng};
	use std::io::Cursor;

	// Basic santiy checks on NetworkKeySuite (the underlying SIV encryption is tested in the siv module)
	#[test]
	fn test_network_key_suite() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let username = "testuser";
		let password = "testpassword";
		let keys = NetworkKeySuite::derive(username.as_bytes(), password.as_bytes());
		let bad_keys = NetworkKeySuite::derive(username.as_bytes(), "badpassword".as_bytes());

		// Keys should be different if password is different
		assert_ne!(keys, bad_keys);

		// Encrypt and decrypt
		let plaintext = rng.gen_iter::<u8>().take(2017).collect::<Vec<u8>>();
		let id = rng.gen_iter::<u8>().take(32).collect::<Vec<u8>>();
		let bad_id = rng.gen_iter::<u8>().take(32).collect::<Vec<u8>>();
		let ciphertext = keys.encrypt_object(&id, &plaintext);

		assert_eq!(plaintext, keys.decrypt_object(&id, &ciphertext).unwrap());
		assert!(keys.decrypt_object(&bad_id, &ciphertext).is_err());
		assert!(bad_keys.decrypt_object(&id, &ciphertext).is_err());

		// Check that the same keys are derived from the same username and password
		assert_eq!(keys, NetworkKeySuite::derive(username.as_bytes(), password.as_bytes()));

		// Check that different keys are derived from different usernames
		assert_ne!(keys, NetworkKeySuite::derive("differentuser".as_bytes(), password.as_bytes()));
	}


	// Basic santiy checks on FileKeySuite (the underlying SIV encryption is tested in the siv module)
	#[test]
	fn test_file_key_suite() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let password = "testpassword";
		let params = Default::default();
		let keys = FileKeySuite::derive(password.as_bytes(), &params).unwrap();
		let bad_keys = FileKeySuite::derive("badpassword".as_bytes(), &params).unwrap();

		// Keys should be different if password is different
		assert_ne!(keys, bad_keys);

		// Encrypt and decrypt
		let plaintext = rng.gen_iter::<u8>().take(2017).collect::<Vec<u8>>();
		let ciphertext = keys.encrypt_object(&plaintext);

		assert_eq!(plaintext, keys.decrypt_object(&ciphertext).unwrap());
		assert!(bad_keys.decrypt_object(&ciphertext).is_err());

		// Check that the same keys are derived from the same password
		assert_eq!(keys, FileKeySuite::derive(password.as_bytes(), &params).unwrap());
	}

	// Make sure errors are thrown for the various kinds of file corruption
	#[test]
	fn file_corruption() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let payload = b"payloada";
		let password = b"password";
		let encryption_parameters = Default::default();
		let file_key_suite = FileKeySuite::derive(password, &encryption_parameters).unwrap();

		let encrypted_data = {
			let mut buffer = Vec::new();

			encrypt_to_file(&mut buffer, payload, &encryption_parameters, &file_key_suite).expect("this shouldn't fail");

			buffer
		};

		// Run tests a few times (they're random)
		for _ in 0..64 {
			let mutation_byte: u8 = rng.gen();

			let truncated = &encrypted_data[..encrypted_data.len() - rng.gen_range(1, encrypted_data.len())];
			let corrupted_checksum = {
				let mut buffer = encrypted_data.clone();
				rng.choose_mut(&mut buffer).map(|x| *x ^= mutation_byte);
				buffer
			};
			let corrupted_mac = {
				let mut data = (&encrypted_data[..encrypted_data.len() - 32]).to_owned();
				// NOTE: We don't mutate the first couple of bytes where the header is.
				// This is because it might mutate the scrypt parameters to absurd values, which
				// can cause the library to spin forever during tests.
				// TODO: This isn't ideal as we'd like to test corrupting those bits too, but not sure how.
				rng.choose_mut(&mut data[32..]).map(|x| *x ^= mutation_byte);
				let checksum = sha256(&data);
				data.extend_from_slice(&checksum);
				data
			};

			assert!(decrypt_from_file(&mut Cursor::new(truncated), password).is_err());

			// Sometimes mutation_byte is 0, which means no corruption happened.  This is a good chance to test our test.
			if mutation_byte == 0 {
				assert_eq!(
					decrypt_from_file(&mut Cursor::new(corrupted_checksum), password)
						.map(|(pt, _, _)| pt)
						.map_err(|_| ()),
					Ok(payload.to_vec())
				);
				assert_eq!(
					decrypt_from_file(&mut Cursor::new(corrupted_mac), password)
						.map(|(pt, _, _)| pt)
						.map_err(|_| ()),
					Ok(payload.to_vec())
				);
			} else {
				assert!(decrypt_from_file(&mut Cursor::new(corrupted_checksum), password).is_err());
				assert!(decrypt_from_file(&mut Cursor::new(corrupted_mac), password).is_err());
			}
		}
	}
}
