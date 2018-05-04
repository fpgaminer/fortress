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
extern crate rand;
extern crate time;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate data_encoding;
extern crate crypto;
extern crate byteorder;
extern crate tempdir;
pub extern crate fortresscrypto;

#[macro_use] mod newtype_macros;
mod database_object;
mod database_object_map;

pub use database_object::{Directory, Entry, EntryHistory};

use rand::{OsRng, Rng};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use std::str;
use fortresscrypto::{MasterKey, EncryptionParameters, FileKeySuite};
use database_object::DatabaseObject;
use database_object_map::DatabaseObjectMap;


new_type!{
	public ID(32);
}


#[derive(Serialize, Eq, PartialEq, Debug)]
pub struct Database {
	objects: DatabaseObjectMap,
	root_directory: ID,

	master_key: Option<MasterKey>,

	#[serde(skip_serializing, skip_deserializing)]
	encryption_parameters: EncryptionParameters,
	#[serde(skip_serializing, skip_deserializing)]
	file_key_suite: FileKeySuite,
}

impl Database {
	pub fn new_with_password(password: &[u8]) -> Database {
		let encryption_parameters = Default::default();
		let file_key_suite = FileKeySuite::derive(password, &encryption_parameters);
		let master_key = None;  //TODO: MasterKey::derive(username, password);

		let root = Directory::new();
		let root_directory = root.get_id().clone();
		let mut objects = DatabaseObjectMap::new();
		objects.update(DatabaseObject::Directory(root));

		Database {
			objects: objects,
			root_directory: root_directory,
			master_key: master_key,
			encryption_parameters: encryption_parameters,
			file_key_suite: file_key_suite,
		}
	}

	pub fn change_password(&mut self, password: &[u8]) {
		self.encryption_parameters = Default::default();
		self.file_key_suite = FileKeySuite::derive(password, &self.encryption_parameters);
		self.master_key = None;  // TODO: Derive
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
		// Serialized payload
		let payload = serde_json::to_vec(&self)?;

		// Encrypt and write to file
		let file = File::create(path)?;
		let mut writer = BufWriter::new(file);
		
		fortresscrypto::encrypt_to_file(&mut writer, &payload, &self.encryption_parameters, &self.file_key_suite)
	}

	pub fn load_from_path<P: AsRef<Path>>(path: P, password: &[u8]) -> io::Result<Database> {
		// This struct is needed because Database has fields that aren't part of
		// serialization, but can't implement Default.
		#[derive(Deserialize)]
		struct SerializableDatabase {
			objects: DatabaseObjectMap,
			root_directory: ID,
			master_key: Option<MasterKey>,
		}

		// Read file and decrypt
		let (plaintext, encryption_parameters, file_key_suite) = {
			let file = File::open(path)?;
			let mut reader = BufReader::new(file);

			fortresscrypto::decrypt_from_file(&mut reader, password)?
		};
		
		// Deserialize
		let db: SerializableDatabase = serde_json::from_slice(&plaintext)?;

		// Keep encryption keys for quicker saving later
		Ok(Database {
			objects: db.objects,
			root_directory: db.root_directory,
			master_key: db.master_key,

			encryption_parameters: encryption_parameters,
			file_key_suite: file_key_suite,
		})
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


#[cfg(test)]
mod tests {
	use super::{Database, DatabaseObject, Directory, random_string, Entry, EntryHistory, ID, serde_json};
	use rand::{OsRng, Rng};
	use std::collections::HashMap;
	use tempdir::TempDir;

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
		let tmp_dir = TempDir::new("test").unwrap();

		// Create DB
		let mut db = Database::new_with_password("password".as_bytes());
		let old_salt = db.encryption_parameters.salt.clone();
		let old_file_key_suite = db.file_key_suite.clone();

		let mut entry = Entry::new();
		entry.edit(EntryHistory::new(HashMap::new()));
		entry.edit(EntryHistory::new([
			("title".to_string(), "Password change".to_string()),
			].iter().cloned().collect()));
		db.add_entry(entry);

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Password change should change file encryption keys, even if using the same password
		db.change_password("password".as_bytes());
		assert_ne!(db.encryption_parameters.salt, old_salt);
		assert_ne!(db.file_key_suite, old_file_key_suite);

		db.change_password("password2".as_bytes());
		assert_ne!(db.encryption_parameters.salt, old_salt);
		assert_ne!(db.file_key_suite, old_file_key_suite);

		// Save
		db.save_to_path(tmp_dir.path().join("test2.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "password".as_bytes()).unwrap();
		let db3 = Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password2".as_bytes()).unwrap();
		Database::load_from_path(tmp_dir.path().join("test2.fortressdb"), "password".as_bytes()).expect_err("Shouldn't be able to load database with old password");

		assert_eq!(db.objects, db2.objects);
		assert_eq!(db.root_directory, db2.root_directory);
		assert_eq!(db.objects, db3.objects);
		assert_eq!(db.root_directory, db3.root_directory);
	}

	// Just some sanity checks on our keys
	#[test]
	fn key_sanity_checks() {
		let db = Database::new_with_password("password".as_bytes());
		let db2 = Database::new_with_password("password".as_bytes());

		assert!(db != db2);
		assert_ne!(db.encryption_parameters, db2.encryption_parameters);
		assert_ne!(db.file_key_suite, db2.file_key_suite);
		assert_eq!(db.master_key, db2.master_key);
		// TODO: assert_eq!(db.network_key_suite, db2.network_key_suite);
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
		let tmp_dir = TempDir::new("test").unwrap();
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Unicode in password
		let password = rng.gen_iter::<char>().take(256).collect::<String>();
		let mut db = Database::new_with_password(password.as_bytes());

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
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), password.as_bytes()).unwrap();
		assert_eq!(db, db2);

		let entry_id = db2.get_root().list_entries(&db2)[0];
		let entry = db2.get_entry_by_id(entry_id).unwrap();

		assert_eq!(entry[&a], c);
	}

	#[test]
	fn test_empty() {
		let tmp_dir = TempDir::new("test").unwrap();

		// Test empty password
		let mut db = Database::new_with_password(&[]);

		// Test empty entry
		db.add_entry(Entry::new());

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), &[]).unwrap();
		assert_eq!(db, db2);

		let entry_id = db2.get_root().list_entries(&db2)[0];
		let entry = db2.get_entry_by_id(entry_id).unwrap();

		assert_eq!(entry.get_state().len(), 0);

		// Test empty database
		let db = Database::new_with_password("foobar".as_bytes());

		// Save
		db.save_to_path(tmp_dir.path().join("test.fortressdb")).unwrap();

		// Load
		let db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar".as_bytes()).unwrap();
		assert_eq!(db, db2);

		assert_eq!(db2.objects.len(), 1);
	}

	// Integration test on the whole Database
	// Simulate typical usage of Database, exercising as many features as possible, and make sure Database operates correctly.
	#[test]
	fn test_database() {
		let tmp_dir = TempDir::new("test").unwrap();

		// Build database
		let mut db = Database::new_with_password("foobar".as_bytes());

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
		let mut db2 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar".as_bytes()).unwrap();
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
		let db3 = Database::load_from_path(tmp_dir.path().join("test.fortressdb"), "foobar".as_bytes()).unwrap();
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
}