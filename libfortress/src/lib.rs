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

	/// If password is changed, this is set to the old sync parameters until the server is successfully told about the change.
	old_sync_parameters: Option<SyncParameters>,
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
			old_sync_parameters: None,
		}
	}

	pub fn change_password<A: AsRef<str>, B: AsRef<str>>(&mut self, username: A, password: B) {
		let username = username.as_ref();
		let password = password.as_ref();

		let encryption_parameters = Default::default();
		self.file_key_suite = FileKeySuite::derive(password.as_bytes(), &encryption_parameters).expect("Internal error: Scrypt parameters were invalid.");

		self.old_sync_parameters = Some(self.sync_parameters.clone());

		// TODO: Derive in a background thread
		self.sync_parameters = SyncParameters::new(username, password);
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

		if old_parent == Some(*new_parent) {
			return;
		}

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

	pub fn load_from_reader<P: AsRef<str>, R: io::Read>(password: P, reader: &mut R) -> Result<Database, FortressError> {
		let password = password.as_ref();

		// This struct is needed because Database has fields that aren't part of
		// serialization, but can't implement Default.
		#[derive(Deserialize)]
		struct SerializableDatabase {
			objects: DatabaseObjectMap,
			sync_parameters: SyncParameters,
			sync_url: Option<Url>,
			old_sync_parameters: Option<SyncParameters>,
		}

		// Read file and decrypt
		let (plaintext, file_key_suite) = fortresscrypto::decrypt_from_file(reader, password.as_bytes())?;

		// Deserialize
		let db: SerializableDatabase = serde_json::from_slice(&plaintext)?;

		// Keep encryption keys for quicker saving later
		Ok(Database {
			objects: db.objects,
			sync_parameters: db.sync_parameters,

			file_key_suite,
			sync_url: db.sync_url,
			old_sync_parameters: db.old_sync_parameters,
		})
	}

	pub fn load_from_path<P: AsRef<Path>, A: AsRef<str>>(path: P, password: A) -> Result<Database, FortressError> {
		let file = File::open(path)?;
		let mut reader = BufReader::new(file);

		Self::load_from_reader(password, &mut reader)
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

		// If password was previously changed, tell the server first
		if let Some(old_sync_parameters) = &self.old_sync_parameters {
			self.sync_api_update_login_key(&client, url, old_sync_parameters)?;
			self.old_sync_parameters = None;
		}

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

	/// Tell the server about a change in our LoginKey
	fn sync_api_update_login_key(&self, client: &reqwest::blocking::Client, url: &Url, old_sync_parameters: &SyncParameters) -> Result<(), ApiError> {
		let body = self.sync_parameters.get_login_key().0.to_vec();
		let url = url.join("/user/login_key").expect("internal error");
		let test_url = url.join("/objects").expect("internal error");

		match api_request(
			client,
			old_sync_parameters.get_login_id(),
			old_sync_parameters.get_login_key(),
			Method::POST,
			url,
			body,
		) {
			Ok(_) => Ok(()),
			Err(ApiError::ApiError(401, _)) => {
				// It's possible the server already knows about the new key, let's check by doing a test request
				api_request(
					client,
					self.sync_parameters.get_login_id(),
					self.sync_parameters.get_login_key(),
					Method::GET,
					test_url,
					"",
				)
				.map(|_| ())
			},
			Err(err) => Err(err),
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
	ApiError(u16, String),
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
			ApiError::ApiError(_, err) => write!(f, "API error: {}", err),
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
		let status = response.status();
		let error = response.text()?;
		Err(ApiError::ApiError(status.into(), error))
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
	use std::{collections::HashMap, io::Cursor};
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

	// This test contains a pre-serialized database and deserializes it to ensure that we don't accidentally change the serialization formats.
	#[test]
	fn database_deserialization() {
		let serialized = b"\x66\x6f\x72\x74\x72\x65\x73\x73\x32\x00\x08\x08\x00\x00\x00\x01\x00\x00\x00\x63\xea\x66\xbf\x64\x6c\x7b\x3f\xed\x98\xd3\xc8\x59\x50\x81\xe3\x2e\x16\xab\x51\x1c\x67\xb8\x71\x8a\x35\x3f\xfb\x86\x8c\xf5\x4f\x95\xa6\x6f\x32\x6f\x21\x99\xc5\xd8\xa3\x1c\x21\x27\xb4\x9d\xdb\xcf\x5a\x6f\xf5\xea\x1f\x19\x5f\x83\x6b\x15\x7a\x1d\x40\x0f\x26\x57\x89\x77\xc7\x99\x70\x3a\x53\xc3\xb5\x90\xd7\x7e\xe6\xa0\x75\xee\x5b\xf4\x53\x45\xe7\xa6\x0b\x2f\x06\xad\xca\xb1\x7c\xeb\x5a\xc5\x89\x14\x74\xec\x81\xc3\x14\x84\x16\x4c\x98\x6c\x7d\xfa\xa4\x0a\x86\xb1\x2b\xf6\x10\x32\xfe\xd4\x7d\x7a\xbd\xb5\x02\xce\xcb\x4b\x7d\x99\xc4\x78\x0b\x68\xfd\x0e\x28\x8e\x58\x4f\x58\x79\x78\xd4\x48\xc1\x5b\x87\x9d\xce\x6f\xf4\x14\x02\xe0\xec\xa6\x1a\xa7\xd8\x9a\xa9\xaa\x31\x93\x74\x7e\xcd\x7c\x27\x1d\xa0\xe2\xf3\xa1\x3a\xca\x10\x88\xc3\xea\x81\x2b\xf5\x4d\xda\xa3\x5a\x45\x24\x48\x47\xf9\xce\xd0\xbd\x24\xde\xe9\xaf\xfe\x2d\x5e\x1c\x9d\x48\x12\xe5\x0f\x79\xaa\x65\xd3\xea\x30\xc1\x14\x6c\x9b\x01\xf1\x84\x5b\x30\xda\x84\xac\xb9\x38\x90\xc4\xe0\xac\x83\x9a\x6e\xa7\xfe\x09\x35\xbf\x2c\xc6\xd5\xfc\x3b\xf8\xfe\xb8\xa8\x2e\x3c\x3d\xb7\x2b\x63\xd0\xb1\x15\x9c\xec\xb6\x60\xdf\xc5\x4c\x4b\x65\x14\x95\xdd\xa0\x31\x9f\xcc\x0f\x9b\xe4\xff\x6e\xcd\x67\x85\x1f\x44\x69\x16\x2c\x30\xe5\xfc\x9c\x35\x80\x91\x16\x81\x74\x58\x17\xf5\x32\x23\x3a\x5a\xa9\x21\xcd\x6c\xd5\x6f\xbd\xc9\xc2\x3f\x9c\x7c\x5b\x42\x68\xea\xfe\x7f\xa4\x97\xc1\x97\x61\x1f\x5f\x9a\x8e\x47\x09\xcd\xf7\x24\x7c\x8e\xaa\xff\x92\x45\x19\xa3\x46\x5f\xa5\x78\x9b\xca\x48\xc0\x03\x2b\x94\x6b\x4f\x30\xd5\xb1\x4c\x02\x91\x01\xea\xe1\x16\xd8\xd7\x57\x4a\xf9\x35\xb5\xf8\xf5\x17\x4c\x36\x61\x9c\xcb\xcf\x1f\xb5\x52\x00\xe4\x28\x19\x40\x57\x3b\x5d\x65\x7a\xd0\x7d\x2e\x1e\x8c\x66\xe8\xcc\x09\x70\xbd\x60\x5f\x88\x11\x80\xe2\x6a\xab\x4f\x20\x90\x6f\x0c\xec\x53\xca\xb2\x42\x71\x0e\x3f\x2a\xf4\x92\x4b\x33\xee\xaf\xce\x68\xf9\x90\x3f\xd0\x49\x3e\x13\x06\x97\x0a\x85\xb8\x77\xab\xac\xa5\x2a\x88\x34\xc1\xb1\xc3\x91\x12\x1b\x8a\xa9\x1e\x13\x78\x3b\x44\xd6\x53\xa4\xda\x65\x03\x14\x04\x28\xe5\x39\x5c\x5c\xcf\xf3\x02\xca\x94\x00\xdc\x6f\x27\xf6\x75\xa2\x5c\x94\xc9\xdf\x71\xdc\x3a\xe9\xaf\xf2\xc2\xf8\xea\xef\x8f\x13\x5c\x51\x92\xf7\x17\xcb\xd1\x87\xb2\x0a\x91\x40\x5d\x8a\x23\xac\x00\x3c\x8e\x9c\xfa\xe1\x71\x89\x10\x3e\xe2\xe2\xe2\x36\x85\x6e\x30\x98\xdb\x03\x51\x2e\xdb\x9a\x93\x38\x38\x57\x36\x00\x58\x5d\x6b\xb1\x7c\xcd\xac\x95\x30\xc1\x8c\x0d\x97\xa0\x94\x46\x35\xee\xcd\x80\xb8\xda\xa6\x79\xe9\xb1\xfb\x12\x72\x7c\xf3\xd0\x44\x84\x38\x15\x06\xfd\xcc\x14\x6f\x37\xb9\x20\x44\x90\x32\xce\x9f\x94\xed\x97\x82\x87\x15\xfb\x2c\x7a\x06\x5d\x71\x57\x56\x16\xb5\x75\xc6\x3e\x29\x2b\x1c\x77\x18\xd5\x38\xa1\xf1\xc7\xd2\x58\xbc\x91\xf2\xc4\x01\x32\x87\xb1\x8c\xcd\xec\x7d\xf9\x7a\x3d\xa1\xbf\x08\x6b\x34\x80\xf0\x3f\x31\xf7\x91\xd5\x72\xeb\x39\xc3\x51\x90\xc6\x9a\x6a\x54\x8f\x87\xf2\xed\xbd\x9e\x9b\x36\x20\x02\xca\x66\x96\x7e\xbd\x50\x95\xfe\x0d\xee\xc2\x64\xc4\x06\x26\xe7\xa0\x22\x29\x5d\x2f\xdd\xe3\x8f\x66\x27\x7d\xba\x3f\x35\x31\xe4\xa8\x33\x8e\x78\x79\xb4\x7f\x91\xd1\x5d\xee\xde\x11\x32\x4b\x64\x71\x32\x60\x49\xb9\x1c\x7e\xc1\x4d\xc6\x70\xa0\x52\x4e\x37\xda\x95\xad\xc3\x94\x0d\xfd\x70\x0a\xdd\x4a\x4e\x29\x72\xe5\x84\xdf\x7d\x26\x03\xec\xc2\x42\x84\xfd\xc0\x5d\x73\xf0\x42\x3c\x78\x1d\xbe\x30\x6e\xb8\x05\x13\x5b\x93\x13\x2a\xe6\x6b\x7a\xa1\x4f\x8a\x61\xd7\xda\x19\xf3\x80\xf2\x00\x96\x93\xbd\x44\xe9\xb5\x34\x18\x5f\x92\x51\x03\x52\x76\xe4\xbd\xa8\x04\x20\x61\x6e\xfd\x47\x4e\xd9\xe0\xe0\x43\xc5\x2d\x1f\x8f\x4d\x66\xcf\x84\x23\x0a\xb8\xe6\x69\xbe\x7f\x8e\xbd\xf1\xdb\x06\x8e\x7d\xc1\x77\xc2\x67\x0d\x2a\xa7\x73\x6b\xac\x24\xe2\xfe\x47\x00\x78\x73\x8f\x88\x63\x09\xa5\xee\x9a\xe9\xd4\x5f\xd9\x80\x56\x6b\xc7\xfa\x2d\xe7\xc1\x49\xc9\x56\xbe\xd5\xfd\x03\x65\xb7\xd9\xb4\x03\xe5\x07\xe5\x2d\x75\x06\xb8\xc1\x66\xd5\x69\xb5\x4c\x9d\xda\x9d\x1f\xb0\xdf\xe5\xf0\xe2\x45\xce\x5c\xeb\xff\xff\x5d\x0c\xc1\xbf\xf8\x4a\x33\xcd\x59\xac\xbb\x89\x7a\x06\x7a\x86\x96\xd9\x18\x0f\xbd\xa4\x80\x1f\xbe\xb7\x2b\x18\xd9\x27\x8b\xd0\xab\xf9\x63\x20\x2e\x23\x1c\xa9\xb4\xf1\x67\x45\x60\x4e\x28\x67\x06\x25\x36\x05\x13\xf7\x86\x35\x26\xf6\x57\x62\xae\xd1\x3e\xdd\x05\xcf\x56\x4f\x85\xae\x52\xcd\x55\x41\xa5\xf5\x2f\x8c\xcb\xd2\xf2\x0c\xa5\x90\x3f\xd5\x7d\x60\xde\xe1\x12\x32\x78\xb8\x74\xb6\x3c\x8b\xfa\x96\x2a\x64\xa9\x7b\xbf\xba\x28\x69\x8e\x27\x3b\x7b\xdb\x7c\xae\xad\xaf\x37\xe7\x01\x49\x77\x3e\xfe\x7a\x44\x5e\x7b\x52\xc5\xae\x80\xa6\xad\x82\xbc\xf1\x2f\x80\x1d\x6d\x9a\x82\x83\x97\x84\x27\xc4\x68\x84\x2b\x33\xde\xc2\x68\xf6\x27\x9d\x87\xe2\x76\x87\xbd\x81\x9d\xef\xed\x8e\x1d\x03\x04\x2e\xc9\x86\xee\x23\x19\x33\x35\xb7\xba\x44\x90\x61\x4e\xa8\x1f\x99\x07\x23\x02\xd2\xb2\xbd\x35\x30\x87\x73\x59\xae\x80\x3e\xc2\xe4\xf0\xfd\x2d\x54\x87\x59\x95\x50\xfb\x9c\x1e\x26\xa5\x95\xcf\x24\xb6\x8d\x7a\xba\x27\x1b\x9d\xfd\x55\x26\x63\xec\xde\x16\xcc\x38\xa4\x03\xb1\xa6\xac\xe6\x82\x0f\x0c\x0f\x30\x4d\x31\x3e\xa8\xeb\x8d\x73\x32\x4e\x13\xfe\x07\xf7\x29\xe3\x63\x9c\x2d\x84\x9f\x37\xd1\x58\x46\xc8\x79\x10\x35\x05\xf5\xb8\xae\x0a\x43\xdc\xf1\x04\x32\xd7\x45\x23\xf2\x14\x23\xc9\x00\x9d\x29\xad\xe4\x8b\x59\xd4\x7a\x8c\x10\x5a\xaf\x89\x8b\x0b\xd0\xf7\xfd\xec\xfb\x2a\x89\xde\x61\x42\x90\x31\xca\x22\xe2\xc6\x3a\x3d\x53\x5c\x9e\x71\xd5\x77\xd7\x21\x3e\x99\x6e\x2c\xa8\xa8\xc7\x3c\x9b\x83\xe6\x69\x61\xaf\x77\x8e\x67\x6f\xfe\xb3\x5d\xb6\x28\xf6\x38\xa7\x87\x91\x4e\xf9\xa0\x51\xde\x74\x7c\x09\x1b\xf2\xdb\x7c\x3d\x53\x13\x16\x2c\x68\xc7\x38\x55\x14\xc6\x68\xa3\xe1\xc6\x52\x49\xa7\xfc\xd2\x63\xee\xe2\x9b\x9c\x04\xdc\x6f\xb5\x93\x6f\x1c\x35\x06\x52\x9b\xd6\x6e\xed\x07\xe2\x41\x36\x9c\x39\xe8\xb3\xcd\x8f\x7b\x03\x81\x24\x74\x38\x27\xe6\x86\xb7\x45\xdc\x5f\x2c\x29\x2a\xbd\x46\x3b\xc3\x67\x34\x21\x4b\x8c\xf4\xa1\x1e\x00\xf5\x86\x6b\xe1\x5e\xea\x06\x96\x9e\x67\xdb\xee\xab\x51\x4e\x1f\xcb\x8f\x4d\x2f\xc3\x9a\x56\xfc\x9c\x5b\xd9\xa5\x97\xa4\x92\x88\xc0\x68\x7b\x7d\xa1\x0a\x93\x65\xbf\x32\x1e\xc7\x0a\x4d\xad\xa4\xf0\x6b\xfc\xf2\xb3\xaa\x05\x90\xf9\x76\xb3\xdb\xcb\x5d\x2a\x65\x86\xea\x3a\xa9\x20\xb8\x7a\x37\x74\x0b\xfa\x41\x30\x78\x4f\x8b\x26\xad\x92\xa5\x09\xd9\x09\x97\xea\x59\xa8\xb5\xb1\x89\x2f\x10\x75\x65\xb6\x0b\x77\x47\x0c\xae\x24\xb5\xe1\x36\x71\xeb\x3e\xf9\xc3\xdf\x26\x7b\xc2\xa2\x52\x55\xc6\x35\x2b\x9c\xcb\xfc\x64\xf4\x4d\x86\x33\x83\x38\x05\x72\x39\x61\x0b\x57\xfb\x38\x6f\x40\x6b\xd9\xba\xc9\x71\x68\xc8\xb9\xb1\x42\xae\x02\xa4\xd5\xf2\x86\x45\xc5\xfb\x1a\x98\x0d\x04\xa5\x17\x45\x80\x4c\x0b\x55\xc7\xba\x57\xd4\x5a\xca\xb4\x2a\x8b\xc0\xbd\xf9\x4b\x9e\x38\xd9\x13\x06\x0f\xb0\xec\x1b\x06\x68\x6c\x55\x4a\x6f\xfd\xf2\x90\xc3\xae\x3b\x8a\xc8\xae\x66\xc9\xfe\x0d\x8c\x1e\xf3\x9a\x69\x48\x47\x53\x19\x02\x6f\x62\x31\x2e\xbc\x08\xaa\x07\x05\xa8\x85\x1a\x3b\x23\x8c\x22\x0c\x71\x31\x11\x06\x60\x63\x39\xd5\x07\xa4\xc0\x09\x67\x94\xd1\xd0\x68\x1d\x26\x24\x48\x98\x89\x3f\x10\xe5\x2b\x1f\xe2\x84\x76\xe4\x42\xaa\x88\x25\xb7\x25\x63\xf2\xc8\x80\x27\xf5\x52\xc4\xc2\xd8\x4f\xd9\x90\x1c\xf8\x90\x14\x55\xed\x75\x7e\x4c\x9f\xc6\xf2\x75\x67\x4c\x84\x8d\x7d\x83\xee\x15\x9c\xa3\xf3\xfd\x3b\x0a\xe5\x0c\xc2\x8f\x28\xbc\x8b\x5f\x72\x87\x25\xf7\xbc\xdc\x1b\x5a\x95\xe6\x23\x48\x08\x73\xba\xe0\x4a\xfb\xfe\x9f\x41\x7f\x69\x0a\x69\x1d\xc6\xab\x75\x59\xd2\x79\xd5\xf4\x73\xa2\x98\xd6\x6b\xaf\x44\x0f\xdd\x64\x65\xe7\x0f\x28\xa9\x20\x05\x55\xce\xfa\x71\xd4\x49\xb3\xd7\xb0\x15\xbe\xb4\xa5\x15\xc8\x7b\x91\x02\x59\x4d\x84\x48\xfd\x22\xf9\x14\xa5\xda\x88\xf8\x68\xc5\x8d\x33\x54\x27\x2c\xdc\xbf\x17\xc2\x62\x4d\x4e\xcc\x01\xbe\x81\x0f\x0f\xcc\x88\x59\xc8\x85\xcb\x87\xd4\xac\xdc\x24\x02\x32\xc0\x39\x80\x91\x12\x40\xf9\x89\x53\xe6\x55\x00\xb4\x68\xb4\x54\xfa\xaf\x3d\xb2\xb7\x78\xdd\x3e\xcf\xea\x40\x39\xc7\x50\x87\xf6\x56\x4b\xcf\x1e\x08\x09\x75\x1f\x17\x05\x9c\x6a\xbd\x7a\xba\x63\xd2\x43\x33\x6a\xa1\x9e\x87\xe8\xa4\x0d\x03\x5f\xe4\xee\x76\x6f\xdf\x14\x16\xb3\xf4\xed\x6a\x60\xdf\x0f\x82\x28\x70\xf3\xd8\x25\x3c\x50\x12\x5a\xa0\xca\xe8\x64\x9e\xc4\x97\xa5\x90\x00\x3f\xd6\x9b\x4d\xc2\xa7\x9d\x5d\xfb\xf6\x96\xc3\x90\x02\x0c\xe2\xa7\x66\x23\xb9\xd6\x42\xde\xa9\xda\xef\x52\xbd\x31\xc0\x79\xde\x0d\xab\x5d\xd3\xf4\xc1\xe4\xa9\x75\xb2\x13\x69\x56\x80\x57\x59\x58\xbb\x04\x9b\xa8\x3f\x31\x0d\x51\xc7\x7f\x53\x7e\x89\x37\x19\xff\x3d\x4e\x43\x94\x84\x59\x81\x1e\x51\xb7\xd5\xc2\x56\x39\x3b\x2e\x1d\x85\xbe\xdf\xf8\xa9\x23\x07\x01\x0d\x29\xf0\x97\x1e\x5e\x47\xb1\xec\x63\x65\x10\xc1\xe7\xcc\xc0\xa6\xc1\xf2\x57\xe4\x0c\x9c\x11\x15\x87\xc4\x99\x59\xbe\xbe\x6f\x00\x4d\xb3\x48\x04\x6b\x65\x46\x6f\x78\x4a\x42\x15\x83\x41\x28\xdd\x5d\x5d\x8b\x06\x78\x9b\xe6\x5f\xf5\x82\x94\x7c\xf5\x86\x42\x01\x0a\x38\x96\x98\x31\x78\x0d\x39\xa8\x13\x22\x40\xb2\x45\x46\x5d\xb6\x6a\xdb\xce\x57\xae\x8f\x97\x26\xd6\xfe\x03\xa0\xf5\x75\xd5\x43\x8f\xa0\x7d\xac\x55\xb4\xb6\x4d\x17\x16\x7f\x49\x62\x9f\x10\x7a\xf4\xde\x68\x40\x5a\x17\xd7\x02\xfa\x69\xe0\x05\x6d\xda\xe0\xd6\x93\x2d\x2a\xd9\x21\x25\x7a\xef\xb9\xc3\x80\x43\x71\xd5\x50\x5b\x7b\xd5\x2f\xe0\x39\x12\x90\xc3\xb6\x7a\x78\x73\xd2\xe6\xb2\xfe\x8b\x89\xc8\x97\x09\x85\x9f\x7c\xb9\x30\x47\xb4\x81\x1c\x24\x38\x69\xc1\x52\xfb\x6d\xcb\xc5\xf3\xa1\x62\x09\x61\xb7\xe7\x4c\xec\x79\x5f\xff\xbc\x0d\x47\x35\x3a\x48\x9b\x8e\x6b\x78\x8d\x86\x00\x9a\x7c\xf3\x63\xe3\x84\x56\xdc\xa9\x21\xda\x7e\x6a\xdf\xd3\x42\xee\xa3\x3d\xe0\x3c\x9f\x95\x4e\x86\xf7\x44\x83\xf2\x35\x5a\x22\x1a\x7b\x8b\xac\x93\xfe\x4e\xc8\x2a\x00\xcf\x4b\x41\x19\xf6\xde\x71\xff\x89\xb6\x35\x13\xbe\xbc\x36\x54\x30\x21\x96\x1e\xb4\xb4\x4e\xc7\x29\x3b\x49\x10\x3c\xe7\x12\x72\xdf\x34\x6b\x33\x95\xbe\xcb\x17\x59\x22\xb8\xe2\x23\xba\x10\x87\xc8\x05\x07\xfc\x16\x20\x95\x5d\x3d\xf3\x3e\x87\xc9\x7d\x3b\x9b\x97\xf2\x03\x7c\x9b\x09\xb8\x60\x11\xb5\x36\x18\xd5\x5e\xfb\x70\x89\xc6\x7b\x8b\x58\x8f\x41\x67\x0f\x7c\x5c\xe9\xff\x3c\x27\xf5\xca\x2f\x3b\x24\xd8\x74\x78\x2d\x2c\xec\x7c\x81\x9f\xb2\xc3\x49\x01\xa0\xf2\x7c\xaf\xd0\xca\x92\x9f\xe1\xd8\xfd\x44\x37\xf4\x66\x4c\x2b\xbb\x29\x1c\x84\x7c\x55\xd0\x4d\xad\x80\x44\xdb\x6c\x27\x56\x7c\x4c\xf3\xc6\xac\x1c\xe8\x2e\x91\x14\xc2\x3e\xa9\x4c\x3f\xa2\x34\xb4\x21\x8b\x99\xd4\x6f\x84\x0c\x0a\x15\xcb\xcc\x21\xd5\xb5\x2d\xc8\x7c\xf1\x75\xaa\x38\x81\x8e\x6d\xb4\x70\x25\x53\x55\x4c\x6b";

		let database = Database::load_from_reader("test_password", &mut Cursor::new(serialized)).unwrap();

		// Check a few places to make sure the data is as expected
		let root_dir = database.get_root();
		let dir2 = database.list_directories().find(|d| d.get_id() != crate::ROOT_DIRECTORY_ID).unwrap();
		let entry1 = dir2
			.list_entries(&database)
			.into_iter()
			.map(|id| database.get_entry_by_id(&id).unwrap())
			.next()
			.unwrap();
		let entry2 = root_dir
			.list_entries(&database)
			.into_iter()
			.map(|id| database.get_entry_by_id(&id).unwrap())
			.next()
			.unwrap();

		assert_eq!(root_dir.get_name().unwrap(), "My Passwords");
		assert_eq!(dir2.get_name().unwrap(), "Test Dir");
		assert_eq!(entry2["notes"], "Notes for two");
		assert_eq!(entry1["username"], "first");
	}
}
