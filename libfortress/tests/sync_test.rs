mod sync_server;

use libfortress::{Database, Entry, EntryHistory, FortressError};
use reqwest::Url;
use std::collections::HashMap;


#[test]
fn sync_integration_test() {
	// Build database
	let mut db = Database::new_with_password("username", "foobar");

	// Start testing server
	let sync_url = Url::parse(&sync_server::server(db.get_login_key().clone())).unwrap();
	db.set_sync_url(Some(sync_url.clone()));

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

	db.sync().unwrap();

	// Syncing right now shouldn't change anything
	db.sync().unwrap();
	assert_eq!(db, db_before_sync);

	// Syncing the older database should bring it up to speed
	db_old.sync().unwrap();
	assert_eq!(db_old, db);

	// But still shouldn't affect db
	db.sync().unwrap();
	assert_eq!(db, db_before_sync);

	// Syncing parallel_db should pick up db's edits
	parallel_db.sync().unwrap();
	assert_eq!(
		parallel_db.get_entry_by_id(entry3.get_id()).unwrap(),
		db.get_entry_by_id(entry3.get_id()).unwrap()
	);
	assert_ne!(parallel_db, db);

	// Now syncing db should pick up parallel db's edits
	db.sync().unwrap();
	assert_eq!(parallel_db, db);
	assert_ne!(db, db_old);

	// Everything should be synced now
	parallel_db.sync().unwrap();
	assert_eq!(parallel_db, db);

	// Now test bootstrapping from nothing but username and password
	let mut bootstrap_db = Database::new_with_password("username", "foobar");
	bootstrap_db.set_sync_url(Some(sync_url.clone()));

	bootstrap_db.sync().unwrap();
	// We compare the serialized forms, because things like the FileKeySuite won't be equal
	assert_eq!(serde_json::to_string(&bootstrap_db).unwrap(), serde_json::to_string(&db).unwrap());

	// Now test password change
	let mut old_db = db.clone();

	db.get_root_mut().rename("New Root Name".to_string());

	// Change password on db
	db.change_password("username", "barfoo");

	// Sync so the server has the new password
	db.sync().unwrap();

	// Sync again to ensure the new password still works
	db.sync().unwrap();

	// Syncing the old database should fail with 401
	match old_db.sync() {
		Err(FortressError::SyncApiError(libfortress::ApiError::ApiError(401, _))) => (),
		_ => panic!("Syncing with old password should fail"),
	}

	// Now change the password on the old database
	old_db.change_password("username", "barfoo");

	// Syncing the old database should now work
	old_db.sync().unwrap();

	// And the databases should be equal (except for the FileKeySuite)
	assert_eq!(serde_json::to_string(&db).unwrap(), serde_json::to_string(&old_db).unwrap());
}
