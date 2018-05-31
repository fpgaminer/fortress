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
// stored in the Database is never unintentionally lost.  By using this methodogly
// of enforcing non-destructive and other invariants we can drastically reduce
// the probability of bugs violating this intentions.
// 
// 
// NOTE: Changing any of these structs which derive Serialize/Deserialize requires
// bumping the database format version.
// 
// NOTE: No versioning is currently included for cloud objects.  The plan is to
// add versioning the next time the format changes, and to change the way the 
// network and login keys are calculated to prevent old versions from syncing.
// We can then have a plan for more graceful versioning going forward.
extern crate rand;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate data_encoding;
extern crate crypto;
extern crate byteorder;
extern crate tempfile;
pub extern crate fortresscrypto;
extern crate reqwest;

#[macro_use] mod newtype_macros;
mod database_object;
mod database_object_map;
pub mod sync_parameters;

pub use database_object::{Directory, Entry, EntryHistory};

use rand::{OsRng, Rng};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use std::str;
use fortresscrypto::{EncryptionParameters, FileKeySuite, LoginKey, MacTag};
use database_object::{DatabaseObject, DirectoryHistoryAction};
use database_object_map::DatabaseObjectMap;
use sync_parameters::{SyncParameters, LoginId};
use std::cmp;
use reqwest::{Url, IntoUrl};
use tempfile::NamedTempFile;


new_type!{
	public ID(32);
}


// TODO: Not sure if we want this to be cloneable?
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Database {
	objects: DatabaseObjectMap,
	root_directory: ID,

	sync_parameters: SyncParameters,

	#[serde(skip_serializing, skip_deserializing)]
	encryption_parameters: EncryptionParameters,
	#[serde(skip_serializing, skip_deserializing)]
	file_key_suite: FileKeySuite,
	#[serde(skip_serializing, skip_deserializing)]
	pub do_not_set_testing: bool,     // DO NOT SET TO true; used only during integration testing.
}

impl Database {
	pub fn new_with_password<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> Database {
		let username = username.as_ref();
		let password = password.as_ref();

		let encryption_parameters = Default::default();
		let file_key_suite = FileKeySuite::derive(password.as_bytes(), &encryption_parameters);

		// TODO: Derive in a background thread
		let sync_parameters = SyncParameters::new(username, password);

		let root = Directory::new();
		let root_directory = root.get_id().clone();
		let mut objects = DatabaseObjectMap::new();
		objects.update(DatabaseObject::Directory(root));

		Database {
			objects: objects,
			root_directory: root_directory,
			sync_parameters: sync_parameters,

			encryption_parameters: encryption_parameters,
			file_key_suite: file_key_suite,
			do_not_set_testing: false,
		}
	}

	pub fn change_password<A: AsRef<str>, B: AsRef<str>>(&mut self, username: A, password: B) {
		let username = username.as_ref();
		let password = password.as_ref();

		self.encryption_parameters = Default::default();
		self.file_key_suite = FileKeySuite::derive(password.as_bytes(), &self.encryption_parameters);

		// TODO: Derive in a background thread
		self.sync_parameters = SyncParameters::new(username, password);

		// TODO: Need to tell server about our new login-key using our old login-key
		// TODO: We should re-sync after this
	}

	pub fn get_username(&self) -> &str {
		self.sync_parameters.get_username()
	}

	pub fn get_root(&self) -> &Directory {
		match self.objects.get(&self.root_directory).unwrap() {
			&DatabaseObject::Directory(ref dir) => dir,
			_ => panic!(),
		}
	}

	pub fn get_root_mut(&mut self) -> &mut Directory {
		match self.objects.get_mut(&self.root_directory).unwrap() {
			&mut DatabaseObject::Directory(ref mut dir) => dir,
			_ => panic!(),
		}
	}

	pub fn new_entry(&mut self) {
		let entry = Entry::new();
		self.add_entry(entry);
	}

	pub fn add_entry(&mut self, entry: Entry) {
		self.get_root_mut().add(entry.get_id().clone());
		self.objects.update(DatabaseObject::Entry(entry));
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

	pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
		// Create a temporary file to write to
		let mut temp_file = {
			let parent_directory = path.as_ref().parent().ok_or(io::Error::new(io::ErrorKind::NotFound, "Bad path"))?;
			NamedTempFile::new_in(parent_directory)?
		};

		// Serialized payload
		let payload = serde_json::to_vec(&self)?;

		// Encrypt and write to the temporary file
		//let mut writer = BufWriter::new(&mut temp_file);
		fortresscrypto::encrypt_to_file(&mut BufWriter::new(&mut temp_file), &payload, &self.encryption_parameters, &self.file_key_suite)?;

		// Now close the temp file and move it to the destination.
		// Moving a temporary file is atomic (at least on *nix), so doing it this way
		// instead of writing directly to the destination file helps prevent data loss.
		let temp_path = temp_file.into_temp_path();
		temp_path.persist(path).map_err(|e| e.error)
	}

	pub fn load_from_path<P: AsRef<Path>, A: AsRef<str>>(path: P, password: A) -> io::Result<Database> {
		let password = password.as_ref();

		// This struct is needed because Database has fields that aren't part of
		// serialization, but can't implement Default.
		#[derive(Deserialize)]
		struct SerializableDatabase {
			objects: DatabaseObjectMap,
			root_directory: ID,
			sync_parameters: SyncParameters,
		}

		// Read file and decrypt
		let (plaintext, encryption_parameters, file_key_suite) = {
			let file = File::open(path)?;
			let mut reader = BufReader::new(file);

			fortresscrypto::decrypt_from_file(&mut reader, password.as_bytes())?
		};
		
		// Deserialize
		let db: SerializableDatabase = serde_json::from_slice(&plaintext)?;

		// Keep encryption keys for quicker saving later
		Ok(Database {
			objects: db.objects,
			root_directory: db.root_directory,
			sync_parameters: db.sync_parameters,

			encryption_parameters: encryption_parameters,
			file_key_suite: file_key_suite,
			do_not_set_testing: false,
		})
	}

	// TODO: Sync should be performed in a separate background thread
	// TODO: Instead of having library users call sync themselves, we should just an init method which sets up a continuous automatic
	// background sync.
	// TODO: Cleanup unwraps and add proper error handling
	// TODO: Need to sync a "Restore" object that other clients can use to bootstrap from
	// nothing.  This will tell them the root object id.
	// Returns true if the database has changed as a result of the sync.
	pub fn sync<U: IntoUrl>(&mut self, url: U) -> bool {
		let mut url = url.into_url().unwrap();
		if self.do_not_set_testing == false {
			url.set_scheme("https").unwrap();	// Force SSL
		}
		let client = reqwest::Client::new();

		// Diff existing objects
		let (updates, unknown_ids) = self.sync_api_diff_objects(&client, &url).unwrap();

		// Upload objects the server didn't know about
		for unknown_id in &unknown_ids {
			if let Some(object) = self.objects.get(unknown_id) {
				self.sync_api_update_object(&client, &url, object).unwrap();
			}
			else {
				println!("WARNING: Server named an unknown_id that we don't have in our database: {:?}", unknown_id);
			}
		}

		// Block needed to control lifetime
		let queued_updates = {
			// Decrypt and sort diffs into Entries and Directories
			let (entry_updates, directory_updates, reuploads) = self.sync_decrypt_and_sort_diffs(&updates);

			// Re-upload broken objects
			for object in reuploads {
				match self.sync_api_update_object(&client, &url, object) {
					Ok(_) => (),
					Err(err) => println!("WARNING: Error updating object on server: {:?}", err),
				}
			}

			// Merges
			let mut queued_updates = Vec::new();

			queued_updates.append(&mut self.sync_merge_entries(&entry_updates));
			queued_updates.append(&mut self.sync_merge_directories(&client, &url, &directory_updates));

			queued_updates
		};

		// TODO: This is just a proxy for whether changes occured and may tend to indicate true even
		// in cases where no changes happened.
		let database_changed = queued_updates.len() > 0;

		// Finally, merge queued updates into our local database
		for (new_object, should_upload) in queued_updates {
			// Update local DB
			self.objects.update(new_object.clone());

			if should_upload {
				match self.sync_api_update_object(&client, &url, &new_object) {
					Ok(_) => (),
					Err(err) => {
						println!("WARNING: Error uploading object to server: {:?}", err);
					}
				}
			}
		}

		database_changed
	}

	// Upload object to fortress server
	fn sync_api_update_object(&self, client: &reqwest::Client, url: &Url, object: &DatabaseObject) -> Result<(), ApiError> {
		#[derive(Serialize, Debug)]
		struct UpdateObjectRequest<'a,'b,'c,'d,'e> {
			user_id: &'a LoginId,
			user_key: &'b LoginKey,
			object_id: &'c ID,
			#[serde(with = "hex_format")] data: &'d [u8],
			data_mac: &'e MacTag,
		}

		// Encrypt
		let (ciphertext, mac) = {
			let payload = serde_json::to_vec(&object).unwrap();
			self.sync_parameters.get_network_key_suite().encrypt_object(&object.get_id()[..], &payload)
		};

		let request = UpdateObjectRequest {
			user_id: self.sync_parameters.get_login_id(),
			user_key: self.sync_parameters.get_login_key(),
			object_id: object.get_id(),
			data: &ciphertext,
			data_mac: &mac,
		};

		let _: ApiEmptyResponse = api_request(&client, url.join("/update_object").unwrap(), &request)?;
		Ok(())
	}

	// Diffs all existing objects in the database against the fortress server
	// Returns updates and IDs unknown to the server.
	fn sync_api_diff_objects(&self, client: &reqwest::Client, url: &Url) -> Result<(Vec<DiffObjectResponseUpdate>, Vec<ID>), ApiError> {
		#[derive(Serialize)]
		struct DiffObjectRequest<'a,'b,'c> {
			user_id: &'a LoginId,
			user_key: &'b LoginKey,
			objects: Vec<DiffObjectRequestObject<'c>>,
		}

		#[derive(Serialize)]
		struct DiffObjectRequestObject<'a> {
			id: &'a ID,
			mac: MacTag,
		}

		#[derive(Deserialize)]
		struct DiffObjectResponse {
			updates: Vec<DiffObjectResponseUpdate>,
			unknown_ids: Vec<ID>,
		}

		let mut diff_object_request = DiffObjectRequest {
			user_id: self.sync_parameters.get_login_id(),
			user_key: self.sync_parameters.get_login_key(),
			objects: Vec::new(),
		};

		for (id, object) in &self.objects {
			// Calculate MAC
			// TODO: This should be cached (and possibly saved out to file)
			let payload = serde_json::to_vec(object).unwrap();
			let (_, mac) = self.sync_parameters.get_network_key_suite().encrypt_object(&id[..], &payload);

			diff_object_request.objects.push(DiffObjectRequestObject {
				id: id,
				mac: mac,
			});
		}

		let response: DiffObjectResponse = api_request(&client, url.join("/diff_objects").unwrap(), &diff_object_request)?;

		Ok((response.updates, response.unknown_ids))
	}

	// Fetch an object from the server.
	// If the object doesn't exist on the server or could not be decrypted then None is returned.
	fn sync_api_get_object(&self, client: &reqwest::Client, url: &Url, id: &ID) -> Result<Option<DatabaseObject>, ApiError> {
		#[derive(Serialize)]
		struct GetObjectRequest<'a,'b,'c> {
			user_id: &'a LoginId,
			user_key: &'b LoginKey,
			object_id: &'c ID,
		}

		#[derive(Deserialize)]
		struct GetObjectResponse {
			#[serde(with = "hex_format")]
			data: Vec<u8>,
			mac: MacTag,
		}

		let request = GetObjectRequest {
			user_id: self.sync_parameters.get_login_id(),
			user_key: self.sync_parameters.get_login_key(),
			object_id: id,
		};

		let response: GetObjectResponse = api_request(&client, url.join("/get_object").unwrap(), &request)?;
		let ciphertext = [&response.data[..], &response.mac[..]].concat();

		match self.sync_parameters.get_network_key_suite().decrypt_object(&id[..], &ciphertext) {
			Ok(plaintext) => {
				Ok(Some(serde_json::from_slice(&plaintext).unwrap()))
			},
			Err(err) => {
				println!("WARNING: Error while decrypting server object(ID: {:?}): {}", id, err);
				Ok(None)
			}
		}
	}

	// Decrypt and sort diffs into Entries and Directories
	// Returns a list of entry updates, directory updates, and objects that need to be reuploaded due to errors
	fn sync_decrypt_and_sort_diffs(&self, updates: &[DiffObjectResponseUpdate]) -> (Vec<(&Entry, Entry)>, Vec<(&Directory, Directory)>, Vec<&DatabaseObject>) {
		let mut entry_updates = Vec::new();
		let mut directory_updates = Vec::new();
		let mut reuploads = Vec::new();

		for update in updates {
			// Get local object
			let local_object = match self.objects.get(&update.id) {
				Some(object) => object,
				None => continue,	// Ignore server's weirdness
			};

			// Decrypt
			let plaintext = {
				let ciphertext = [&update.data[..], &update.mac[..]].concat();

				match self.sync_parameters.get_network_key_suite().decrypt_object(&update.id[..], &ciphertext) {
					Ok(plaintext) => plaintext,
					Err(err) => {
						println!("WARNING: Error while decrypting server object: {}", err);

						// Fix using our copy
						reuploads.push(local_object);
						continue;
					}
				}
			};

			// Parse
			let server_object = serde_json::from_slice(&plaintext).unwrap();

			// Sort
			match local_object {
				&DatabaseObject::Entry(ref local_entry) => match server_object {
					DatabaseObject::Entry(server_entry) => entry_updates.push((local_entry, server_entry)),
					_ => {
						// This shouldn't happen, but we'll fix using our copy
						reuploads.push(local_object);
						continue;
					},
				},
				&DatabaseObject::Directory(ref local_directory) => match server_object {
					DatabaseObject::Directory(server_directory) => directory_updates.push((local_directory, server_directory)),
					_ => {
						// This shouldn't happen, but we'll fix using our copy
						reuploads.push(local_object);
						continue;
					},
				},
			}
		}

		(entry_updates, directory_updates, reuploads)
	}

	fn sync_merge_entries(&self, updates: &[(&Entry, Entry)]) -> Vec<(DatabaseObject, bool)> {
		let mut queued_updates = Vec::new();

		for &(local_entry, ref server_entry) in updates {
			// Merge
			let (new_entry, should_upload) = sync_merge_entry(local_entry, &server_entry);

			// Queue update
			queued_updates.push((DatabaseObject::Entry(new_entry), should_upload));
		}

		queued_updates
	}

	fn sync_merge_directories(&self, client: &reqwest::Client, url: &Url, updates: &[(&Directory, Directory)]) -> Vec<(DatabaseObject, bool)> {
		let mut queued_updates = Vec::new();

		// TODO: Sync doesn't support nested directories yet, so we will only sync the root directory.
		for &(local_directory, ref server_directory) in updates {
			if server_directory.get_id() != self.root_directory {
				panic!("ERROR: We do not currently support nested directories");
			}

			queued_updates.append(&mut self.sync_merge_directory_and_recurse(client, url, local_directory, &server_directory));
		}

		queued_updates
	}

	// Merge a directory update and also download new objects resulting from the merge
	// If we fail to download any of the new objects this function will return an empty vector
	fn sync_merge_directory_and_recurse(&self, client: &reqwest::Client, url: &Url, local_directory: &Directory, server_directory: &Directory) -> Vec<(DatabaseObject, bool)> {
		let mut queued_updates = Vec::new();

		let (new_directory, new_ids, should_upload) = sync_merge_directory(local_directory, server_directory, &self.objects);
		queued_updates.push((DatabaseObject::Directory(new_directory), should_upload));

		// Download new objects
		for new_id in new_ids {
			match self.sync_api_get_object(client, url, &new_id) {
				Ok(Some(object)) => {
					queued_updates.push((object, false));
				},
				Ok(None) => {
					println!("WARNING: Missing object from server ({:?})", new_id);
					return Vec::new();  // Back out of merging the directory, it would be incomplete otherwise
				},
				Err(err) => {
					println!("WARNING: Error fetching object from server ({:?}): {:?}", new_id, err);
					return Vec::new();  // Back out of merging the directory, it would be incomplete otherwise
				},
			}
		}

		queued_updates
	}
}


#[derive(Deserialize)]
struct DiffObjectResponseUpdate {
	id: ID,
	#[serde(with = "hex_format")]
	data: Vec<u8>,
	mac: MacTag,
}


#[derive(Deserialize)]
#[serde(untagged)]
enum ApiErrorResponse<T> {
	Error { error: String },
	Ok(T),
}

// Used when the api will either return an error or nothing.
#[derive(Deserialize)]
struct ApiEmptyResponse {}

#[derive(Debug)]
enum ApiError {
	ReqwestError(reqwest::Error),
	ApiError(String),
}

impl From<reqwest::Error> for ApiError {
	fn from(err: reqwest::Error) -> ApiError {
		ApiError::ReqwestError(err)
	}
}

// TODO: The server either returns { error: String } or the intended response (R).
// The way we handle that is with a generic, flattened enum ApiErrorResponse.
// This is a bit awkward; is there a better solution?
fn api_request<U, T, R>(client: &reqwest::Client, url: U, request: &T) -> Result<R, ApiError>
	where U: IntoUrl,
	      T: serde::ser::Serialize + ?Sized,
	      R: serde::de::DeserializeOwned
{
	let response = client.post(url)
		.json(request)
		.send()?;
	
	let response: ApiErrorResponse<R> = response.error_for_status()?.json()?;

	match response {
		ApiErrorResponse::Error { error: err } => Err(ApiError::ApiError(err)),
		ApiErrorResponse::Ok(v) => Ok(v),
	}
}


// Returns the merged entry and a bool indicating if the new entry should be uploaded to the server.
// TODO: Currently this panics in the case of a conflict.  We'll want to do conflict resolution instead.
fn sync_merge_entry(local_entry: &Entry, server_entry: &Entry) -> (Entry, bool) {
	// Make sure there are no conflicts
	let shared_history_len = cmp::min(local_entry.get_history().len(), server_entry.get_history().len());

	if local_entry.get_history()[..shared_history_len] != server_entry.get_history()[..shared_history_len] {
		panic!("Unable to merge entries; conflict");
	}

	let mut new_entry = local_entry.clone();

	// We use >= so that, in the case where the entries are the same, we force a re-upload.
	if local_entry.get_history().len() >= server_entry.get_history().len() {
		// Server entry is old
		return (new_entry, true);
	}
	else {
		// Server entry is ahead of us
		for history in &server_entry.get_history()[shared_history_len..] {
			new_entry.edit(history.clone());
		}

		return (new_entry, false);
	}
}


// Returns the merged directory, a list of new IDs to download, and a bool indicating if the merged directory should be uploaded to the server.
// TODO: Does not currently support nested directories.
// TODO: Currently panics in the case of a conflict.
fn sync_merge_directory(local_directory: &Directory, server_directory: &Directory, known_objects: &DatabaseObjectMap) -> (Directory, Vec<ID>, bool) {
	let local_directory_history = local_directory.get_history();
	let server_directory_history = server_directory.get_history();

	let shared_history_len = cmp::min(local_directory_history.len(), server_directory_history.len());

	if local_directory_history[..shared_history_len] != server_directory_history[..shared_history_len] {
		panic!("Unable to merge directories; conflict");
	}

	let mut new_directory = local_directory.clone();
	let mut new_ids = Vec::new();

	if local_directory_history.len() > server_directory_history.len() {
		// Server directory is old
		return (new_directory, new_ids, true);
	}
	else {
		// Server directory is ahead of us
		for history in &server_directory_history[shared_history_len..] {
			match history.action {
				DirectoryHistoryAction::Add => {
					new_directory.add_with_time(history.id, history.time);

					match known_objects.get(&history.id) {
						Some(&DatabaseObject::Directory(_)) => {
							panic!("Nested directories are not currently supported.");
						},
						Some(&DatabaseObject::Entry(_)) => {},
						None => {
							new_ids.push(history.id);
						},
					}
				},
				DirectoryHistoryAction::Remove => {
					new_directory.remove_with_time(history.id, history.time);
				},
			}
		}

		return (new_directory, new_ids, false);
	}
}


mod hex_format {
	use data_encoding::HEXLOWER_PERMISSIVE;
	use serde::{self, Deserialize, Serializer, Deserializer};

	pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
		where S: Serializer
	{
		serializer.serialize_str(&HEXLOWER_PERMISSIVE.encode(data))
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
		where D: Deserializer<'de>
	{
		let s = String::deserialize(deserializer)?;
		HEXLOWER_PERMISSIVE.decode(s.as_bytes()).map_err(serde::de::Error::custom)
	}
}


pub fn random_string(length: usize, uppercase: bool, lowercase: bool, numbers: bool, others: &str) -> String {
	let mut rng = OsRng::new().expect("OsRng failed to initialize");
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

	if alphabet.len() == 0 {
		return String::new();
	}

	let alphabet: Vec<char> = alphabet.into_iter().collect();
	let mut result = String::new();

	for _ in 0..length {
		result.push(rng.choose(&alphabet).unwrap().clone());
	}

	result
}


// Returns the current unix timestamp in nanoseconds.
// Our library won't handle time before the unix epoch, so we return u64.
// NOTE: This will panic if used past ~2500 C.E. (Y2K taught me nothing).
fn unix_timestamp() -> u64 {
	let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
	timestamp.as_secs().checked_mul(1000000000).unwrap().checked_add(timestamp.subsec_nanos() as u64).unwrap()
}


#[cfg(test)]
mod tests {
	use super::{Database, DatabaseObject, Directory, random_string, Entry, EntryHistory, ID, serde_json};
	use rand::{OsRng, Rng};
	use std::collections::HashMap;
	use tempfile::tempdir;

	#[test]
	fn encrypt_then_decrypt() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let password_len = rng.gen_range(0, 64);
		let password: String = rng.gen_iter::<char>().take(password_len).collect();
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
		let old_salt = db.encryption_parameters.salt.clone();
		let old_file_key_suite = db.file_key_suite.clone();
		let old_sync_parameters = db.sync_parameters.clone();

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new([
			("title".to_string(), "Password change".to_string()),
			].iter().cloned().collect()));
		db.add_entry(entry);

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Password change should change file encryption keys, even if using the same password
		db.change_password("username", "password");
		assert_ne!(db.encryption_parameters.salt, old_salt);
		assert_ne!(db.file_key_suite, old_file_key_suite);

		// Password change should not change network keys if using the same password
		assert_eq!(db.sync_parameters, old_sync_parameters);

		// Changing username should change network keys even if using the same password
		db.change_password("username2", "password");
		assert_ne!(db.sync_parameters.get_master_key(), old_sync_parameters.get_master_key());
		assert_ne!(db.sync_parameters.get_login_key(), old_sync_parameters.get_login_key());
		assert_ne!(db.sync_parameters.get_login_id(), old_sync_parameters.get_login_id());
		assert_ne!(db.sync_parameters.get_network_key_suite(), old_sync_parameters.get_network_key_suite());

		// Password change should change all keys if username and/or password are different
		db.change_password("username", "password2");
		assert_ne!(db.encryption_parameters.salt, old_salt);
		assert_ne!(db.file_key_suite, old_file_key_suite);
		assert_ne!(db.sync_parameters, old_sync_parameters);

		// Save
		db.save_to_path(tmp_dir.path().join("test2.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "password").unwrap();
		let db3 = Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password2").unwrap();
		Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password").expect_err("Shouldn't be able to load database with old password");

		assert_eq!(db.objects, db2.objects);
		assert_eq!(db.root_directory, db2.root_directory);
		assert_eq!(db.objects, db3.objects);
		assert_eq!(db.root_directory, db3.root_directory);
	}

	// Just some sanity checks on our keys
	#[test]
	fn key_sanity_checks() {
		let db = Database::new_with_password("username", "password");
		let db2 = Database::new_with_password("username", "password");

		assert!(db != db2);
		assert_ne!(db.encryption_parameters, db2.encryption_parameters);
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
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Unicode in username and password
		let username: String = rng.gen_iter::<char>().take(256).collect();
		let password: String = rng.gen_iter::<char>().take(256).collect();
		let mut db = Database::new_with_password(&username, &password);

		// Unicode in entries
		let a = rng.gen_iter::<char>().take(256).collect::<String>();
		let b = rng.gen_iter::<char>().take(256).collect::<String>();
		let c = rng.gen_iter::<char>().take(256).collect::<String>();

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new([
			(rng.gen_iter::<char>().take(256).collect::<String>(), rng.gen_iter::<char>().take(256).collect::<String>()),
			(rng.gen_iter::<char>().take(256).collect::<String>(), rng.gen_iter::<char>().take(256).collect::<String>()),
			(a.clone(), b.clone()),
			].iter().cloned().collect()));
		entry.edit(EntryHistory::new([
			(rng.gen_iter::<char>().take(256).collect::<String>(), rng.gen_iter::<char>().take(256).collect::<String>()),
			(rng.gen_iter::<char>().take(256).collect::<String>(), rng.gen_iter::<char>().take(256).collect::<String>()),
			(rng.gen_iter::<char>().take(256).collect::<String>(), rng.gen_iter::<char>().take(256).collect::<String>()),
			(a.clone(), c.clone()),
			].iter().cloned().collect()));
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
		entry.edit(EntryHistory::new([
			("title".to_string(), "Test test".to_string()),
			("username".to_string(), "Username".to_string())
			].iter().cloned().collect()));
		db.add_entry(entry);

		let mut entry = Entry::new();
		let tmp_entry_id = entry.get_id().clone();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new([
			("title".to_string(), "Test test".to_string()),
			("username".to_string(), "Username".to_string()),
			].iter().cloned().collect()));
		entry.edit(EntryHistory::new([
			("username".to_string(), "Username".to_string()),
			("title".to_string(), "Ooops".to_string()),
			("password".to_string(), "Password".to_string()),
			].iter().cloned().collect()));
		db.add_entry(entry);

		db.get_root_mut().remove(tmp_entry_id.clone());
		db.get_root_mut().add(tmp_entry_id.clone());

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let mut db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar").unwrap();
		assert_eq!(db, db2);

		// Edit
		let entry_id: ID = **db2.get_root().list_entries(&db2).iter().find(|id| {
			let entry = db2.get_entry_by_id(id).unwrap();

			entry.get("title") == None
		}).unwrap();

		{
			let entry = db2.get_entry_by_id_mut(&entry_id).unwrap();
			entry.edit(EntryHistory::new([
				("title".to_string(), "Forgot this one".to_string()),
				].iter().cloned().collect()));
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
					assert_eq!(entry.get_history().len(), 2);
					assert_eq!(entry.get_history()[0].data.len(), 0);
				},
				Some("Test test") => {
					assert_eq!(entry.get("username").unwrap(), "Username");
					assert_eq!(entry.get_history().len(), 2);
				},
				Some("Ooops") => {
					assert_eq!(entry["username"], "Username");
					assert_eq!(entry.get_state()["password"], "Password");
					assert_eq!(entry.get_history()[0].data.len(), 0);
					assert_eq!(entry.get_history()[1].data["username"], "Username");
					assert_eq!(entry.get_history()[2].data.get("username"), None);
					assert_eq!(entry.get_history()[1]["title"], "Test test");
				}
				_ => {
					panic!("Unknown title");
				}
			}
		}
	}

	// Test to make sure serialization is fully deterministic (the same database object serializes to the same string every time)
	#[test]
	fn entry_deterministic_serialization() {
		// Create entry
		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new([
			("title".to_string(), "Serialization".to_string()),
			("username".to_string(), "Foo".to_string()),
			("password".to_string(), "password".to_string()),
			("url".to_string(), "Url".to_string()),
			].iter().cloned().collect()));
		entry.edit(EntryHistory::new([
			("title".to_string(), "Title change".to_string()),
			("url".to_string(), "Url change".to_string()),
			].iter().cloned().collect()));

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
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Create directory
		let mut directory = Directory::new();

		let id1: ID = rng.gen();
		let id2: ID = rng.gen();
		let id3: ID = rng.gen();

		directory.add(id1.clone());
		directory.add(id2.clone());
		directory.add(id3.clone());
		directory.remove(id2.clone());
		directory.remove(id3.clone());
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

	// TODO: Test all the failure modes of opening a database
	// TODO: e.g. make sure corrupting the database file results in a checksum failure, make sure a bad mac results in a MAC failure, etc.
	// TODO: Add a test that contains a pre-serialized database and which deserializes it to ensure that we don't accidentally change the serialization formats.
}