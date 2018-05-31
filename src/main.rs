extern crate libfortress;
extern crate gtk;
extern crate data_encoding;
#[macro_use]
extern crate clap;

use data_encoding::HEXLOWER_PERMISSIVE;
use gtk::{CellRendererText, ListStore, TreeView, TreeViewColumn};
use gtk::prelude::*;
use libfortress::{Database, ID};
use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::io::{self, Write, Read, BufReader};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender, Receiver};


macro_rules! connect {
	($master:ident, $widget:expr, $event:ident, $callback:ident) => (
		{
			let tx = $master.tx.clone();
			let rx = $master.rx.clone();
			let app = $master.app.clone();

			$widget.$event(move |_| {
				tx.send(Self::$callback).unwrap();

				if let Ok(rx) = rx.try_borrow_mut() {
					for msg in rx.try_iter() {
						msg(&mut *app.borrow_mut());
					}
				}
			});
		}
	);
}


macro_rules! builder_ui {
	($ui:ident; $( $name:ident : $t:ty ),* ) => {
		struct $ui {
			$(
				$name: $t,
			)*
		}

		impl $ui {
			fn from_builder(builder: &gtk::Builder) -> Self {
				Self {
					$(
						$name: builder.get_object(stringify!($name)).expect(
							concat!("Glade missing ", stringify!($name))
						),
					)*
				}
			}
		}
	};
}


fn main() {
	let matches = clap::App::new("fortress")
		.version(crate_version!())
		.about("Password Manager")
		.setting(clap::AppSettings::ColoredHelp)
		.setting(clap::AppSettings::UnifiedHelpMessage)
		.args_from_usage(
			"--encrypt               'Just encrypt the specified payload, writing to stdout'
		     --decrypt               'Just decrypt the specified database, writing to stdout'
		     --password=[PASSWORD]   'Password to use for --decrypt'
		     [DATABASE]              'Fortress file to open'"
		)
		.group(clap::ArgGroup::with_name("crypt").args(&["encrypt", "decrypt"]).requires_all(&["password", "DATABASE"]))
		.get_matches();

	if matches.is_present("decrypt") {
		let password = matches.value_of("password").unwrap();
		let path = matches.value_of("DATABASE").unwrap();

		do_decrypt(path, password);
		return;
	}

	if matches.is_present("encrypt") {
		let password = matches.value_of("password").unwrap();
		let path = matches.value_of("DATABASE").unwrap();

		do_encrypt(path, password);
		return;
	}

	// Initialize GTK
	if gtk::init().is_err() {
		println!("Failed to initialize GTK.");
		return;
	}

	let app = App::new();
	let (tx, rx) = channel::<fn(&mut App)>();
	let event_master = EventMaster {
		app: Rc::new(RefCell::new(app)),
		rx: Rc::new(RefCell::new(rx)),
		tx: tx,
	};

	event_master.app.borrow().connect_events(&event_master);

	gtk::main();
}


fn do_decrypt(path: &str, password: &str) {
	// Read file and decrypt
	let (payload, _, _) = {
		let file = File::open(path).unwrap();
		let mut reader = BufReader::new(file);

		libfortress::fortresscrypto::decrypt_from_file(&mut reader, password.as_bytes()).unwrap()
	};

	io::stdout().write(&payload).unwrap();
}


fn do_encrypt(path: &str, password: &str) {
	let payload = {
		let mut data = Vec::new();
		File::open(path).unwrap().read_to_end(&mut data).unwrap();
		data
	};

	let encryption_parameters = Default::default();
	let file_key_suite = libfortress::fortresscrypto::FileKeySuite::derive(password.as_bytes(), &encryption_parameters);

	libfortress::fortresscrypto::encrypt_to_file(&mut io::stdout(), &payload, &encryption_parameters, &file_key_suite).unwrap();
}


fn create_and_fill_model(database: &Database) -> gtk::TreeModelFilter {
	let model = ListStore::new(&[String::static_type(), String::static_type()]);

	let mut entries: Vec<(ID, String, u64)> = database.get_root().list_entries(&database).iter().map(|id| {
		let entry = database.get_entry_by_id(id).unwrap();
		(**id, entry["title"].clone(), entry.get_time_created())
	}).collect();

	// Sort by time created (and then by ID as a tie breaker)
	entries.sort_by(|a, b| {
		a.2.cmp(&b.2).then(a.0.cmp(&b.0))
	});

	for entry in &entries {
		let hexid = HEXLOWER_PERMISSIVE.encode(&entry.0[..]);
		model.insert_with_values(None, &[0, 1], &[&hexid, &entry.1]);
	}

	gtk::TreeModelFilter::new(&model, None)
}


fn append_column(tree: &TreeView, id: i32) {
	let column = TreeViewColumn::new();
	let cell = CellRendererText::new();

	column.pack_start(&cell, true);
	column.add_attribute(&cell, "text", id);
	column.set_resizable(true);
	tree.append_column(&column);
}


struct EventMaster<T> {
	app: Rc<RefCell<T>>,
	rx: Rc<RefCell<Receiver<fn(&mut App)>>>,
	tx: Sender<fn(&mut App)>,
}


builder_ui!(UiReferences;
	window: gtk::Window,
	stack: gtk::Stack,

	stack_child_intro: gtk::Widget,
	intro_btn_open: gtk::Button,
	intro_btn_create: gtk::Button,

	stack_child_password: gtk::Widget,
	open_btn_open: gtk::Button,
	open_entry_password: gtk::Entry,

	stack_child_database: gtk::Widget,
	tree: gtk::TreeView,
	database_btn_menu: gtk::Button,
	database_btn_new_entry: gtk::Button,
	database_entry_search: gtk::Entry,

	stack_entry: gtk::Widget,
	entry_title: gtk::Entry,
	entry_username: gtk::Entry,
	entry_password: gtk::Entry,
	entry_url: gtk::Entry,
	entry_notes: gtk::TextView,
	entry_btn_save: gtk::Button,
	entry_btn_close: gtk::Button,
	entry_btn_generate_password: gtk::Button,

	stack_menu: gtk::Widget,
	menu_btn_close: gtk::Button,
	menu_btn_change_password: gtk::Button,
	menu_btn_sync: gtk::Button,

	stack_generate: gtk::Widget,
	generate_spin_count: gtk::SpinButton,
	generate_chk_uppercase: gtk::CheckButton,
	generate_chk_lowercase: gtk::CheckButton,
	generate_chk_numbers: gtk::CheckButton,
	generate_entry_other: gtk::Entry,
	generate_btn_generate: gtk::Button
);


enum AppState {
	Intro,
	OpenDatabasePassword,
	CreateDatabasePassword,
	ChangePassword,
	ViewDatabase,
	EditEntry,
	Menu,
	GeneratePassword,
}


struct App {
	state: AppState,
	database: Option<Database>,
	database_path: Option<PathBuf>,
	current_entry_id: Option<ID>,
	ui: UiReferences,

	entry_title: String,
	entry_username: String,
	entry_password: String,
	entry_url: String,
	entry_notes: String,

	database_search: Rc<RefCell<String>>,
	database_model: Option<gtk::TreeModelFilter>,
}

impl App {
	fn new() -> App {
		let builder = gtk::Builder::new_from_string(include_str!("window.glade"));
		let ui = UiReferences::from_builder(&builder);
		let database_path = env::args().nth(1).map(|path| PathBuf::from(path));

		append_column(&ui.tree, 1);

		ui.window.connect_delete_event(|_, _| {
			gtk::main_quit();
			Inhibit(false)
		});

		// Apply CSS
		let screen = ui.window.get_screen().unwrap();
		let provider = gtk::CssProvider::new();
		provider.load_from_data(include_str!("style.css")).unwrap();
		gtk::StyleContext::add_provider_for_screen(&screen, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

		if database_path.is_some() {
			ui.stack.set_visible_child(&ui.stack_child_password);
			ui.window.set_focus(Some(&ui.open_entry_password));
		} else {
			ui.stack.set_visible_child(&ui.stack_child_intro);
		}
		ui.window.show_all();


		App {
			state: if database_path.is_some() { AppState::OpenDatabasePassword } else { AppState::Intro },
			database: None,
			database_path: database_path,
			current_entry_id: None,
			ui: ui,

			entry_title: String::new(),
			entry_username: String::new(),
			entry_password: String::new(),
			entry_url: String::new(),
			entry_notes: String::new(),

			database_search: Rc::new(RefCell::new(String::new())),
			database_model: None,
		}
	}

	fn update(&mut self) {
		match self.state {
			AppState::Intro => self.ui.stack.set_visible_child(&self.ui.stack_child_intro),
			AppState::OpenDatabasePassword => {
				self.ui.stack.set_visible_child(&self.ui.stack_child_password);
				self.ui.open_btn_open.set_label("Open");
			},
			AppState::CreateDatabasePassword => {
				self.ui.stack.set_visible_child(&self.ui.stack_child_password);
				self.ui.open_btn_open.set_label("Create");
			},
			AppState::ChangePassword => {
				self.ui.stack.set_visible_child(&self.ui.stack_child_password);
				self.ui.open_btn_open.set_label("Change");
			},
			AppState::ViewDatabase => self.ui.stack.set_visible_child(&self.ui.stack_child_database),
			AppState::EditEntry => self.ui.stack.set_visible_child(&self.ui.stack_entry),
			AppState::Menu => self.ui.stack.set_visible_child(&self.ui.stack_menu),
			AppState::GeneratePassword => self.ui.stack.set_visible_child(&self.ui.stack_generate),
		}

		self.ui.entry_title.set_text(&self.entry_title);
		self.ui.entry_username.set_text(&self.entry_username);
		self.ui.entry_password.set_text(&self.entry_password);
		self.ui.entry_url.set_text(&self.entry_url);
		self.ui.entry_notes.get_buffer().unwrap().set_text(&self.entry_notes);

		// TODO: Be more efficient here
		let model = self.database.as_ref().map(|db| {
			let model = create_and_fill_model(db);
			let search_string = self.database_search.clone();

			model.set_visible_func(move |model,iter| {
				let title = model.get_value(&iter, 1).get::<String>().unwrap().to_lowercase();
				let search_string = &*search_string.borrow().to_lowercase();

				if search_string != "" {
					title.contains(search_string)
				} else {
					true
				}
			});
			model
		});
		self.ui.tree.set_model(model.as_ref());
		self.database_model = model;
	}

	fn connect_events(&self, master: &EventMaster<Self>) {
		// Intro
		connect!(master, self.ui.intro_btn_open, connect_clicked, intro_open_clicked);
		connect!(master, self.ui.intro_btn_create, connect_clicked, intro_create_clicked);

		// Password
		connect!(master, self.ui.open_btn_open, connect_clicked, password_btn_clicked);
		connect!(master, self.ui.open_entry_password, connect_activate, password_btn_clicked);

		// Database View
		connect!(master, self.ui.tree.get_selection(), connect_changed, on_cursor_changed);
		connect!(master, self.ui.database_btn_menu, connect_clicked, database_menu_clicked);
		connect!(master, self.ui.database_btn_new_entry, connect_clicked, database_new_entry_clicked);
		connect!(master, self.ui.database_entry_search, connect_changed, database_search_changed);

		// Entry View
		connect!(master, self.ui.entry_btn_save, connect_clicked, entry_save_clicked);
		connect!(master, self.ui.entry_btn_close, connect_clicked, entry_close_clicked);
		connect!(master, self.ui.entry_btn_generate_password, connect_clicked, entry_generate_password_clicked);
		connect!(master, self.ui.entry_title, connect_changed, entry_title_changed);
		connect!(master, self.ui.entry_username, connect_changed, entry_username_changed);
		connect!(master, self.ui.entry_password, connect_changed, entry_password_changed);
		connect!(master, self.ui.entry_url, connect_changed, entry_url_changed);
		connect!(master, self.ui.entry_notes.get_buffer().unwrap(), connect_changed, entry_notes_changed);

		// Menu
		connect!(master, self.ui.menu_btn_close, connect_clicked, menu_close_clicked);
		connect!(master, self.ui.menu_btn_change_password, connect_clicked, menu_change_password_clicked);
		connect!(master, self.ui.menu_btn_sync, connect_clicked, menu_sync_clicked);

		// Generate View
		connect!(master, self.ui.generate_btn_generate, connect_clicked, generate_btn_clicked);
	}

	fn on_cursor_changed(&mut self) {
		// HACK: For some reason GTK is trigger two of these events with every click.
		// The first event is correct, but the second event selects the first item in the list.
		// This hack will ignore the second event, because we'll be in a different state by then.
		match self.state {
			AppState::ViewDatabase => (),
			_ => return,
		}

		let selection = self.ui.tree.get_selection();

		if let Some((model, iter)) = selection.get_selected() {
			if let Some(ref mut database) = self.database {
				let hexid = model.get_value(&iter, 0).get::<String>().unwrap();
				self.current_entry_id = Some(ID::from_slice(&mut HEXLOWER_PERMISSIVE.decode(hexid.as_bytes()).unwrap()).unwrap());

				let entry = database.get_entry_by_id(&self.current_entry_id.unwrap()).unwrap();

				self.entry_title = entry["title"].to_string();
				self.entry_username = entry["username"].to_string();
				self.entry_password = entry["password"].to_string();
				self.entry_url = entry["url"].to_string();
				self.entry_notes = entry["notes"].to_string();

				self.state = AppState::EditEntry;
			}

			self.update();
		}
	}

	fn database_new_entry_clicked(&mut self) {
		self.current_entry_id = None;

		self.entry_title.clear();
		self.entry_username.clear();
		self.entry_password.clear();
		self.entry_url.clear();
		self.entry_notes.clear();

		self.state = AppState::EditEntry;
		self.update();
	}

	fn entry_save_clicked(&mut self) {
		let notes_buffer = self.ui.entry_notes.get_buffer().unwrap();
		let entry_data = libfortress::EntryHistory::new([
			("title".to_string(), self.ui.entry_title.get_text().unwrap()),
			("username".to_string(), self.ui.entry_username.get_text().unwrap()),
			("password".to_string(), self.ui.entry_password.get_text().unwrap()),
			("url".to_string(), self.ui.entry_url.get_text().unwrap()),
			("notes".to_string(), notes_buffer.get_text(&notes_buffer.get_start_iter(), &notes_buffer.get_end_iter(), false).unwrap()),
		].iter().cloned().collect());

		if let Some(entry_id) = self.current_entry_id {
			// Edit entry
			let entry = self.database.as_mut().unwrap().get_entry_by_id_mut(&entry_id).unwrap();
			entry.edit(entry_data);
		} else {
			// New entry
			let mut entry = libfortress::Entry::new();
			entry.edit(entry_data);
			self.current_entry_id = Some(entry.get_id().clone());
			self.database.as_mut().unwrap().add_entry(entry);
		}

		self.database.as_ref().unwrap().save_to_path(self.database_path.as_ref().unwrap()).unwrap();

		self.state = AppState::ViewDatabase;
		self.update();
	}

	fn password_btn_clicked(&mut self) {
		let password = self.ui.open_entry_password.get_text().unwrap();

		match self.state {
			AppState::OpenDatabasePassword => {
				self.database = Some(libfortress::Database::load_from_path(self.database_path.as_ref().unwrap(), password).unwrap());

				self.state = AppState::ViewDatabase;
				self.update();
			},
			AppState::CreateDatabasePassword => {
				// Select where to save the new database
				let dialog = gtk::FileChooserDialog::new(Some("Create Fortress"), Some(&self.ui.window), gtk::FileChooserAction::Save);

				dialog.add_buttons(&[
					("Create", gtk::ResponseType::Ok.into()),
					("Cancel", gtk::ResponseType::Cancel.into())
				]);

				dialog.set_select_multiple(false);
				let response = dialog.run();
				let ok: i32 = gtk::ResponseType::Ok.into();

				if response == ok {
					if let Some(file) = dialog.get_filename() {
						let path = PathBuf::from(file);
						// TODO: Username
						let database = libfortress::Database::new_with_password("", password);
						database.save_to_path(&path).unwrap();
						self.database = Some(database);
						self.database_path = Some(path);

						self.state = AppState::ViewDatabase;
						self.update();
					} else {
						self.state = AppState::Intro;
						self.update();
					}
				} else {
					self.state = AppState::Intro;
					self.update();
				}
				dialog.destroy();
			},
			AppState::ChangePassword => {
				let username = self.database.as_ref().unwrap().get_username().to_string();
				self.database.as_mut().unwrap().change_password(username, password);
				self.database.as_ref().unwrap().save_to_path(self.database_path.as_ref().unwrap()).unwrap();

				self.state = AppState::Menu;
				self.update();
			},
			_ => (),
		}
	}

	fn intro_open_clicked(&mut self) {
		// Select a database file to open
		let dialog = gtk::FileChooserDialog::new(Some("Open Fortress"), Some(&self.ui.window), gtk::FileChooserAction::Open);

		dialog.add_buttons(&[
			("Open", gtk::ResponseType::Ok.into()),
			("Cancel", gtk::ResponseType::Cancel.into())
		]);

		dialog.set_select_multiple(false);
		let response = dialog.run();
		let ok: i32 = gtk::ResponseType::Ok.into();

		if response == ok {
			if let Some(file) = dialog.get_filename() {
				self.database_path = Some(PathBuf::from(file));
				self.ui.window.set_focus(Some(&self.ui.open_entry_password));
				self.state = AppState::OpenDatabasePassword;
				self.update();
			}
		}
		dialog.destroy();
	}

	fn intro_create_clicked(&mut self) {
		self.state = AppState::CreateDatabasePassword;
		self.database_path = None;
		self.update();
	}

	fn database_menu_clicked(&mut self) {
		self.state = AppState::Menu;
		self.update();
	}

	fn menu_close_clicked(&mut self) {
		self.state = AppState::ViewDatabase;
		self.update();
	}

	fn menu_change_password_clicked(&mut self) {
		self.state = AppState::ChangePassword;
		self.update();
	}

	fn menu_sync_clicked(&mut self) {
		// TODO: Make URL configurable
		// TODO: Provide visual feedback
		if self.database.as_mut().unwrap().sync("https://127.0.0.1:9081") {
			// Database changed; save to disk.
			self.database.as_ref().unwrap().save_to_path(self.database_path.as_ref().unwrap()).unwrap();
		}
		
		self.state = AppState::ViewDatabase;
		self.update();
	}

	fn entry_close_clicked(&mut self) {
		self.state = AppState::ViewDatabase;
		self.update();
	}

	fn entry_generate_password_clicked(&mut self) {
		self.state = AppState::GeneratePassword;
		self.update();
	}

	fn generate_btn_clicked(&mut self) {
		let num_chars = self.ui.generate_spin_count.get_value_as_int();
		let uppercase_letters = self.ui.generate_chk_uppercase.get_active();
		let lowercase_letters = self.ui.generate_chk_lowercase.get_active();
		let numbers = self.ui.generate_chk_numbers.get_active();
		let other_chars = self.ui.generate_entry_other.get_text().unwrap();

		if !uppercase_letters && !lowercase_letters && !numbers && other_chars.len() == 0 {
			// TODO: Display an error
			return;
		}

		self.entry_password = libfortress::random_string(num_chars as usize, uppercase_letters, lowercase_letters, numbers, &other_chars);
		self.state = AppState::EditEntry;
		self.update();
	}

	fn entry_title_changed(&mut self) {
		self.entry_title = self.ui.entry_title.get_text().unwrap();
	}

	fn entry_username_changed(&mut self) {
		self.entry_username = self.ui.entry_username.get_text().unwrap();
	}

	fn entry_password_changed(&mut self) {
		self.entry_password = self.ui.entry_password.get_text().unwrap();
	}

	fn entry_url_changed(&mut self) {
		self.entry_url = self.ui.entry_url.get_text().unwrap();
	}

	fn entry_notes_changed(&mut self) {
		let notes_buffer = self.ui.entry_notes.get_buffer().unwrap();
		self.entry_notes = notes_buffer.get_text(&notes_buffer.get_start_iter(), &notes_buffer.get_end_iter(), false).unwrap();
	}

	fn database_search_changed(&mut self) {
		*self.database_search.borrow_mut() = self.ui.database_entry_search.get_text().unwrap();

		if let Some(ref model) = self.database_model {
			model.refilter();
		}
	}
}