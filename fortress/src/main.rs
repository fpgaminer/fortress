mod create_database;
mod dialog_modal;
mod entry_editor;
mod generate;
mod menu;
mod open_database;
mod view_database;

use clap::{Parser, Subcommand};
use create_database::CreateDatabaseModel;
use dialog_modal::{DialogConfig, DialogModel, DialogMsg};
use entry_editor::{EntryEditorModel, EntryEditorMsg};
use generate::GenerateModel;
use gtk::prelude::GtkWindowExt;
use libfortress::{Database, EntryHistory, ID};
use menu::MenuModel;
use open_database::OpenDatabaseModel;
use relm4::{
	gtk::{self, prelude::Cast, traits::WidgetExt},
	send, AppUpdate, Model, RelmApp, RelmComponent, Sender, WidgetPlus, Widgets,
};
use relm4_components::ParentWindow;
use serde::{Deserialize, Serialize};
use std::{
	cell::{RefCell, RefMut},
	fs::{self, File},
	io::{self, BufReader, Read, Write},
	path::{Path, PathBuf},
	rc::Rc,
};
use view_database::ViewDatabaseModel;


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

	let config_path = data_dir.join("config.json");
	let database_path = data_dir.join("database.fortress");

	let config = if config_path.exists() {
		let reader = File::open(&config_path).expect("Failed to open config file");
		serde_json::from_reader(reader).expect("Failed to parse config file")
	} else {
		let config = Config { sync_url: "".to_owned() };

		config.save_to_path(&config_path).expect("Failed to save config file");

		config
	};
	let config = Rc::new(RefCell::new(config));

	// This needs to be called before building AppModel, because we need to call things like EntryBuffer::new().
	gtk::init().expect("Failed to initialize GTK");

	let database = Rc::new(RefCell::new(None));

	let model = AppModel {
		state: if database_path.exists() {
			AppState::OpenDatabase
		} else {
			AppState::CreateDatabase
		},
		database,
		config,
		config_path,
		database_path,
	};
	let app = RelmApp::new(model);
	app.run();
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


struct AppModel {
	state: AppState,
	database: Rc<RefCell<Option<Database>>>,
	config: Rc<RefCell<Config>>,
	config_path: PathBuf,
	database_path: PathBuf,
}

#[derive(Clone)]
enum AppMsg {
	ShowError(String),

	// From ViewDatabase
	NewEntry,
	EditEntry(ID),
	ShowMenu,

	// From EntryEditor
	EntryEditorSaved { id: Option<ID>, data: EntryHistory },
	EntryEditorClosed,
	GeneratePassword,

	// From CreateDatabase
	DatabaseCreated(Database),

	// From OpenDatabase
	DatabaseOpened(Database),

	// From Generate
	PasswordGenerated(Option<String>),

	// From Dialog
	CloseDialog,

	// From Menu
	CloseMenu,
}

#[derive(Debug, PartialEq)]
enum AppState {
	OpenDatabase,
	CreateDatabase,
	ViewDatabase,
	EditEntry,
	Menu,
	GeneratePassword,
}

impl Model for AppModel {
	type Msg = AppMsg;
	type Widgets = AppWidgets;
	type Components = AppComponents;
}

impl AppUpdate for AppModel {
	fn update(&mut self, msg: AppMsg, components: &AppComponents, sender: Sender<AppMsg>) -> bool {
		match msg {
			AppMsg::ShowError(err) => {
				components
					.dialog
					.send(DialogMsg::Show(DialogConfig {
						title: "Error".to_string(),
						text: err,
						buttons: vec![("Okay".to_owned(), AppMsg::CloseDialog)],
					}))
					.unwrap();
			},
			AppMsg::DatabaseCreated(database) | AppMsg::DatabaseOpened(database) => {
				*self.database.borrow_mut() = Some(database);
				self.state = AppState::ViewDatabase;
			},
			AppMsg::NewEntry => {
				self.state = AppState::EditEntry;
				components.entry_editor.send(EntryEditorMsg::NewEntry).unwrap();
			},
			AppMsg::EditEntry(id) => {
				let entry = self.database.borrow().as_ref().unwrap().get_entry_by_id(&id).unwrap().clone();
				self.state = AppState::EditEntry;
				components.entry_editor.send(EntryEditorMsg::EditEntry(entry)).unwrap();
			},
			AppMsg::EntryEditorSaved { id, data } => {
				let mut database = RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()).expect("Database not open");

				if let Some(id) = id {
					// Edit entry
					let entry = database.get_entry_by_id_mut(&id).expect("internal error");
					entry.edit(data);
				} else {
					// New entry
					let mut entry = libfortress::Entry::new();
					entry.edit(data);
					database.add_entry(entry);
				}

				if let Err(err) = database.save_to_path(&self.database_path) {
					// TODO: This is a fatal error.  We should use a different dialog that allows the user to try and save again, or quit the application.
					send!(sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
					return true;
				}

				self.state = AppState::ViewDatabase;
			},
			AppMsg::EntryEditorClosed => {
				self.state = AppState::ViewDatabase;
			},
			AppMsg::PasswordGenerated(password) => {
				if let Some(password) = password {
					components.entry_editor.send(EntryEditorMsg::PasswordGenerated(password)).unwrap();
				}
				self.state = AppState::EditEntry;
			},
			AppMsg::GeneratePassword => {
				self.state = AppState::GeneratePassword;
			},
			AppMsg::CloseDialog => {
				components.dialog.send(DialogMsg::Hide).unwrap();
			},
			AppMsg::CloseMenu => {
				self.state = AppState::ViewDatabase;
			},
			AppMsg::ShowMenu => {
				self.state = AppState::Menu;
			},
		}
		true
	}
}


#[relm4::widget]
impl Widgets<AppModel, ()> for AppWidgets {
	additional_fields! {
		stack_child_create: gtk::Box,
		stack_child_open: gtk::Box,
		stack_child_entry: gtk::Box,
		stack_child_database: gtk::Box,
		stack_child_generate: gtk::Box,
		stack_child_menu: gtk::Box,
	}

	view! {
		main_window = gtk::ApplicationWindow {
			set_title: Some("Fortress"),
			set_default_width: 800,
			set_default_height: 600,
			set_child = stack = Some(&gtk::Stack) {
				set_transition_type: gtk::StackTransitionType::SlideLeftRight,
				set_margin_all: 5,
				set_margin_start: 40,
				set_margin_end: 40,
				set_margin_top: 40,
				set_margin_bottom: 40,

				add_child: components.create_database.root_widget(),
				add_child: components.open_database.root_widget(),
				add_child: components.view_database.root_widget(),
				add_child: components.entry_editor.root_widget(),
				add_child: components.generate.root_widget(),
				add_child: components.menu.root_widget(),
			},
		}
	}

	fn post_init() {
		// TODO: Is there a better way to do this? I couldn't get this to work with the view! macro.
		let stack_child_create = components.create_database.root_widget().clone();
		let stack_child_open = components.open_database.root_widget().clone();
		let stack_child_database = components.view_database.root_widget().clone();
		let stack_child_entry = components.entry_editor.root_widget().clone();
		let stack_child_generate = components.generate.root_widget().clone();
		let stack_child_menu = components.menu.root_widget().clone();

		AppWidgets::update_stack(
			&stack,
			&model.state,
			&stack_child_create,
			&stack_child_open,
			&stack_child_database,
			&stack_child_entry,
			&stack_child_generate,
			&stack_child_menu,
		);
	}

	fn post_view() {
		AppWidgets::update_stack(
			stack,
			&model.state,
			stack_child_create,
			stack_child_open,
			stack_child_database,
			stack_child_entry,
			stack_child_generate,
			stack_child_menu,
		);
	}
}

impl AppWidgets {
	#[allow(clippy::too_many_arguments)]
	fn update_stack(
		stack: &gtk::Stack,
		state: &AppState,
		stack_child_create: &gtk::Box,
		stack_child_open: &gtk::Box,
		stack_child_database: &gtk::Box,
		stack_child_entry: &gtk::Box,
		stack_child_generate: &gtk::Box,
		stack_child_menu: &gtk::Box,
	) {
		stack.set_visible_child(match state {
			AppState::CreateDatabase => stack_child_create,
			AppState::OpenDatabase => stack_child_open,
			AppState::ViewDatabase => stack_child_database,
			AppState::EditEntry => stack_child_entry,
			AppState::GeneratePassword => stack_child_generate,
			AppState::Menu => stack_child_menu,
		});
	}
}

impl ParentWindow for AppWidgets {
	fn parent_window(&self) -> Option<gtk::Window> {
		Some(self.main_window.clone().upcast())
	}
}

#[derive(relm4::Components)]
struct AppComponents {
	dialog: RelmComponent<DialogModel<AppModel>, AppModel>,
	create_database: RelmComponent<CreateDatabaseModel, AppModel>,
	open_database: RelmComponent<OpenDatabaseModel, AppModel>,
	view_database: RelmComponent<ViewDatabaseModel, AppModel>,
	entry_editor: RelmComponent<EntryEditorModel, AppModel>,
	generate: RelmComponent<GenerateModel, AppModel>,
	menu: RelmComponent<MenuModel, AppModel>,
}


#[derive(Serialize, Deserialize)]
struct Config {
	sync_url: String,
}

impl Config {
	fn save_to_path<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
		// Writes to a temp file and then atomically swaps it over; for fault tolerance.
		let temp_path = path.as_ref().with_extension("tmp");
		{
			let writer = File::create(&temp_path)?;
			serde_json::to_writer(writer, &self)?;
		}

		fs::rename(&temp_path, path)
	}
}
