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


#[derive(Clone)]
struct UiReferences {
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
	btn_new_entry: gtk::Button,
	btn_save_entry: gtk::Button,
	entry_title: gtk::Entry,
	entry_username: gtk::Entry,
	entry_password: gtk::Entry,
	entry_url: gtk::Entry,
	entry_notes: gtk::TextView,
}


struct App {
	database: fortress::Database,
	database_path: Option<PathBuf>,
	current_entry_id: Vec<u8>,
	ui: UiReferences,
}

impl App {
	fn new() -> App {
		let builder = gtk::Builder::new_from_string(include_str!("window.glade"));
		let ui = UiReferences {
			window: builder.get_object("window1").unwrap(),
			stack: builder.get_object("stack1").unwrap(),

			stack_child_intro: builder.get_object("stack-child-intro").unwrap(),
			intro_btn_open: builder.get_object("intro-btn-open").unwrap(),
			intro_btn_create: builder.get_object("intro-btn-create").unwrap(),

			stack_child_password: builder.get_object("stack-child-password").unwrap(),
			open_entry_password: builder.get_object("open-entry-password").unwrap(),
			open_btn_open: builder.get_object("open-btn-open").unwrap(),

			stack_child_database: builder.get_object("stack-child-database").unwrap(),
			tree: builder.get_object("entry-list").unwrap(),
			btn_new_entry: builder.get_object("btn-new-entry").unwrap(),
			btn_save_entry: builder.get_object("btn-save-entry").unwrap(),
			entry_title: builder.get_object("entry-title").unwrap(),
			entry_username: builder.get_object("entry-user-name").unwrap(),
			entry_password: builder.get_object("entry-password").unwrap(),
			entry_url: builder.get_object("entry-url").unwrap(),
			entry_notes: builder.get_object("entry-notes").unwrap(),
		};
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
			database: fortress::Database::default(),
			database_path: database_path,
			current_entry_id: Vec::new(),
			ui: ui,
		}
	}

	fn connect_events(&self, master: &EventMaster<Self>) {
		connect!(master, self.ui.tree, connect_cursor_changed, on_cursor_changed);
		connect!(master, self.ui.btn_new_entry, connect_clicked, on_new_entry);
		connect!(master, self.ui.btn_save_entry, connect_clicked, on_save_entry);
		connect!(master, self.ui.open_btn_open, connect_clicked, action_open_database);
		connect!(master, self.ui.intro_btn_open, connect_clicked, action_select_database);
		connect!(master, self.ui.intro_btn_create, connect_clicked, action_create_database);
	}

	fn on_cursor_changed(&mut self) {
		let selection = self.ui.tree.get_selection();

		if let Some((model, iter)) = selection.get_selected() {
			let hexid = model.get_value(&iter, 0).get::<String>().unwrap();
			self.current_entry_id.clear();
			self.current_entry_id.append(&mut hexid.from_hex().unwrap());
			let entry = self.database.get_entry_by_id(&self.current_entry_id).unwrap();
			let entry_data = entry.history.last().unwrap();

			self.ui.entry_title.set_text(&entry_data.title);
			self.ui.entry_username.set_text(&entry_data.username);
			self.ui.entry_password.set_text(&entry_data.password);
			self.ui.entry_url.set_text(&entry_data.url);
			self.ui.entry_notes.get_buffer().unwrap().set_text(&entry_data.notes);
		}
	}

	fn on_new_entry(&mut self) {
		self.current_entry_id.clear();

		self.ui.entry_title.set_text("");
		self.ui.entry_username.set_text("");
		self.ui.entry_password.set_text("");
		self.ui.entry_url.set_text("");
		self.ui.entry_notes.get_buffer().unwrap().set_text("");
	}

	fn on_save_entry(&mut self) {
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
			self.database.add_entry(entry);
		}
		else {
			// Edit entry
			let mut entry = self.database.get_entry_by_id(&self.current_entry_id).unwrap();
			entry.edit(&entry_data);
		}

		self.database.save_to_path(self.database_path.as_ref().unwrap()).unwrap();

		let model = create_and_fill_model(&self.database);
    	self.ui.tree.set_model(Some(&model));
	}

	fn action_open_database(&mut self) {
		let password = self.ui.open_entry_password.get_text().unwrap();

		if let Some(ref path) = self.database_path {
			// Open database using the password the user entered
			self.database = fortress::Database::load_from_path(path, password.as_bytes()).unwrap();
			let model = create_and_fill_model(&self.database);
			self.ui.tree.set_model(Some(&model));
			self.ui.stack.set_visible_child(&self.ui.stack_child_database);
		}
		else {
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
					self.database = fortress::Database::new_with_password(password.as_bytes());
					self.database.save_to_path(&path).unwrap();
					self.database_path = Some(path);

					let model = create_and_fill_model(&self.database);
					self.ui.tree.set_model(Some(&model));
					self.ui.stack.set_visible_child(&self.ui.stack_child_database);
				}
			}
			dialog.destroy();
		}
	}

	fn action_select_database(&mut self) {
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
				self.ui.stack.set_visible_child(&self.ui.stack_child_password);
				self.ui.open_btn_open.set_label("Open");
			}
		}
		dialog.destroy();
	}

	fn action_create_database(&mut self) {
		// Display the password entry panel
		self.ui.stack.set_visible_child(&self.ui.stack_child_password);
		self.ui.open_btn_open.set_label("Create");
		self.database_path = None;
	}
}