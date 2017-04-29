extern crate fortress;
extern crate gtk;
extern crate rustc_serialize;

use fortress::{Database};
use gtk::prelude::*;
use gtk::{CellRendererText, ListStore, TreeView, TreeViewColumn};
use std::rc::Rc;
use std::cell::RefCell;
use rustc_serialize::hex::{ToHex, FromHex};
use std::env;
use std::path::PathBuf;
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
						$name: builder.get_object(stringify!($name)).expect(concat!("Glade missing ", stringify!($name))),
					)*
				}
			}
		}
	};
}


fn main() {
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


fn create_and_fill_model (database: &Database) -> ListStore {
	let model = ListStore::new(&[String::static_type(), String::static_type()]);

	for entry in &database.entries {
		let hexid = entry.id.to_hex();
		let entry_data = entry.history.last().unwrap();
		model.insert_with_values (None, &[0, 1], &[&hexid, &entry_data.title]);
	}

	model
}


fn append_column (tree: &TreeView, id: i32) {
	let column = TreeViewColumn::new();
	let cell = CellRendererText::new();

    column.pack_start(&cell, true);
    column.add_attribute(&cell, "text", id);
	column.set_resizable (true);
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

	stack_entry: gtk::Widget,
	entry_title: gtk::Entry,
	entry_username: gtk::Entry,
	entry_password: gtk::Entry,
	entry_url: gtk::Entry,
	entry_notes: gtk::TextView,
	entry_btn_save: gtk::Button,
	entry_btn_close: gtk::Button,

	stack_menu: gtk::Widget,
	menu_btn_close: gtk::Button,
	menu_btn_change_password: gtk::Button
);


enum AppState {
	Intro,
	OpenDatabasePassword,
	CreateDatabasePassword,
	ChangePassword,
	ViewDatabase,
	EditEntry,
	Menu,
}


struct App {
	state: AppState,
	database: Option<fortress::Database>,
	database_path: Option<PathBuf>,
	current_entry_id: Vec<u8>,
	ui: UiReferences,

	entry_title: String,
	entry_username: String,
	entry_password: String,
	entry_url: String,
	entry_notes: String,
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
		provider.load_from_path("src/style.css").unwrap();
		gtk::StyleContext::add_provider_for_screen(&screen, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

		if database_path.is_some() {
			ui.stack.set_visible_child(&ui.stack_child_password);
		}
		else {
			ui.stack.set_visible_child(&ui.stack_child_intro);
		}

		ui.window.show_all();

		App {
			state: AppState::Intro,
			database: None,
			database_path: database_path,
			current_entry_id: Vec::new(),
			ui: ui,
			
			entry_title: String::new(),
			entry_username: String::new(),
			entry_password: String::new(),
			entry_url: String::new(),
			entry_notes: String::new(),
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
		}

		self.ui.entry_title.set_text(&self.entry_title);
		self.ui.entry_username.set_text(&self.entry_username);
		self.ui.entry_password.set_text(&self.entry_password);
		self.ui.entry_url.set_text(&self.entry_url);
		self.ui.entry_notes.get_buffer().unwrap().set_text(&self.entry_notes);

		// TODO: Be more efficient here
		let model = self.database.as_ref().map(|db| create_and_fill_model(db));
		self.ui.tree.set_model(model.as_ref());
	}

	fn connect_events(&self, master: &EventMaster<Self>) {
		// Intro
		connect!(master, self.ui.intro_btn_open, connect_clicked, intro_open_clicked);
		connect!(master, self.ui.intro_btn_create, connect_clicked, intro_create_clicked);

		// Password
		connect!(master, self.ui.open_btn_open, connect_clicked, password_btn_clicked);

		// Database View
		connect!(master, self.ui.tree.get_selection(), connect_changed, on_cursor_changed);
		connect!(master, self.ui.database_btn_menu, connect_clicked, database_menu_clicked);
		connect!(master, self.ui.database_btn_new_entry, connect_clicked, database_new_entry_clicked);

		// Entry View
		connect!(master, self.ui.entry_btn_save, connect_clicked, entry_save_clicked);
		connect!(master, self.ui.entry_btn_close, connect_clicked, entry_close_clicked);

		// Menu
		connect!(master, self.ui.menu_btn_close, connect_clicked, menu_close_clicked);
		connect!(master, self.ui.menu_btn_change_password, connect_clicked, menu_change_password_clicked);
		
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
				self.current_entry_id.clear();
				self.current_entry_id.append(&mut hexid.from_hex().unwrap());

				let entry = database.get_entry_by_id(&self.current_entry_id).unwrap();
				let entry_data = entry.history.last().unwrap();

				self.entry_title = entry_data.title.clone();
				self.entry_username = entry_data.username.clone();
				self.entry_password = entry_data.password.clone();
				self.entry_url = entry_data.url.clone();
				self.entry_notes = entry_data.notes.clone();

				self.state = AppState::EditEntry;
			}

			self.update();
		}
	}

	fn database_new_entry_clicked(&mut self) {
		self.current_entry_id.clear();

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
		let entry_data = fortress::EntryData::new(
			&self.ui.entry_title.get_text().unwrap(),
			&self.ui.entry_username.get_text().unwrap(),
			&self.ui.entry_password.get_text().unwrap(),
			&self.ui.entry_url.get_text().unwrap(),
			&notes_buffer.get_text(&notes_buffer.get_start_iter(), &notes_buffer.get_end_iter(), false).unwrap(),
		);

		if self.current_entry_id.len() == 0 {
			// New entry
			let mut entry = fortress::Entry::new();
			entry.edit(&entry_data);
			self.current_entry_id.clear();
			self.current_entry_id.extend_from_slice(&entry.id);
			self.database.as_mut().unwrap().add_entry(entry);
		}
		else {
			// Edit entry
			let mut entry = self.database.as_mut().unwrap().get_entry_by_id(&self.current_entry_id).unwrap();
			entry.edit(&entry_data);
		}

		self.database.as_ref().unwrap().save_to_path(self.database_path.as_ref().unwrap()).unwrap();

		self.state = AppState::ViewDatabase;
		self.update();
	}

	fn password_btn_clicked(&mut self) {
		let password = self.ui.open_entry_password.get_text().unwrap();

		match self.state {
			AppState::OpenDatabasePassword => {
				self.database = Some(fortress::Database::load_from_path(self.database_path.as_ref().unwrap(), password.as_bytes()).unwrap());

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
						let database = fortress::Database::new_with_password(password.as_bytes());
						database.save_to_path(&path).unwrap();
						self.database = Some(database);
						self.database_path = Some(path);

						self.state = AppState::ViewDatabase;
						self.update();
					}
					else {
						self.state = AppState::Intro;
						self.update();
					}
				}
				else {
					self.state = AppState::Intro;
					self.update();
				}
				dialog.destroy();
			},
			AppState::ChangePassword => {
				self.database.as_mut().unwrap().change_password(password.as_bytes());
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

	fn entry_close_clicked(&mut self) {
		self.state = AppState::ViewDatabase;
		self.update();
	}
}