use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use crypto::{scrypt, chacha20, pbkdf2};
use crypto::digest::Digest;
use crypto::hmac::Hmac;
use crypto::mac::{Mac, MacResult};
use crypto::sha2::Sha256;
use crypto::symmetriccipher::SynchronousStreamCipher;
use rand::{OsRng, Rng};
use std::io::{self, Cursor, Write, BufRead, Read};
use std::str;


#[derive(Eq, PartialEq, Debug)]
pub struct Encryptor {
	pub master_key: [u8; 32],
	pub params: EncryptionParameters,
}

impl Encryptor {
	pub fn new(password: &[u8], params: EncryptionParameters) -> Encryptor {
		Encryptor {
			master_key: params.derive_master_key(password),
			params: params,
		}
	}

	pub fn decrypt(password: &[u8], rawdata: &[u8]) -> io::Result<(u32, Encryptor, Vec<u8>)> {
		// Verify checksum
		let header_and_payload = verify_checksum(&rawdata)?;

		// Read header, which includes KDF and encryption parameters
		let (encryption_parameters, pbkdf2_salt, ciphertext_and_mactag) = read_header(header_and_payload)?;

		// Derive master key from password and database parameters
		let encryptor = Encryptor::new(password, encryption_parameters);

		// Derive encryption keys
		let encryption_keys = encryptor.derive_encryption_keys(pbkdf2_salt);

		// Verify mac tag
		let ciphertext = verify_mac(&encryption_keys, header_and_payload, ciphertext_and_mactag)?;

		// Decrypt
		let plaintext = encryption_keys.decrypt(ciphertext);

		Ok((1, encryptor, plaintext))
	}

	pub fn encrypt(&self, payload: &[u8]) -> io::Result<Vec<u8>> {
		let mut output: Vec<u8> = Vec::new();

		// Generate unique salt
		let pbkdf2_salt: [u8; 32] = {
			let mut rng = OsRng::new().expect("OsRng failed to initialize");
			rng.gen()
		};

		// Write header
		self.write_header(&mut output, &pbkdf2_salt)?;

		// Derive encryption keys
		let encryption_keys = self.derive_encryption_keys(pbkdf2_salt);

		// Write encrypted payload
		output.append(&mut encryption_keys.encrypt(&payload));

		// Write MAC tag
		{
			let mac_tag = encryption_keys.hmac(&output);
			output.write_all(mac_tag.code())?;
		}

		// Write checksum
		{
			let checksum = sha256(&output);
			output.write_all(&checksum)?;
		}

		Ok(output)
	}

	fn derive_encryption_keys(&self, salt: [u8; 32]) -> EncryptionKeys {
		let mut encryption_keys: EncryptionKeys = Default::default();
		let mut keying_material = [0u8; (32 + 8 + 32)];
		let mut mac = Hmac::new(Sha256::new(), &self.master_key);
		pbkdf2::pbkdf2(&mut mac, &salt, 1, &mut keying_material);

		encryption_keys.chacha_key.copy_from_slice(&keying_material[0..32]);
		encryption_keys.chacha_nonce.copy_from_slice(&keying_material[32..32 + 8]);
		encryption_keys.hmac_key.copy_from_slice(&keying_material[32 + 8..32 + 8 + 32]);

		encryption_keys
	}

	fn write_header<W: Write>(&self, writer: &mut W, pbkdf2_salt: &[u8; 32]) -> io::Result<()> {
		writer.write_all(b"fortress1-scrypt-chacha20\0")?;
		writer.write_u8(self.params.log_n)?;
		writer.write_u32::<LittleEndian>(self.params.r)?;
		writer.write_u32::<LittleEndian>(self.params.p)?;
		writer.write_all(&self.params.salt)?;
		writer.write_all(pbkdf2_salt)?;
		Ok(())
	}
}


// Given rawdata, which should be data+sha256checksum, this function
// checks the checksum and then returns a reference to just data.
fn verify_checksum(rawdata: &[u8]) -> io::Result<&[u8]> {
	if rawdata.len() < 32 {
		return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, missing checksum"));
	}

	let data = &rawdata[..rawdata.len() - 32];
	let checksum = &rawdata[rawdata.len() - 32..];
	let calculated_checksum = sha256(data);

	if checksum != calculated_checksum {
		return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, failed checksum"));
	}

	Ok(data)
}


// Given header+ciphertext+mactag and ciphertext+mactag, verify mactag and return ciphertext
fn verify_mac<'a>(encryption_keys: &EncryptionKeys, data: &[u8], ciphertext_and_mactag: &'a [u8]) -> io::Result<&'a [u8]> {
	if data.len() < 32 {
		return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, missing mac tag"));
	}

	let mac_tag = MacResult::new(&data[data.len() - 32..]);
	let calculated_mac = encryption_keys.hmac(&data[..data.len() - 32]);

	if mac_tag != calculated_mac {
		return Err(io::Error::new(io::ErrorKind::Other, "incorrect password or corrupt database"));
	}

	Ok(&ciphertext_and_mactag[..ciphertext_and_mactag.len() - 32])
}

// Read an encrypted database's header.
// Returns the encryption parameters, pbkdf2_salt, and a reference to the ciphertext+mactag.
fn read_header(data: &[u8]) -> io::Result<(EncryptionParameters, [u8; 32], &[u8])> {
	let mut cursor = Cursor::new(data);
	let mut header_string = Vec::new();

	cursor.read_until(0, &mut header_string)?;

	// Only v1, scrypt-chacha20 is supported
	if str::from_utf8(&header_string).unwrap() != "fortress1-scrypt-chacha20\0" {
		return Err(io::Error::new(io::ErrorKind::Other, "unsupported encryption"));
	}

	let log_n = cursor.read_u8()?;
	let r = cursor.read_u32::<LittleEndian>()?;
	let p = cursor.read_u32::<LittleEndian>()?;
	let mut scrypt_salt = [0u8; 32];
	cursor.read_exact(&mut scrypt_salt)?;
	let mut pbkdf2_salt = [0u8; 32];
	cursor.read_exact(&mut pbkdf2_salt)?;

	let data_begin = cursor.position() as usize;

	Ok(
		(EncryptionParameters {
			log_n: log_n,
			r: r,
			p: p,
			salt: scrypt_salt
		},
		pbkdf2_salt,
		&cursor.into_inner()[data_begin..])
	)
}


#[derive(Eq, PartialEq, Debug)]
pub struct EncryptionParameters {
	// Parameters for deriving master_key using scrypt
	pub log_n: u8,
	pub r: u32,
	pub p: u32,
	pub salt: [u8; 32],
}

impl EncryptionParameters {
	// Derive master key from user password
	fn derive_master_key(&self, password: &[u8]) -> [u8; 32] {
		let mut master_key = [0u8; 32];
		let scrypt_params = scrypt::ScryptParams::new(self.log_n, self.r, self.p);
		scrypt::scrypt(password, &self.salt, &scrypt_params, &mut master_key);
		master_key
	}
}

// Default is N=18, r=8, p=1 (less N when in debug mode)
// Some sites suggested 16 for modern systems, but I didn't see measurable benefit on my development machine.
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

#[derive(Default)]
struct EncryptionKeys {
	pub chacha_key: [u8; 32],
	pub chacha_nonce: [u8; 8],
	pub hmac_key: [u8; 32],
}

impl EncryptionKeys {
	fn hmac(&self, data: &[u8]) -> MacResult {
		let mut hmac = Hmac::new(Sha256::new(), &self.hmac_key);
		hmac.input(data);
		hmac.result()
	}

	fn decrypt(&self, ciphertext: &[u8]) -> Vec<u8> {
		let mut plaintext = vec![0u8; ciphertext.len()];
		let mut chacha = chacha20::ChaCha20::new(&self.chacha_key, &self.chacha_nonce);
		chacha.process(ciphertext, &mut plaintext);
		plaintext
	}

	fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
		let mut ciphertext = vec![0u8; plaintext.len()];
		let mut chacha = chacha20::ChaCha20::new(&self.chacha_key, &self.chacha_nonce);
		chacha.process(plaintext, &mut ciphertext);
		ciphertext
	}
}


fn sha256(input: &[u8]) -> [u8; 32] {
	let mut hash = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut hash);
	hash
}