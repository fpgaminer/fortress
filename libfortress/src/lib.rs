// Methodology Note:
// This library enforces invariants by using Rust's visibility rules.
// For example one major feature of most types in this library is
// non-destructive editing.  Editing an Entry, for example, simply appends
// to its history, rather than destructively modifying it.  This invariant is
// enforced by making all properties of the struct private and forcing access
// through getters/setters and other public methods.
// Even within this library itself these rules are enforced to prevent mistakes
// by placing Entry in a sub-module so that other modules cannot mistakenly
// access private properties of the struct.
//
// The goal of this library is for users to be confident that, no matter what, data
// stored in the Database is never unintentionally lost.  By using this methodology
// of enforcing non-destructive and other invariants we can drastically reduce
// the probability of bugs violating this intention.
//
//
// NOTE: Changing any of these structs which derive Serialize/Deserialize requires
// bumping the database format version.
//
// NOTE: No versioning is currently included for cloud objects.  The plan is to
// add versioning the next time the format changes, and to change the way the
// network and login keys are calculated to prevent old versions from syncing.
// We can then have a plan for more graceful versioning going forward.
#[macro_use]
mod newtype_macros;
mod database_object;
mod database_object_map;
mod errors;
pub mod sync_parameters;

pub use crate::database_object::{Directory, Entry, EntryHistory};

use crate::{database_object::DatabaseObject, database_object_map::DatabaseObjectMap, sync_parameters::SyncParameters};
pub use errors::FortressError;
pub use fortresscrypto;
use fortresscrypto::{EncryptedObject, FileKeySuite, LoginId, LoginKey, SIV};
use rand::{rngs::OsRng, seq::SliceRandom, Rng};
use reqwest::{IntoUrl, Method};
use serde::{Deserialize, Serialize};
use std::{
	collections::{HashMap, HashSet},
	fs::File,
	io::{self, BufReader, BufWriter},
	path::Path,
	str,
};
use tempfile::NamedTempFile;
use url::Url;


new_type! {
	public ID(32);
}


const ROOT_DIRECTORY_ID: ID = ID([0; 32]);


// TODO: Not sure if we want this to be cloneable?
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Database {
	objects: DatabaseObjectMap,

	sync_parameters: SyncParameters,

	#[serde(skip_serializing, skip_deserializing)]
	file_key_suite: FileKeySuite,

	sync_url: Option<Url>,
}

impl Database {
	pub fn new_with_password<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> Database {
		let username = username.as_ref();
		let password = password.as_ref();

		let encryption_parameters = Default::default();
		let file_key_suite = FileKeySuite::derive(password.as_bytes(), &encryption_parameters).expect("Internal error: Scrypt parameters were invalid.");

		// TODO: Derive in a background thread
		let sync_parameters = SyncParameters::new(username, password);

		let root = Directory::new_root();
		let mut objects = DatabaseObjectMap::new();
		objects.update(DatabaseObject::Directory(root));

		Database {
			objects,
			sync_parameters,
			file_key_suite,
			sync_url: None,
		}
	}

	pub fn change_password<A: AsRef<str>, B: AsRef<str>>(&mut self, username: A, password: B) {
		let username = username.as_ref();
		let password = password.as_ref();

		let encryption_parameters = Default::default();
		self.file_key_suite = FileKeySuite::derive(password.as_bytes(), &encryption_parameters).expect("Internal error: Scrypt parameters were invalid.");

		// TODO: Derive in a background thread
		self.sync_parameters = SyncParameters::new(username, password);

		// TODO: Need to tell server about our new login-key using our old login-key
		// TODO: We should re-sync after this
	}

	pub fn get_username(&self) -> &str {
		self.sync_parameters.get_username()
	}

	pub fn get_login_id(&self) -> &LoginId {
		self.sync_parameters.get_login_id()
	}

	pub fn get_login_key(&self) -> &LoginKey {
		self.sync_parameters.get_login_key()
	}

	pub fn get_sync_url(&self) -> Option<&Url> {
		self.sync_url.as_ref()
	}

	pub fn set_sync_url(&mut self, url: Option<Url>) {
		self.sync_url = url;
	}

	pub fn get_root(&self) -> &Directory {
		self.get_directory_by_id(&ROOT_DIRECTORY_ID).expect("Internal error")
	}

	pub fn get_root_mut(&mut self) -> &mut Directory {
		self.get_directory_by_id_mut(&ROOT_DIRECTORY_ID).expect("Internal error")
	}

	pub fn new_entry(&mut self) {
		let entry = Entry::new();
		self.add_entry(entry);
	}

	pub fn add_entry(&mut self, entry: Entry) {
		self.get_root_mut().add(*entry.get_id());
		self.objects.update(DatabaseObject::Entry(entry));
	}

	pub fn add_directory(&mut self, directory: Directory) {
		self.get_root_mut().add(*directory.get_id());
		self.objects.update(DatabaseObject::Directory(directory));
	}

	pub fn get_entry_by_id(&self, id: &ID) -> Option<&Entry> {
		match self.objects.get(id)? {
			&DatabaseObject::Entry(ref entry) => Some(entry),
			_ => None,
		}
	}

	pub fn get_entry_by_id_mut(&mut self, id: &ID) -> Option<&mut Entry> {
		match self.objects.get_mut(id)? {
			&mut DatabaseObject::Entry(ref mut entry) => Some(entry),
			_ => None,
		}
	}

	pub fn get_directory_by_id(&self, id: &ID) -> Option<&Directory> {
		match self.objects.get(id)? {
			&DatabaseObject::Directory(ref dir) => Some(dir),
			_ => None,
		}
	}

	pub fn get_directory_by_id_mut(&mut self, id: &ID) -> Option<&mut Directory> {
		match self.objects.get_mut(id)? {
			&mut DatabaseObject::Directory(ref mut dir) => Some(dir),
			_ => None,
		}
	}

	pub fn list_directories(&self) -> impl Iterator<Item = &Directory> {
		self.objects.values().filter_map(|obj| obj.as_directory())
	}

	pub fn list_directories_mut(&mut self) -> impl Iterator<Item = &mut Directory> {
		self.objects.values_mut().filter_map(|obj| obj.as_directory_mut())
	}

	pub fn list_entries(&self) -> impl Iterator<Item = &Entry> {
		self.objects.values().filter_map(|obj| obj.as_entry())
	}

	pub fn list_entries_mut(&mut self) -> impl Iterator<Item = &mut Entry> {
		self.objects.values_mut().filter_map(|obj| obj.as_entry_mut())
	}

	pub fn get_parent_directory(&self, id: &ID) -> Option<&Directory> {
		self.list_directories().find(move |dir| dir.contains(id))
	}

	pub fn get_parent_directory_mut(&mut self, id: &ID) -> Option<&mut Directory> {
		self.list_directories_mut().find(move |dir| dir.contains(id))
	}

	pub fn move_object(&mut self, id: &ID, new_parent: &ID) {
		let old_parent = self.get_parent_directory_mut(id).map(|d| *d.get_id());

		// Add to new parent first (so the entry isn't dangling during the operation)
		if let Some(parent) = self.get_directory_by_id_mut(new_parent) {
			parent.add(*id);
		}

		// Remove from old parent
		if let Some(parent) = old_parent.and_then(|id| self.get_directory_by_id_mut(&id)) {
			parent.remove(*id);
		}
	}

	pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), FortressError> {
		// Create a temporary file to write to
		let mut temp_file = {
			let parent_directory = path.as_ref().parent().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Bad path"))?;
			NamedTempFile::new_in(parent_directory)?
		};

		// Serialized payload
		let payload = serde_json::to_vec(&self)?;

		// Encrypt and write to the temporary file
		fortresscrypto::encrypt_to_file(&mut BufWriter::new(&mut temp_file), &payload, &self.file_key_suite)?;

		// Now close the temp file and move it to the destination.
		// Moving a temporary file is atomic (at least on *nix), so doing it this way
		// instead of writing directly to the destination file helps prevent data loss.
		let temp_path = temp_file.into_temp_path();
		temp_path.persist(path).map_err(|e| e.error).map_err(FortressError::from)
	}

	pub fn load_from_path<P: AsRef<Path>, A: AsRef<str>>(path: P, password: A) -> Result<Database, FortressError> {
		let password = password.as_ref();

		// This struct is needed because Database has fields that aren't part of
		// serialization, but can't implement Default.
		#[derive(Deserialize)]
		struct SerializableDatabase {
			objects: DatabaseObjectMap,
			sync_parameters: SyncParameters,
			sync_url: Option<Url>,
		}

		// Read file and decrypt
		let (plaintext, file_key_suite) = {
			let file = File::open(path)?;
			let mut reader = BufReader::new(file);

			fortresscrypto::decrypt_from_file(&mut reader, password.as_bytes())?
		};

		// Deserialize
		let db: SerializableDatabase = serde_json::from_slice(&plaintext)?;

		// Keep encryption keys for quicker saving later
		Ok(Database {
			objects: db.objects,
			sync_parameters: db.sync_parameters,

			file_key_suite,
			sync_url: db.sync_url,
		})
	}

	// TODO: Sync should be performed in a separate background thread
	// TODO: Instead of having library users call sync themselves, we should just have an init method which sets up a continuous automatic
	// background sync.
	pub fn sync(&mut self) -> Result<(), FortressError> {
		let url = self.sync_url.as_ref().ok_or(FortressError::SyncBadUrl)?;

		// Force SSL on release builds
		let client = if cfg!(debug_assertions) {
			reqwest::blocking::Client::new()
		} else {
			reqwest::blocking::Client::builder()
				.https_only(true)
				.build()
				.expect("Failed to build HTTPS-only client")
		};

		loop {
			// Get list of objects from server
			let server_objects = self.sync_api_list_objects(&client, url)?.into_iter().collect::<HashMap<_, _>>();
			let mut loop_again = false;

			// Download any objects that we're missing or that differ
			for (server_id, server_siv) in &server_objects {
				if let Some(local_object) = self.objects.get(server_id) {
					let encrypted_object = self.encrypt_object(local_object);

					if encrypted_object.siv != *server_siv {
						// Object is different, download it and merge
						let server_object = match self.sync_api_get_object(&client, url, server_id)? {
							Some(object) => object,
							None => {
								// We couldn't get the object from the server (could be a changed password).  Ignore.
								println!("WARNING: Couldn't get object {} from server, ignoring", server_id.to_hex());
								continue;
							},
						};

						let new_object = match (local_object, server_object) {
							(DatabaseObject::Directory(local_directory), DatabaseObject::Directory(server_directory)) => {
								let new_directory = local_directory.merge(&server_directory).ok_or(FortressError::SyncConflict)?;
								DatabaseObject::Directory(new_directory)
							},
							(DatabaseObject::Entry(local_entry), DatabaseObject::Entry(server_entry)) => {
								let new_entry = local_entry.merge(&server_entry).ok_or(FortressError::SyncConflict)?;
								DatabaseObject::Entry(new_entry)
							},
							_ => panic!("Object type mismatch, this should never happen"),
						};

						self.objects.update(new_object);
					}
				} else {
					let object = self
						.sync_api_get_object(&client, url, server_id)?
						.ok_or(FortressError::SyncInconsistentServer)?;
					self.objects.update(object);
				}
			}

			// Upload any objects the server doesn't know about or that differ
			// Objects will differ here if the server had an older version or the merge above resulted in a change
			for (local_id, local_object) in &self.objects {
				let encrypted_object = self.encrypt_object(local_object);

				if let Some(server_siv) = server_objects.get(local_id) {
					if encrypted_object.siv != *server_siv {
						// Object is different, upload it
						self.sync_api_update_object(&client, url, local_object, server_siv)?;
						loop_again = true;
					}
				} else {
					// Object is missing from server, upload it
					self.sync_api_update_object(&client, url, local_object, &SIV([0; 32]))?;
				}
			}

			if !loop_again {
				break;
			}
		}

		Ok(())
	}

	/// List all objects on the server
	fn sync_api_list_objects(&self, client: &reqwest::blocking::Client, url: &Url) -> Result<Vec<(ID, SIV)>, ApiError> {
		api_request(
			client,
			self.sync_parameters.get_login_id(),
			self.sync_parameters.get_login_key(),
			Method::GET,
			url.join("/objects").expect("internal error"),
			"",
		)?
		.json()
		.map_err(ApiError::from)
	}

	/// Upload object to fortress server
	fn sync_api_update_object(&self, client: &reqwest::blocking::Client, url: &Url, object: &DatabaseObject, old_mac: &SIV) -> Result<(), ApiError> {
		// Encrypt
		let encrypted_object = self.encrypt_object(object);

		let body = [&encrypted_object.ciphertext, encrypted_object.siv.as_ref()].concat();
		let url = url
			.join(&format!("/object/{}/{}", object.get_id().to_hex(), old_mac.to_hex()))
			.expect("internal error");

		api_request(
			client,
			self.sync_parameters.get_login_id(),
			self.sync_parameters.get_login_key(),
			Method::POST,
			url,
			body,
		)
		.map(|_| ())
	}

	/// Fetch an object from the server.
	/// If the object doesn't exist on the server or could not be decrypted then None is returned.
	fn sync_api_get_object(&self, client: &reqwest::blocking::Client, url: &Url, id: &ID) -> Result<Option<DatabaseObject>, FortressError> {
		let url = url.join(&format!("/object/{}", id.to_hex())).expect("internal error");

		let response = api_request(
			client,
			self.sync_parameters.get_login_id(),
			self.sync_parameters.get_login_key(),
			Method::GET,
			url,
			"",
		)?
		.bytes()
		.map_err(ApiError::from)?;

		if response.len() < 32 {
			println!("WARNING: Server returned invalid response for object");
			return Ok(None);
		}

		let (ciphertext, siv) = response.split_at(response.len() - 32);
		let siv = SIV::from_slice(siv).expect("internal error");
		let encrypted_object = EncryptedObject {
			ciphertext: ciphertext.to_vec(),
			siv,
		};

		let plaintext = match self.sync_parameters.get_network_key_suite().decrypt_object(&id[..], &encrypted_object) {
			Ok(plaintext) => plaintext,
			Err(err) => {
				println!("WARNING: Error while decrypting server object(ID: {}): {}", id.to_hex(), err);
				return Ok(None);
			},
		};

		match serde_json::from_slice(&plaintext) {
			Ok(object) => Ok(Some(object)),
			Err(err) => {
				println!("WARNING: Error while deserializing server object(ID: {}): {}", id.to_hex(), err);
				Ok(None)
			},
		}
	}

	fn encrypt_object(&self, object: &DatabaseObject) -> EncryptedObject {
		let payload = serde_json::to_vec(&object).expect("internal error");
		self.sync_parameters.get_network_key_suite().encrypt_object(&object.get_id()[..], &payload)
	}
}


#[derive(Debug)]
pub enum ApiError {
	ReqwestError(reqwest::Error),
	ApiError(String),
}

impl From<reqwest::Error> for ApiError {
	fn from(err: reqwest::Error) -> ApiError {
		ApiError::ReqwestError(err)
	}
}

impl std::fmt::Display for ApiError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			ApiError::ReqwestError(err) => write!(f, "Reqwest error: {}", err),
			ApiError::ApiError(err) => write!(f, "API error: {}", err),
		}
	}
}


fn api_request<U, B>(
	client: &reqwest::blocking::Client,
	login_id: &LoginId,
	login_key: &LoginKey,
	method: Method,
	url: U,
	body: B,
) -> Result<reqwest::blocking::Response, ApiError>
where
	U: IntoUrl,
	B: Into<reqwest::blocking::Body>,
{
	let auth_token = login_id.to_hex() + login_key.to_hex().as_str();
	let response = client.request(method, url).bearer_auth(auth_token).body(body).send()?;

	if response.status().is_success() {
		Ok(response)
	} else {
		let error = response.text()?;
		Err(ApiError::ApiError(error))
	}
}


pub fn random_string(length: usize, uppercase: bool, lowercase: bool, numbers: bool, others: &str) -> String {
	let alphabet_uppercase = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
	let alphabet_lowercase = "abcdefghijklmnopqrstuvwxyz";
	let alphabet_numbers = "0123456789";

	// Use a hashset to avoid duplicates caused by "others"
	let mut alphabet = HashSet::new();

	alphabet.extend(others.chars());

	if uppercase {
		alphabet.extend(alphabet_uppercase.chars());
	}

	if lowercase {
		alphabet.extend(alphabet_lowercase.chars());
	}

	if numbers {
		alphabet.extend(alphabet_numbers.chars());
	}

	if alphabet.is_empty() {
		return String::new();
	}

	let alphabet: Vec<char> = alphabet.into_iter().collect();
	let mut result = String::new();

	for _ in 0..length {
		result.push(*alphabet.choose(&mut OsRng).expect("internal error"));
	}

	result
}


// Returns the current unix timestamp in nanoseconds.
// Our library won't handle time before the unix epoch, so we return u64.
// NOTE: This will panic if used past ~2500 C.E. (Y2K taught me nothing).
fn unix_timestamp() -> u64 {
	let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("internal error");
	timestamp
		.as_secs()
		.checked_mul(1000000000)
		.expect("internal error")
		.checked_add(timestamp.subsec_nanos() as u64)
		.expect("internal error")
}


#[cfg(test)]
mod tests {
	use super::{random_string, Database, DatabaseObject, Directory, Entry, EntryHistory, ID};
	use rand::{
		distributions::{uniform::SampleRange, Standard},
		rngs::OsRng,
		thread_rng, Rng,
	};
	use std::collections::HashMap;
	use tempfile::tempdir;

	pub(crate) fn quick_sleep() {
		std::thread::sleep(std::time::Duration::from_nanos(1));
	}

	pub(crate) fn random_uniform_string<R: SampleRange<usize>>(range: R) -> String {
		thread_rng().sample_iter::<char, _>(Standard).take(thread_rng().gen_range(range)).collect()
	}

	#[test]
	fn encrypt_then_decrypt() {
		let password_len = OsRng.gen_range(0..64);
		let password: String = (0..password_len).map(|_| OsRng.gen::<char>()).collect();
		let tmp_dir = tempdir().unwrap();

		let mut db = Database::new_with_password("username", &password);
		db.new_entry();
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), &password).unwrap();

		assert_eq!(db, db2);
		db.new_entry();
		assert!(db != db2);
	}

	#[test]
	fn password_change() {
		let tmp_dir = tempdir().unwrap();

		// Create DB
		let mut db = Database::new_with_password("username", "password");
		let old_file_key_suite = db.file_key_suite.clone();
		let old_sync_parameters = db.sync_parameters.clone();

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new(
			[("title".to_string(), "Password change".to_string())].iter().cloned().collect(),
		));
		db.add_entry(entry);

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Password change should change file encryption keys, even if using the same password
		db.change_password("username", "password");
		assert_ne!(db.file_key_suite, old_file_key_suite);

		// Password change should not change network keys if using the same password
		assert_eq!(db.sync_parameters, old_sync_parameters);

		// Changing username should change network keys even if using the same password
		db.change_password("username2", "password");
		assert_ne!(db.sync_parameters.get_login_key(), old_sync_parameters.get_login_key());
		assert_ne!(db.sync_parameters.get_login_id(), old_sync_parameters.get_login_id());
		assert_ne!(db.sync_parameters.get_network_key_suite(), old_sync_parameters.get_network_key_suite());

		// Password change should change all keys if username and/or password are different
		db.change_password("username", "password2");
		assert_ne!(db.file_key_suite, old_file_key_suite);
		assert_ne!(db.sync_parameters, old_sync_parameters);

		// Save
		db.save_to_path(tmp_dir.path().join("test2.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "password").unwrap();
		let db3 = Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password2").unwrap();
		Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password").expect_err("Shouldn't be able to load database with old password");

		assert_eq!(db.objects, db2.objects);
		assert_eq!(db.objects, db3.objects);
	}

	// Just some sanity checks on our keys
	#[test]
	fn key_sanity_checks() {
		let db = Database::new_with_password("username", "password");
		let db2 = Database::new_with_password("username", "password");

		assert!(db != db2);
		assert_ne!(db.file_key_suite, db2.file_key_suite);
		assert_eq!(db.sync_parameters, db2.sync_parameters);
	}

	#[test]
	fn test_random_string() {
		assert!(random_string(27, true, true, true, "$%^").len() == 27);
		assert!(random_string(1, true, true, true, "$%^").len() == 1);
		assert!(random_string(10, false, true, true, "$%^").len() == 10);
		assert!(random_string(11, false, false, true, "$%^").len() == 11);
		assert!(random_string(20, false, false, false, "$%^").len() == 20);

		assert!(random_string(10000, true, false, false, "").contains("A"));
		assert!(random_string(10000, false, true, false, "").contains("a"));
		assert!(random_string(10000, false, false, true, "").contains("0"));
		assert!(random_string(10000, false, false, false, "%").contains("%"));

		assert!(!random_string(10000, true, false, false, "").contains("a"));
		assert!(!random_string(10000, false, true, false, "").contains("A"));
		assert!(!random_string(10000, false, false, true, "").contains("A"));
		assert!(!random_string(10000, false, false, false, "$%^&").contains("A"));
	}

	#[test]
	fn test_random_string_randomness() {
		// A simple randomness test on random_string.
		// We know the source is good (OsRng) but this makes sure our use of it is correct.
		// TODO: Not sure if my chi-squared formulas are correct.
		let mut bins = HashMap::new();
		let string = random_string(100000, true, true, true, "0%");

		assert!(string.len() == 100000);

		for c in string.chars() {
			*bins.entry(c).or_insert(0) += 1;
		}

		let mut chi_squared = 0.0;
		let e = string.len() as f64 / 63.0;

		for (_, o) in &bins {
			chi_squared += ((*o as f64 - e) * (*o as f64 - e)) / e;
		}

		// >335.9 will basically never happen by chance
		assert!(chi_squared < 335.9);
	}

	// Make sure database can handle Unicode characters everywhere
	#[test]
	fn test_unicode() {
		let tmp_dir = tempdir().unwrap();

		// Unicode in username and password
		let username: String = (0..256).map(|_| OsRng.gen::<char>()).collect();
		let password: String = (0..256).map(|_| OsRng.gen::<char>()).collect();
		let mut db = Database::new_with_password(&username, &password);

		// Unicode in entries
		let a: String = (0..256).map(|_| OsRng.gen::<char>()).collect();
		let b: String = (0..256).map(|_| OsRng.gen::<char>()).collect();
		let c: String = (0..256).map(|_| OsRng.gen::<char>()).collect();

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new(
			[
				(
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
				),
				(
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
				),
				(a.clone(), b.clone()),
			]
			.iter()
			.cloned()
			.collect(),
		));
		entry.edit(EntryHistory::new(
			[
				(
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
				),
				(
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
				),
				(
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
					(0..256).map(|_| OsRng.gen::<char>()).collect::<String>(),
				),
				(a.clone(), c.clone()),
			]
			.iter()
			.cloned()
			.collect(),
		));
		db.add_entry(entry);

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), &password).unwrap();
		assert_eq!(db, db2);
		assert_eq!(db2.sync_parameters.get_username(), username);

		let entry_id = db2.get_root().list_entries(&db2)[0];
		let entry = db2.get_entry_by_id(entry_id).unwrap();

		assert_eq!(entry[&a], c);
	}

	#[test]
	fn test_empty() {
		let tmp_dir = tempdir().unwrap();

		// Test empty password
		let mut db = Database::new_with_password("", "");

		// Test empty entry
		db.add_entry(Entry::new());

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "").unwrap();
		assert_eq!(db, db2);

		let entry_id = db2.get_root().list_entries(&db2)[0];
		let entry = db2.get_entry_by_id(entry_id).unwrap();

		assert_eq!(entry.get_state().len(), 0);

		// Test empty database
		let db = Database::new_with_password("username", "foobar");

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar").unwrap();
		assert_eq!(db, db2);

		assert_eq!(db2.objects.len(), 1);
	}

	// Integration test on the whole Database
	// Simulate typical usage of Database, exercising as many features as possible, and make sure Database operates correctly.
	#[test]
	fn test_database() {
		let tmp_dir = tempdir().unwrap();

		// Build database
		let mut db = Database::new_with_password("username", "foobar");

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		db.add_entry(entry);

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new(
			[("title".to_string(), "Test test".to_string()), ("username".to_string(), "Username".to_string())]
				.iter()
				.cloned()
				.collect(),
		));
		db.add_entry(entry);

		let mut entry = Entry::new();
		let tmp_entry_id = entry.get_id().clone();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new(
			[("title".to_string(), "Test test".to_string()), ("username".to_string(), "Username".to_string())]
				.iter()
				.cloned()
				.collect(),
		));
		entry.edit(EntryHistory::new(
			[
				("username".to_string(), "Username".to_string()),
				("title".to_string(), "Ooops".to_string()),
				("password".to_string(), "Password".to_string()),
			]
			.iter()
			.cloned()
			.collect(),
		));
		db.add_entry(entry);

		db.get_root_mut().remove(tmp_entry_id.clone());
		db.get_root_mut().add(tmp_entry_id.clone());

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let mut db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar").unwrap();
		assert_eq!(db, db2);

		// Edit
		let entry_id: ID = **db2
			.get_root()
			.list_entries(&db2)
			.iter()
			.find(|id| {
				let entry = db2.get_entry_by_id(id).unwrap();

				entry.get("title") == None
			})
			.unwrap();

		{
			let entry = db2.get_entry_by_id_mut(&entry_id).unwrap();
			entry.edit(EntryHistory::new(
				[("title".to_string(), "Forgot this one".to_string())].iter().cloned().collect(),
			));
		}

		// Save
		db2.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db3 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar").unwrap();
		assert_eq!(db2, db3);

		for id in db3.get_root().list_entries(&db3) {
			let entry = db3.get_entry_by_id(id).unwrap();
			let title = entry.get("title");

			match title.map(|t| t.as_str()) {
				Some("Forgot this one") => {
					assert_eq!(entry.get_history().len(), 1);
				},
				Some("Test test") => {
					assert_eq!(entry.get("username").unwrap(), "Username");
					assert_eq!(entry.get_history().len(), 1);
				},
				Some("Ooops") => {
					assert_eq!(entry["username"], "Username");
					assert_eq!(entry.get_state()["password"], "Password");
					assert_eq!(entry.get_history()[0].data["username"], "Username");
					assert_eq!(entry.get_history()[1].data.get("username"), None);
					assert_eq!(entry.get_history()[0]["title"], "Test test");
				},
				_ => {
					panic!("Unknown title");
				},
			}
		}
	}

	// Test to make sure serialization is fully deterministic (the same database object serializes to the same string every time)
	#[test]
	fn entry_deterministic_serialization() {
		// Create entry
		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new(
			[
				("title".to_string(), "Serialization".to_string()),
				("username".to_string(), "Foo".to_string()),
				("password".to_string(), "password".to_string()),
				("url".to_string(), "Url".to_string()),
			]
			.iter()
			.cloned()
			.collect(),
		));
		entry.edit(EntryHistory::new(
			[("title".to_string(), "Title change".to_string()), ("url".to_string(), "Url change".to_string())]
				.iter()
				.cloned()
				.collect(),
		));

		let object = DatabaseObject::Entry(entry);

		// Serialize
		let serialized = serde_json::to_string(&object).unwrap();

		// Serialize a number of times to ensure it is always the same
		for _ in 0..64 {
			// We deserialize into a new copy to force internal HashMap rngs to re-randomize.
			let copy: DatabaseObject = serde_json::from_str(&serialized).unwrap();

			let other = serde_json::to_string(&copy).unwrap();

			assert_eq!(serialized, other);
		}
	}

	// Test to make sure serialization is fully deterministic (the same database object serializes to the same string every time)
	#[test]
	fn directory_deterministic_serialization() {
		// Create directory
		let mut directory = Directory::new();

		let id1: ID = OsRng.gen();
		let id2: ID = OsRng.gen();
		let id3: ID = OsRng.gen();

		directory.add(id1.clone());
		quick_sleep();
		directory.add(id2.clone());
		quick_sleep();
		directory.add(id3.clone());
		quick_sleep();
		directory.remove(id2.clone());
		quick_sleep();
		directory.remove(id3.clone());
		quick_sleep();
		directory.add(id2.clone());

		let object = DatabaseObject::Directory(directory);

		// Serialize
		let serialized = serde_json::to_string(&object).unwrap();

		// Serialize a number of times to ensure it is always the same
		for _ in 0..64 {
			// We deserialize into a new copy to force internal HashMap rngs to re-randomize.
			let copy: DatabaseObject = serde_json::from_str(&serialized).unwrap();

			let other = serde_json::to_string(&copy).unwrap();

			assert_eq!(serialized, other);
		}
	}

	// This test makes sure that the way we test hashmap is confirms non-determinism, so we know the deterministic_serialization test will work properly.
	#[test]
	fn hashmap_is_not_deterministic() {
		let mut hashmap: HashMap<&str, &str> = HashMap::new();

		hashmap.insert("foo", "bar");
		hashmap.insert("dog", "cat");
		hashmap.insert("excel", "determinism");
		hashmap.insert("you", "random");

		let serialized = serde_json::to_string(&hashmap).unwrap();
		let mut differ = false;

		for _ in 0..64 {
			// Need to deserialize into a new copy so we "re-initalize" the HashMap's Rng.
			let deserialized: HashMap<String, String> = serde_json::from_str(&serialized).unwrap();

			if serialized != serde_json::to_string(&deserialized).unwrap() {
				differ = true;
				break;
			}
		}

		assert_eq!(differ, true);
	}

	// TODO: Add a test that contains a pre-serialized database and which deserializes it to ensure that we don't accidentally change the serialization formats.
}
