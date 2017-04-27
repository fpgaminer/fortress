extern crate rand;
extern crate time;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate rustc_serialize;
extern crate flate2;
extern crate crypto;
extern crate byteorder;
extern crate tempdir;

use rand::{OsRng, Rng};
use flate2::Compression;
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use crypto::{scrypt, chacha20, pbkdf2};
use crypto::symmetriccipher::SynchronousStreamCipher;
use crypto::hmac::Hmac;
use crypto::sha2::Sha256;
use crypto::mac::{Mac, MacResult};
use crypto::digest::Digest;
use std::path::Path;
use std::fs::File;
use std::str;
use std::io::{BufRead, Read, Write, Cursor, self};
use serde::Serialize;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Entry {
	#[serde(with = "id_format")]
	pub id: [u8; 32],
	pub history: Vec<EntryData>,
}

impl Entry {
	pub fn new() -> Entry {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		Entry {
			id: rng.gen(),
			history: Vec::new(),
		}
	}

	pub fn edit(&mut self, new_data: &EntryData) {
		self.history.push(new_data.clone());
	}

	pub fn read_latest(&self) -> Option<&EntryData> {
		self.history.last()
	}
}

impl Default for Entry {
	fn default() -> Self {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		Entry {
			id: rng.gen(),
			history: Vec::new(),
		}
	}
}

mod id_format {
	use serde::{self, Deserialize, Serializer, Deserializer};
	use rustc_serialize::hex::{ToHex, FromHex};

	pub fn serialize<S>(id: &[u8], serializer: S) -> Result<S::Ok, S::Error>
		where S: Serializer
	{
		serializer.serialize_str(&id.to_hex())
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
		where D: Deserializer<'de>
	{
		let mut bytes = [0u8; 32];
		let s = String::deserialize(deserializer)?;
		let x = s.from_hex().map_err(serde::de::Error::custom)?;
		for i in 0..32 {
			bytes[i] = x[i];
		}
		Ok(bytes)
	}
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct EntryData {
	pub title: String,
	pub username: String,
	pub password: String,
	pub url: String,
	pub notes: String,
	pub time_created: i64,
}

impl EntryData {
	pub fn new(title: &str, username: &str, password: &str, url: &str, notes: &str) -> EntryData {
		EntryData {
			title: title.to_string(),
			username: username.to_string(),
			password: password.to_string(),
			url: url.to_string(),
			notes: notes.to_string(),
			time_created: time::now_utc().to_timespec().sec,
		}
	}
}

#[derive(Serialize, Deserialize, Default, Eq, PartialEq, Debug)]
pub struct Database {
	pub entries: Vec<Entry>,
	#[serde(skip_serializing, skip_deserializing)]
	master_key: Option<[u8; 32]>,
	#[serde(skip_serializing, skip_deserializing)]
	encryption_parameters: EncryptionParameters,
}

impl Database {
	pub fn new_with_password(password: &[u8]) -> Database {
		let mut db: Database = Default::default();
		db.master_key = Some(Database::derive_master_key(password, &db.encryption_parameters));
		db
	}

	pub fn change_password(&mut self, password: &[u8]) {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Refresh salt
		self.encryption_parameters.salt = rng.gen();

		// Derive the new master key
		self.master_key = Some(Database::derive_master_key(password, &self.encryption_parameters));
	}

	pub fn new_entry(&mut self) {
		let entry = Entry::new();
		self.entries.push(entry);
	}

	pub fn add_entry(&mut self, entry: Entry) {
		self.entries.push(entry);
	}

	pub fn get_entry_by_id(&mut self, id: &[u8]) -> Option<&mut Entry> {
		for entry in &mut self.entries {
			if entry.id == id {
				return Some(entry);
			}
		}

		None
	}

	pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
		let mut output: Vec<u8> = Vec::new();

		// Generate unique salt
		let pbkdf2_salt: [u8; 32] = {
			let mut rng = OsRng::new().expect("OsRng failed to initialize");
			rng.gen()
		};

		// Derive encryption keys
		let encryption_keys = Database::derive_encryption_keys(&self.master_key.unwrap(), &pbkdf2_salt);

		// Write header
		Database::write_header(&mut output, &self.encryption_parameters, &pbkdf2_salt)?;

		// Write serialized, compressed, and encrypted database
		{
			let encrypted_writer = ChaCha20Writer::new(&mut output, &encryption_keys.chacha_key, &encryption_keys.chacha_nonce);
			let compressed_writer = GzEncoder::new(encrypted_writer, Compression::Default);
			let mut json_writer = serde_json::ser::Serializer::new(compressed_writer);

			self.serialize(&mut json_writer)?;
			json_writer.into_inner().finish()?;  // TODO: Do we need to do this?  Can we just call flush?  Will the writer leaving scope force a flush?  Muh dunno...
		}

		// Write MAC tag
		{
			let mac_tag = hmac(&encryption_keys.hmac_key, &output);
			output.write_all(mac_tag.code())?;
		}

		// Write checksum
		{
			let checksum = sha256(&output);
			output.write_all(&checksum)?;
		}

		// Write to file
		let mut file = File::create(path)?;
		file.write_all(&output)
	}

	pub fn load_from_path<P: AsRef<Path>>(path: P, password: &[u8]) -> io::Result<Database> {
		// This block ensures that we deallocate everything we don't need after getting plaintext
		let (master_key, encryption_parameters, plaintext) = {
			// Read file
			let rawdata = read_file(path)?;

			// Verify checksum
			let header_and_payload = Database::verify_checksum(&rawdata)?;

			// Read header, which includes KDF and encryption parameters
			let (encryption_parameters, pbkdf2_salt, ciphertext_and_mactag) = Database::read_header(header_and_payload)?;

			// Derive master key from password and database parameters
			let master_key = Database::derive_master_key(password, &encryption_parameters);

			// Derive encryption keys
			let encryption_keys = Database::derive_encryption_keys(&master_key, &pbkdf2_salt);

			// Verify mac tag
			let ciphertext = Database::verify_mac(header_and_payload, ciphertext_and_mactag, &encryption_keys)?;

			// Decrypt
			let plaintext = Database::decrypt(&encryption_keys, ciphertext);

			(master_key, encryption_parameters, plaintext)
		};

		// Decompress and deserialize
		let mut db: Database = {
			let d = GzDecoder::new(io::Cursor::new(plaintext)).unwrap();
			serde_json::from_reader(d).unwrap()
		};

		// Save master key and encryption parameters for quicker saving
		db.master_key = Some(master_key);
		db.encryption_parameters = encryption_parameters;
		Ok(db)
	}

	// Given rawdata, which should be data+sha256checksum, this function
	// checks the checksum and then returns a reference to just data.
	fn verify_checksum(rawdata: &[u8]) -> io::Result<&[u8]> {
		if rawdata.len() < 32 {
			return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, missing checksum"));
		}

		let data = &rawdata[..rawdata.len()-32];
		let checksum = &rawdata[rawdata.len()-32..];
		let calculated_checksum = sha256(data);
			
		if checksum != calculated_checksum {
			return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, failed checksum"));
		}

		Ok(data)
	}

	// Read an encrypted database's header.
	// Returns the encryption parameters, pbkdf2_salt, and a reference to the ciphertext+mactag.
	fn read_header(data: &[u8]) -> io::Result<(EncryptionParameters,[u8;32],&[u8])> {
		let mut cursor = Cursor::new(data);
		let mut header_string = Vec::new();

		cursor.read_until(0, &mut header_string)?;

		// Only scrypt-chacha20 is supported
		if str::from_utf8(&header_string).unwrap() != "fortress-scrypt-chacha20\0" {
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

		Ok((EncryptionParameters {
			log_n: log_n,
			r: r,
			p: p,
			salt: scrypt_salt
		}, pbkdf2_salt, &cursor.into_inner()[data_begin..]))
	}

	// Derive master key from user password
	fn derive_master_key(password: &[u8], parameters: &EncryptionParameters) -> [u8; 32] {
		let mut master_key = [0u8; 32];
		let scrypt_params = scrypt::ScryptParams::new(parameters.log_n, parameters.r, parameters.p);
		scrypt::scrypt(password, &parameters.salt, &scrypt_params, &mut master_key);
		master_key
	}

	fn derive_encryption_keys(master_key: &[u8;32], salt: &[u8;32]) -> EncryptionKeys {
		let mut encryption_keys: EncryptionKeys = Default::default();
		let mut keying_material = [0u8; (32+8+32)];
		let mut mac = Hmac::new(Sha256::new(), master_key);
		pbkdf2::pbkdf2(&mut mac, salt, 1, &mut keying_material);

		encryption_keys.chacha_key.copy_from_slice(&keying_material[0..32]);
		encryption_keys.chacha_nonce.copy_from_slice(&keying_material[32..32+8]);
		encryption_keys.hmac_key.copy_from_slice(&keying_material[32+8..32+8+32]);

		encryption_keys
	}

	// Given header+ciphertext+mactag and ciphertext+mactag, verify mactag and return ciphertext
	fn verify_mac<'a>(data: &[u8], ciphertext_and_mactag: &'a [u8], encryption_keys: &EncryptionKeys) -> io::Result<&'a [u8]> {
		if data.len() < 32 {
			return Err(io::Error::new(io::ErrorKind::Other, "corrupt database, missing mac tag"));
		}

		let mac_tag = MacResult::new(&data[data.len()-32..]);
		let calculated_mac = hmac(&encryption_keys.hmac_key, &data[..data.len()-32]);

		if mac_tag != calculated_mac {
			return Err(io::Error::new(io::ErrorKind::Other, "incorrect password or corrupt database"));
		}

		Ok(&ciphertext_and_mactag[..ciphertext_and_mactag.len()-32])
	}

	fn decrypt(encryption_keys: &EncryptionKeys, ciphertext: &[u8]) -> Vec<u8> {
		let mut plaintext = vec![0u8; ciphertext.len()];
		let mut chacha = chacha20::ChaCha20::new(&encryption_keys.chacha_key, &encryption_keys.chacha_nonce);
		chacha.process(ciphertext, &mut plaintext);
		plaintext
	}

	fn write_header<W: Write>(writer: &mut W, encryption_parameters: &EncryptionParameters, pbkdf2_salt: &[u8; 32]) -> io::Result<()> {
		writer.write_all(b"fortress-scrypt-chacha20\0")?;
		writer.write_u8(encryption_parameters.log_n)?;
		writer.write_u32::<LittleEndian>(encryption_parameters.r)?;
		writer.write_u32::<LittleEndian>(encryption_parameters.p)?;
		writer.write_all(&encryption_parameters.salt)?;
		writer.write_all(pbkdf2_salt)?;
		Ok(())
	}
}

#[derive(Eq, PartialEq, Debug)]
struct EncryptionParameters {
	// Parameters for deriving master_key using scrypt
	pub log_n: u8,
	pub r: u32,
	pub p: u32,
	pub salt: [u8; 32],
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

struct ChaCha20Writer<W> {
	chacha: chacha20::ChaCha20,
	writer: W,
	buffer: Vec<u8>,
}

impl<W> ChaCha20Writer<W>
where
	W: io::Write,
{
	pub fn new(writer: W, key: &[u8], nonce: &[u8]) -> Self {
		ChaCha20Writer {
			chacha: chacha20::ChaCha20::new(key, nonce),
			writer: writer,
			buffer: Vec::new(),
		}
	}
}

impl<W> Write for ChaCha20Writer<W>
where
	W: io::Write,
{
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		// TODO: The way this is implemented is ... garbage.
		// We should just re-implement most of the pieces we used from rust-crypto and add nicer
		// interfaces.
		self.buffer.resize(buf.len(), 0);
		self.chacha.process(buf, &mut self.buffer);
		self.writer.write(&self.buffer)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.writer.flush()
	}
}

fn sha256(input: &[u8]) -> [u8; 32] {
	let mut hash = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut hash);
	hash
}


fn hmac(key: &[u8], input: &[u8]) -> MacResult {
	let mut hmac = Hmac::new(Sha256::new(), key);
	hmac.input(input);
	hmac.result()
}


fn read_file<P: AsRef<Path>>(path: P) -> io::Result<Vec<u8>> {
	let mut data = Vec::new();
	File::open(path)?.read_to_end(&mut data)?;
	Ok(data)
}


#[cfg(test)]
mod tests {
	use rand::{OsRng, Rng};
	use tempdir::TempDir;
	use super::{Database, read_file};

	#[test]
	fn encrypt_then_decrypt() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let password_len = rng.gen_range(0, 64);
		let password = rng.gen_iter::<u8>().take(password_len).collect::<Vec<u8>>();
		let tmp_dir = TempDir::new("test").unwrap();

		let mut db = Database::new_with_password(&password);
		db.new_entry();
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), &password).unwrap();

		assert_eq!(db, db2);
		db.new_entry();
		assert!(db != db2);
	}

	#[test]
	fn password_change() {
		let mut db = Database::new_with_password("password".as_bytes());
		let old_salt = db.encryption_parameters.salt;
		let old_master_key = db.master_key.unwrap();
		db.change_password("password".as_bytes());
		assert!(db.encryption_parameters.salt != old_salt);
		assert!(db.master_key.unwrap() != old_master_key);
	}

	// Make sure every encryption uses a different encryption key
	#[test]
	fn encryption_is_salted() {
		let tmp_dir = TempDir::new("test").unwrap();

		let db = Database::new_with_password("password".as_bytes());
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();
		db.save_to_path(tmp_dir.path().join("test2.fortressdb")).unwrap();

		let encrypted1 = read_file(tmp_dir.path().join("test.fortressdb")).unwrap();
		let encrypted2 = read_file(tmp_dir.path().join("test2.fortressdb")).unwrap();

		assert!(encrypted1 != encrypted2);
	}

	// Just some sanity checks on our keys
	#[test]
	fn key_sanity_checks() {
		let db = Database::new_with_password("password".as_bytes());
		let db2 = Database::new_with_password("password".as_bytes());
		let zeros = [0u8; 32];

		assert!(db != db2);
		assert!(db.master_key.unwrap() != db2.master_key.unwrap());
		assert!(db.master_key.unwrap() != zeros);
		assert!(db.encryption_parameters.salt != zeros);
	}

	// TODO: Test all the failure modes of opening a database
	// TODO: e.g. make sure corrupting the database file results in a checksum failure, make sure a bad mac results in a MAC failure, etc.
}
