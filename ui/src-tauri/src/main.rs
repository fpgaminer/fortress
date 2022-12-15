#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use std::{
	collections::HashMap,
	fs::{self, File},
	io::{self, BufReader, Read, Write},
	path::{Path, PathBuf},
	sync::Mutex,
};

use clap::{Parser, Subcommand};
use libfortress::{fortresscrypto::CryptoError, Database, Directory, Entry, EntryHistory, FortressError, ID};
use url::Url;


#[derive(Parser, Debug)]
#[clap(version, about, long_about = None, subcommand_negates_reqs = true)]
struct Args {
	#[command(subcommand)]
	command: Option<Commands>,

	// In debug mode we won't auto-fill dir with the user's data dir.  Instead this argument is required.
	// This is so devs don't accidentally mess up their personal Fortress data during development.
	#[cfg(debug_assertions)]
	#[clap(long, value_parser, required = true)]
	dir: Option<PathBuf>,

	#[cfg(not(debug_assertions))]
	#[clap(long, value_parser)]
	dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
	/// Just encrypt the specified payload, writing to stdout
	Encrypt { path: PathBuf },

	/// Just decrypt the specified payload, writing to stdout
	Decrypt { path: PathBuf },
}


fn main() {
	let args = Args::parse();

	// Handle encrypt/decrypt commands
	match &args.command {
		Some(Commands::Encrypt { path }) => {
			let password = read_password();

			do_encrypt(path, &password);
			return;
		},
		Some(Commands::Decrypt { path }) => {
			let password = read_password();

			do_decrypt(path, &password);
			return;
		},
		None => {},
	}

	// Handle normal operation
	#[cfg(not(debug_assertions))]
	let data_dir = args.dir.unwrap_or_else(|| {
		directories::ProjectDirs::from("", "", "Fortress")
			.expect("Unable to find data dir")
			.data_dir()
			.to_owned()
	});
	#[cfg(debug_assertions)]
	let data_dir = args.dir.expect("Data dir is required in debug mode");

	fs::create_dir_all(&data_dir).expect("Failed to create data directory");

	if !data_dir.is_dir() {
		eprintln!("'{:?}' is not a directory.", data_dir);
		return;
	}

	let database_path = data_dir.join("database.fortress");
	let appstate = AppState {
		database_path,
		database: Mutex::new(None),
	};

	tauri::Builder::default()
		.manage(appstate)
		.invoke_handler(tauri::generate_handler![
			database_exists,
			create_database,
			unlock_database,
			list_entries,
			list_directories,
			error_dialog,
			move_object,
			rename_directory,
			new_directory,
			random_string,
			edit_entry,
			get_username,
			get_sync_keys,
			get_sync_url,
			set_sync_url,
			change_password,
			sync_database
		])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}


struct AppState {
	database_path: PathBuf,
	database: Mutex<Option<Database>>,
}


fn format_fortress_error(err: FortressError) -> String {
	match err {
		FortressError::CryptoError(CryptoError::DecryptionError) => "Incorrect password.".to_owned(),
		FortressError::CryptoError(CryptoError::BadChecksum) => "File is corrupted.".to_owned(),
		err => format!("{}", err),
	}
}


#[tauri::command]
fn error_dialog(message: String, window: tauri::Window) {
	tauri::api::dialog::message(Some(&window), "Error", message)
}


#[tauri::command]
fn random_string(length: usize, uppercase: bool, lowercase: bool, numbers: bool, others: String) -> String {
	libfortress::random_string(length, uppercase, lowercase, numbers, &others)
}


#[tauri::command]
fn database_exists(state: tauri::State<AppState>) -> bool {
	state.database_path.exists()
}


#[tauri::command]
fn create_database(username: String, password: String, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = Database::new_with_password(username, password);

	database.get_root_mut().rename("My Passwords");

	database.save_to_path(&state.database_path).map_err(format_fortress_error)?;

	*state.database.lock().unwrap() = Some(database);

	Ok(())
}


#[tauri::command]
fn unlock_database(password: String, state: tauri::State<AppState>) -> Result<(), String> {
	match Database::load_from_path(&state.database_path, password) {
		Ok(database) => {
			*state.database.lock().unwrap() = Some(database);
			Ok(())
		},
		Err(err) => Err(format_fortress_error(err)),
	}
}


#[tauri::command]
fn list_entries(state: tauri::State<AppState>) -> Result<Vec<Entry>, ()> {
	let database = state.database.lock().unwrap();

	database.as_ref().ok_or(()).map(|d| d.list_entries().cloned().collect())
}


#[tauri::command]
fn list_directories(state: tauri::State<AppState>) -> Result<Vec<Directory>, ()> {
	let database = state.database.lock().unwrap();

	database.as_ref().ok_or(()).map(|d| d.list_directories().cloned().collect())
}


#[tauri::command]
fn move_object(object_id: ID, new_parent_id: ID, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		database.move_object(&object_id, &new_parent_id);

		// Save the database
		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn rename_directory(directory_id: ID, new_name: String, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		let directory = database.get_directory_by_id_mut(&directory_id).ok_or("Directory not found.")?;
		directory.rename(new_name);

		// Save the database
		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn new_directory(name: String, state: tauri::State<AppState>) -> Result<ID, String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		let mut directory = Directory::new();
		let id = *directory.get_id();
		directory.rename(name);
		database.add_directory(directory);

		// Save the database
		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(id)
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn edit_entry(entry_id: Option<ID>, data: HashMap<String, String>, parent_id: ID, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	let data = EntryHistory::new(data);

	if let Some(database) = database.as_mut() {
		if let Some(id) = entry_id {
			// Edit entry
			let entry = database.get_entry_by_id_mut(&id).ok_or("Entry not found.")?;
			entry.edit(data);
		} else {
			// New entry
			let mut entry = libfortress::Entry::new();
			let entry_id = *entry.get_id();
			entry.edit(data);
			database.add_entry(entry);
			database.move_object(&entry_id, &parent_id);
		}

		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn get_username(state: tauri::State<AppState>) -> Result<String, ()> {
	let database = state.database.lock().unwrap();

	database.as_ref().ok_or(()).map(|d| d.get_username().to_owned())
}


#[tauri::command]
fn get_sync_keys(state: tauri::State<AppState>) -> Result<String, ()> {
	let database = state.database.lock().unwrap();

	database
		.as_ref()
		.ok_or(())
		.map(|d| format!("{}:{}", d.get_login_id().to_hex(), d.get_login_key().to_hex()))
}


#[tauri::command]
fn get_sync_url(state: tauri::State<AppState>) -> Result<Option<Url>, ()> {
	let database = state.database.lock().unwrap();

	database.as_ref().ok_or(()).map(|d| d.get_sync_url().cloned())
}


#[tauri::command]
fn set_sync_url(url: String, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		database.set_sync_url(Some(url.parse().map_err(|_| "Invalid URL.")?));

		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn change_password(username: String, password: String, state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		database.change_password(&username, &password);

		if let Err(err) = database.save_to_path(&state.database_path) {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


#[tauri::command]
fn sync_database(state: tauri::State<AppState>) -> Result<(), String> {
	let mut database = state.database.lock().unwrap();

	if let Some(database) = database.as_mut() {
		if let Err(err) = database.sync() {
			Err(format_fortress_error(err))
		} else {
			Ok(())
		}
	} else {
		Err("Database is not unlocked.".to_owned())
	}
}


fn read_password() -> String {
	// NOTE: We could use something like the rpassword crate to read this without showing the password
	// on screen, but that adds another dependency and the decrypt/encrypt commands are generally only
	// used during development or exotic scenarios.
	let mut password = String::new();
	eprint!("Password: ");
	io::stderr().flush().unwrap();
	io::stdin().read_line(&mut password).expect("Failed to read password from stdin");
	password.trim_end().to_owned()
}


/// Read file and decrypt
fn do_decrypt<P: AsRef<Path>>(path: P, password: &str) {
	let (payload, _) = {
		let file = File::open(path).expect("Failed to open file");
		let mut reader = BufReader::new(file);

		libfortress::fortresscrypto::decrypt_from_file(&mut reader, password.as_bytes()).expect("Failed to decrypt file")
	};

	io::stdout().write_all(&payload).expect("Failed to write to stdout");
}


/// Read file and encrypt
fn do_encrypt<P: AsRef<Path>>(path: P, password: &str) {
	let payload = {
		let mut data = Vec::new();
		File::open(path)
			.expect("Failed to open file")
			.read_to_end(&mut data)
			.expect("Failed to read file");
		data
	};

	let encryption_parameters = Default::default();
	let file_key_suite =
		libfortress::fortresscrypto::FileKeySuite::derive(password.as_bytes(), &encryption_parameters).expect("Failed to derive file key suite");

	libfortress::fortresscrypto::encrypt_to_file(&mut io::stdout(), &payload, &file_key_suite).expect("Failed to encrypt file");
}
