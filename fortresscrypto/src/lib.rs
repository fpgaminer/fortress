extern crate byteorder;
extern crate crypto;
extern crate rand;
extern crate data_encoding;
extern crate serde;

#[macro_use] mod newtype_macros;

use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use crypto::{scrypt, chacha20};
use crypto::digest::Digest;
use crypto::hmac::Hmac;
use crypto::mac::{Mac, MacResult};
use crypto::sha2::Sha256;
use crypto::symmetriccipher::SynchronousStreamCipher;
use rand::{OsRng, Rng};
use std::io::{self, Cursor, Write, BufRead, Read};
use std::str;


new_type!{
	secret Key(32);
}

new_type!{
	public MacTag(32);
}


// The MasterKey is used to derive the keys used for cloud storage.  Because of this, we want it to be _extremely_ hard to crack.
// Since we don't care how long it initially takes to generate (e.g. the user won't notice it taking 5 minutes to initially sync to the cloud), we can use
// really big scrypt parameters.
// With the parameters set this way it should cost an attacker ~$50 million to crack a user's weak password using rented compute power (assuming a random, 8 character all lowercase password consisting of only a-z).
// It should take the average computer ~5 minutes to derive (less if more cores are used).
// NOTE: In debug mode, we use smaller parameters since debug builds are only for testing and run very slow.
#[cfg(debug_assertions)]
const MASTER_KEY_SCRYPT_LOG_N: u8 = 14;
#[cfg(not(debug_assertions))]
const MASTER_KEY_SCRYPT_LOG_N: u8 = 20;

const MASTER_KEY_SCRYPT_R: u32 = 8;

#[cfg(debug_assertions)]
const MASTER_KEY_SCRYPT_P: u32 = 1;
#[cfg(not(debug_assertions))]
const MASTER_KEY_SCRYPT_P: u32 = 128;

// Fixed key used to derive salt from username for master key's scrypt parameters
const MASTER_KEY_USERNAME_SALT: Key = Key([0x51,0xc3,0xd0,0x0b,0xde,0x2b,0x32,0x58,0xca,0x17,0x92,0x72,0x15,0x3e,0xd0,0xfd,0x2e,0x47,0x56,0x04,0xda,0x14,0xba,0xc2,0xb7,0xa3,0xb9,0xbc,0xb0,0x50,0x4f,0xba]);

// Fixed key used to hash username for login (so the server doesn't know our real email)
// In case of a server breach, this makes it annoying for attackers to crack user data, because they don't know the usernames and thus can't derive the master key's salt.
const LOGIN_USERNAME_SALT: Key = Key([0x87,0x65,0x09,0x06,0xef,0xda,0x47,0x65,0x7a,0x1f,0x95,0x36,0x8f,0x7a,0xf7,0x11,0xc0,0xd1,0x0e,0x51,0x47,0x35,0x44,0x3c,0x0b,0xdc,0xa4,0x6e,0x11,0x81,0xaa,0xc4]);


new_type!{
	secret MasterKey(32);
}

impl MasterKey {
	// Derive from username and password using a very aggressive KDF.
	// This function call will take a long time to finish (5 minutes or more).
	pub fn derive(username: &[u8], password: &[u8]) -> MasterKey {
		// Use the username as salt for the scrypt function, so attackers can't build a rainbow table to broadly attack users.
		// Hide username behind hmac so salt is unique to this application.
		let salt = hmac(&MASTER_KEY_USERNAME_SALT, username).code().to_vec();
		let mut master_key = [0u8; 32];
		let scrypt_params = scrypt::ScryptParams::new(MASTER_KEY_SCRYPT_LOG_N, MASTER_KEY_SCRYPT_R, MASTER_KEY_SCRYPT_P);
		scrypt::scrypt(password, &salt, &scrypt_params, &mut master_key);
		MasterKey(master_key)
	}
}


new_type!{
	secret LoginKey(32);
}

impl LoginKey {
	pub fn derive(master_key: &MasterKey) -> LoginKey {
		LoginKey(derive_key(&Key(master_key.0), DerivativeKeyId::LoginKey).0)
	}
}


pub fn hash_username_for_login(username: &[u8]) -> Vec<u8> {
	hmac(&LOGIN_USERNAME_SALT, username).code().to_vec()
}


#[derive(Eq, PartialEq, Debug, Clone)]
pub struct NetworkKeySuite {
	salt_key: Key,
	mac_key: Key,
	encryption_key: Key,
}

impl NetworkKeySuite {
	pub fn derive(master_key: &MasterKey) -> NetworkKeySuite {
		let master_key = Key(master_key.0);

		NetworkKeySuite {
			salt_key: derive_key(&master_key, DerivativeKeyId::NetworkSaltKey),
			mac_key: derive_key(&master_key, DerivativeKeyId::NetworkMacKey),
			encryption_key: derive_key(&master_key, DerivativeKeyId::NetworkEncryptionKey),
		}
	}

	// Deterministically encrypt data, returning (ciphertext, mac)
	// ID is included in the MAC calculation, but not in the ciphertext; useful for ensuring the server can't swap object data around.
	pub fn encrypt_object(&self, id: &[u8], data: &[u8]) -> (Vec<u8>, MacTag) {
		deterministic_encryption(id, data, &self.salt_key, &self.mac_key, &self.encryption_key)
	}

	// Deterministically decrypt payload, after validating mac.  Returns plaintext.
	pub fn decrypt_object(&self, id: &[u8], payload: &[u8]) -> io::Result<Vec<u8>> {
		deterministic_decryption(id, payload, &self.mac_key, &self.encryption_key)
	}
}


#[derive(Eq, PartialEq, Debug, Clone)]
pub struct FileKeySuite {
	salt_key: Key,
	mac_key: Key,
	encryption_key: Key,
}

impl FileKeySuite {
	pub fn derive(password: &[u8], params: &EncryptionParameters) -> FileKeySuite {
		let mut file_key = [0u8; 32];
		let scrypt_params = scrypt::ScryptParams::new(params.log_n, params.r, params.p);
		scrypt::scrypt(password, &params.salt, &scrypt_params, &mut file_key);
		let file_key = Key(file_key);

		FileKeySuite {
			salt_key: derive_key(&file_key, DerivativeKeyId::FileSaltKey),
			mac_key: derive_key(&file_key, DerivativeKeyId::FileMacKey),
			encryption_key: derive_key(&file_key, DerivativeKeyId::FileEncryptionKey),
		}
	}

	// Deterministically encrypt data, returning ciphertext+mac
	fn encrypt_object(&self, data: &[u8]) -> Vec<u8> {
		let (mut ciphertext, mac) = deterministic_encryption(&[], data, &self.salt_key, &self.mac_key, &self.encryption_key);
		ciphertext.extend_from_slice(&mac[..]);
		ciphertext
	}

	// Deterministically decrypt ciphertext, after validating mac.  Returns plaintext.
	fn decrypt_object(&self, data: &[u8]) -> io::Result<Vec<u8>> {
		deterministic_decryption(&[], data, &self.mac_key, &self.encryption_key)
	}
}


// Decrypts a database stored on disk.  Returns the plaintext, EncryptionParameters that were used, and the FileKeySuite that was used.
pub fn decrypt_from_file<R: Read>(reader: &mut R, password: &[u8]) -> io::Result<(Vec<u8>, EncryptionParameters, FileKeySuite)> {
	// Read file
	let mut filedata = Vec::new();
	reader.read_to_end(&mut filedata)?;

	// Parse header
	let (params, payload_and_checksum) = parse_header(&filedata)?;

	// Check checksum
	if payload_and_checksum.len() < 32 {
		return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "missing checksum"));
	}

	let payload = &payload_and_checksum[..payload_and_checksum.len()-32];
	let checksum = &payload_and_checksum[payload_and_checksum.len()-32..];

	let calculated_hash = sha256(&filedata[..filedata.len()-32]).to_vec();

	if calculated_hash != checksum {
		return Err(io::Error::new(io::ErrorKind::Other, "bad checksum"));
	}

	// Derive keys
	let file_key_suite = FileKeySuite::derive(password, &params);

	// Decrypt
	let plaintext = file_key_suite.decrypt_object(payload)?;

	Ok((plaintext, params, file_key_suite))
}


// Encrypts a database to disk.  Resulting file will contain a header, ciphertext, mac, and checksum.
pub fn encrypt_to_file<W: Write>(writer: &mut W, data: &[u8], params: &EncryptionParameters, key_suite: &FileKeySuite) -> io::Result<()> {
	let ciphertext = key_suite.encrypt_object(data);
	let header = build_header(params);
	let hash = {
		let mut hash = [0u8; 32];
		let mut hasher = Sha256::new();
		hasher.input(&header);
		hasher.input(&ciphertext);
		hasher.result(&mut hash);
		hash
	};

	writer.write_all(&header)?;
	writer.write_all(&ciphertext)?;
	writer.write_all(&hash)
}


fn build_header(params: &EncryptionParameters) -> Vec<u8> {
	let mut result = Vec::new();

	result.write_all(b"fortress2\0").unwrap();
	result.write_u8(params.log_n).unwrap();
	result.write_u32::<LittleEndian>(params.r).unwrap();
	result.write_u32::<LittleEndian>(params.p).unwrap();
	result.write_all(&params.salt).unwrap();
	result
}


fn parse_header(data: &[u8]) -> io::Result<(EncryptionParameters, &[u8])> {
	let mut reader = Cursor::new(data);

	let mut header_string = Vec::new();
	reader.read_until(0, &mut header_string)?;

	// Only v2 is supported
	if str::from_utf8(&header_string).unwrap() != "fortress2\0" {
		return Err(io::Error::new(io::ErrorKind::Other, "unsupported format"));
	}

	let log_n = reader.read_u8()?;
	let r = reader.read_u32::<LittleEndian>()?;
	let p = reader.read_u32::<LittleEndian>()?;
	let mut scrypt_salt = [0u8; 32];
	reader.read_exact(&mut scrypt_salt)?;

	let pos = reader.position() as usize;

	Ok((EncryptionParameters {
		log_n: log_n,
		r: r,
		p: p,
		salt: scrypt_salt,
	},
	&reader.into_inner()[pos..]))
}


enum DerivativeKeyId {
	LoginKey,

	NetworkSaltKey,
	NetworkMacKey,
	NetworkEncryptionKey,

	FileSaltKey,
	FileMacKey,
	FileEncryptionKey,
}

// We use an enum and the salt method to help prevent us from accidentally using the same
// id string for two different keys.
impl DerivativeKeyId {
	fn salt(&self) -> &[u8] {
		match *self {
			DerivativeKeyId::LoginKey => b"login-key",

			DerivativeKeyId::NetworkSaltKey => b"network-salt-key",
			DerivativeKeyId::NetworkMacKey => b"network-mac-key",
			DerivativeKeyId::NetworkEncryptionKey => b"network-encryption-key",

			DerivativeKeyId::FileSaltKey => b"file-salt-key",
			DerivativeKeyId::FileMacKey => b"file-mac-key",
			DerivativeKeyId::FileEncryptionKey => b"file-encryption-key",
		}
	}
}

fn derive_key(parent_key: &Key, child_id: DerivativeKeyId) -> Key {
	Key::from_slice(hmac(parent_key, child_id.salt()).code()).unwrap()
}


fn derive_deterministic_encryption_key(encryption_key: &Key, salt: &[u8]) -> Key {
	let salted_encryption_key = hmac(encryption_key, &salt[..32]).code().to_vec();
	Key::from_slice(&salted_encryption_key).unwrap()
}


// Perform deterministic encryption.
// This is done by deriving a salt from the plaintext using HMAC, and using the salt to derive a unique encryption key for the data.
// The use of a unique encryption key allows us to use stream ciphers like ChaCha20.
// Finally, we append a MAC.
// The result is salt+ciphertext+mac.
// During decryption we validate the MAC first, which prevents attackers from manipulating our algorithm.
// id is included in the MAC calculation, but it is not included as part of ciphertext.
fn deterministic_encryption(id: &[u8], plaintext: &[u8], salt_key: &Key, mac_key: &Key, encryption_key: &Key) -> (Vec<u8>, MacTag) {
	let salt = hmac(salt_key, plaintext).code().to_vec();
	let salted_encryption_key = derive_deterministic_encryption_key(encryption_key, &salt);
	let mut ciphertext = chacha20_process(&salted_encryption_key, plaintext);

	let mut result = Vec::new();
	result.extend_from_slice(&salt);
	result.append(&mut ciphertext);

	let mac = {
		let mut hmac = Hmac::new(Sha256::new(), &mac_key[..]);
		hmac.input(id);
		hmac.input(&result);
		MacTag::from_slice(hmac.result().code()).unwrap()
	};

	(result, mac)
}


// Refer to deterministic_encryption for the scheme used here.
// Returns the plaintext or an error (MAC failure).
fn deterministic_decryption(id: &[u8], payload: &[u8], mac_key: &Key, encryption_key: &Key) -> io::Result<Vec<u8>> {
	if payload.len() < 64 {
		return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "missing mac tag or salt"));
	}

	let salt_and_ciphertext = &payload[..payload.len()-32];
	let mac = MacResult::new(&payload[payload.len()-32..]);
	let calculated_mac = {
		let mut hmac = Hmac::new(Sha256::new(), &mac_key[..]);
		hmac.input(id);
		hmac.input(salt_and_ciphertext);
		hmac.result()
	};

	if calculated_mac != mac {
		return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "invalid mac tag"));
	}

	let salt = &salt_and_ciphertext[..32];
	let ciphertext = &salt_and_ciphertext[32..];

	let salted_encryption_key = derive_deterministic_encryption_key(encryption_key, salt);
	let plaintext = chacha20_process(&salted_encryption_key, ciphertext);

	Ok(plaintext)
}


fn sha256(input: &[u8]) -> [u8; 32] {
	let mut hash = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut hash);
	hash
}


fn hmac(key: &Key, data: &[u8]) -> MacResult {
	let mut hmac = Hmac::new(Sha256::new(), &key[..]);
	hmac.input(data);
	hmac.result()
}


fn chacha20_process(key: &Key, data: &[u8]) -> Vec<u8> {
	let mut result = vec![0u8; data.len()];
	let mut chacha = chacha20::ChaCha20::new(&key[..], &[0,0,0,0,0,0,0,0]);
	chacha.process(data, &mut result);
	result
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
	use super::{Key, MasterKey, LoginKey, FileKeySuite, NetworkKeySuite, deterministic_encryption, deterministic_decryption};
	use rand::{OsRng, Rng};

	// Test the deterministic encryption functions.
	#[test]
	fn test_deterministic_encryption() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let salt_key = Key::from_rng(&mut rng);
		let mac_key = Key::from_rng(&mut rng);
		let encryption_key = Key::from_rng(&mut rng);

		let (ciphertext, mac) = deterministic_encryption(&id, &data, &salt_key, &mac_key, &encryption_key);
		let (ciphertext2, mac2) = deterministic_encryption(&id, &data, &salt_key, &mac_key, &encryption_key);
		let mut ciphertext_and_mac = [&ciphertext[..], &mac[..]].concat();

		let plaintext = deterministic_decryption(&id, &ciphertext_and_mac, &mac_key, &encryption_key).unwrap();

		assert_eq!(plaintext, data);
		assert_eq!(ciphertext, ciphertext2);
		assert_eq!(mac, mac2);

		// Make sure it really is using a different key for different data
		let mut data2 = data.clone();
		data2[data.len()-1] ^= 1;
		let (ciphertext3, mac3) = deterministic_encryption(&id, &data2, &salt_key, &mac_key, &encryption_key);

		assert_ne!(ciphertext, ciphertext3);
		assert_ne!(mac, mac3);
		// If it were using the same key, these parts of the ciphertext would be the same (because a stream cipher is used)
		assert_ne!(&ciphertext[32..ciphertext.len()-1], &ciphertext3[32..ciphertext3.len()-1]);

		// Make sure it is verifying the mac
		ciphertext_and_mac[60] ^= 1;

		assert!(deterministic_decryption(&id, &ciphertext_and_mac, &mac_key, &encryption_key).is_err());
	}

	// Sanity check to make sure we didn't typo any of the key derivations.
	#[test]
	fn test_derived_keys_are_different() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let username = rng.gen_iter::<u8>().take(15).collect::<Vec<u8>>();
		let password = rng.gen_iter::<u8>().take(20).collect::<Vec<u8>>();
		let params = Default::default();

		let master_key = MasterKey::derive(&username, &password);
		let login_key = LoginKey::derive(&master_key);
		let file_key_suite = FileKeySuite::derive(&password, &params);
		let network_key_suite = NetworkKeySuite::derive(&master_key);

		let keys = [
			master_key.0,
			login_key.0,
			file_key_suite.salt_key.0,
			file_key_suite.mac_key.0,
			file_key_suite.encryption_key.0,
			network_key_suite.salt_key.0,
			network_key_suite.mac_key.0,
			network_key_suite.encryption_key.0,
		];

		for i in 0..keys.len() {
			for j in 0..keys.len() {
				if i == j {
					continue;
				}

				assert_ne!(keys[i], keys[j]);
			}
		}
	}

	// Test to make sure that ID is authenticated
	#[test]
	fn test_id_is_authenticated() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let id: [u8; 32] = rng.gen();
		let bad_id: [u8; 32] = rng.gen();
		let data = rng.gen_iter::<u8>().take(1034).collect::<Vec<u8>>();
		let salt_key: Key = rng.gen();
		let mac_key: Key = rng.gen();
		let encryption_key: Key = rng.gen();

		let (ciphertext, mac) = deterministic_encryption(&id, &data, &salt_key, &mac_key, &encryption_key);
		let mut ciphertext_and_mac = ciphertext.clone();
		ciphertext_and_mac.extend_from_slice(&mac[..]);

		let plaintext = deterministic_decryption(&id, &ciphertext_and_mac, &mac_key, &encryption_key).unwrap();

		assert_eq!(plaintext, data);

		assert!(deterministic_decryption(&bad_id, &ciphertext_and_mac, &mac_key, &encryption_key).is_err());
	}
}