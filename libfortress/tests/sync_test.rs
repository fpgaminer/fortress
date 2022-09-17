extern crate libfortress;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate data_encoding;
extern crate tiny_http;

mod sync_server;

use libfortress::{Database, Entry, EntryHistory};
use std::collections::HashMap;


// TODO: Test bootstrap
#[test]
fn sync_integration_test() {
	// Start testing server
	let sync_url = sync_server::server();

	// Build database
	let mut db = Database::new_with_password("username", "foobar");
	db.do_not_set_testing = true;

	let mut entry1 = Entry::new();
	entry1.edit(EntryHistory::new(HashMap::new()));
	db.add_entry(entry1.clone());

	let mut entry2 = Entry::new();
	entry2.edit(EntryHistory::new(HashMap::new()));
	entry2.edit(EntryHistory::new(
		[("title".to_string(), "Test test".to_string()), ("username".to_string(), "Username".to_string())]
			.iter()
			.cloned()
			.collect(),
	));
	db.add_entry(entry2.clone());

	let mut entry3 = Entry::new();
	entry3.edit(EntryHistory::new(HashMap::new()));
	entry3.edit(EntryHistory::new(
		[("title".to_string(), "Test test".to_string()), ("username".to_string(), "Username".to_string())]
			.iter()
			.cloned()
			.collect(),
	));
	entry3.edit(EntryHistory::new(
		[
			("username".to_string(), "Username".to_string()),
			("title".to_string(), "Ooops".to_string()),
			("password".to_string(), "Password".to_string()),
		]
		.iter()
		.cloned()
		.collect(),
	));
	db.add_entry(entry3.clone());

	db.get_root_mut().remove(entry3.get_id().clone());
	db.get_root_mut().add(entry3.get_id().clone());

	// Clone
	let mut db_old = db.clone();
	let mut parallel_db = db.clone();

	// Modify db
	{
		let entry = db.get_entry_by_id_mut(entry3.get_id()).unwrap();
		entry.edit(EntryHistory::new([("password".to_string(), "Password2".to_string())].iter().cloned().collect()));
	}

	let mut entry4 = Entry::new();
	entry4.edit(EntryHistory::new(HashMap::new()));
	entry4.edit(EntryHistory::new(
		[
			("title".to_string(), "Unsynced".to_string()),
			("username".to_string(), "Not synced yet".to_string()),
		]
		.iter()
		.cloned()
		.collect(),
	));
	db.add_entry(entry4.clone());

	// Modify parallel_db
	parallel_db.get_entry_by_id_mut(entry1.get_id()).unwrap().edit(EntryHistory::new(
		[
			("title".to_string(), "Parallel Edit".to_string()),
			("username".to_string(), "Editted in parallel".to_string()),
		]
		.iter()
		.cloned()
		.collect(),
	));

	let mut entry5 = Entry::new();
	entry5.edit(EntryHistory::new(HashMap::new()));
	entry5.edit(EntryHistory::new(
		[
			("title".to_string(), "Parallel Add".to_string()),
			("username".to_string(), "Entry from another mother".to_string()),
		]
		.iter()
		.cloned()
		.collect(),
	));
	parallel_db.add_entry(entry5.clone());

	// Sync db
	let db_before_sync = db.clone();

	db.sync(&sync_url);

	// Syncing right now shouldn't change anything
	db.sync(&sync_url);
	assert_eq!(db, db_before_sync);

	// Syncing the older database should bring it up to speed
	db_old.sync(&sync_url);
	assert_eq!(db_old, db);

	// But still shouldn't affect db
	db.sync(&sync_url);
	assert_eq!(db, db_before_sync);

	// Syncing parallel_db should pick up db's edits
	parallel_db.sync(&sync_url);
	assert_eq!(
		parallel_db.get_entry_by_id(entry3.get_id()).unwrap(),
		db.get_entry_by_id(entry3.get_id()).unwrap()
	);
	assert_ne!(parallel_db, db);

	// Now syncing db should pick up parallel db's edits
	db.sync(&sync_url);
	assert_eq!(parallel_db, db);
	assert_ne!(db, db_old);

	// Everything should be synced now
	parallel_db.sync(&sync_url);
	assert_eq!(parallel_db, db);
}
