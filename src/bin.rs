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


macro_rules! clone {
	($($n:ident),+; || $body:block) => (
		{
			$( let $n = $n.clone(); )+
			move || { $body }
		}
	);
	($($n:ident),+; |_| $body:block) => (
		{
			$( let $n = $n.clone(); )+
			move |_| { $body }
		}
	);
	($($n:ident),+; |$($p:ident),+| $body:block) => (
		{
			$( let $n = $n.clone(); )+
			move |$($p),+| { $body }
		}
	);
}


fn main() {
	// Initialize GTK
	if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

	// Build UI components from Glade description
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
	let mut app = App::default();
	app.database_path = env::args().nth(1).map(|path| PathBuf::from(path));

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

	// UI logic
	let app = Rc::new(RefCell::new(app));
	let current_entry_id = Rc::new(RefCell::new(Vec::<u8>::new()));

	ui.tree.connect_cursor_changed(clone!(app,ui,current_entry_id; |_| {
		let selection = ui.tree.get_selection();
		let mut app = app.borrow_mut();
		let mut current_entry_id = current_entry_id.borrow_mut();

		if let Some((model, iter)) = selection.get_selected() {
			let hexid = model.get_value(&iter, 0).get::<String>().unwrap();
			current_entry_id.clear();
			current_entry_id.append(&mut hexid.from_hex().unwrap());
			let entry = app.database.get_entry_by_id(&current_entry_id).unwrap();
			let entry_data = entry.history.last().unwrap();

			ui.entry_title.set_text(&entry_data.title);
			ui.entry_username.set_text(&entry_data.username);
			ui.entry_password.set_text(&entry_data.password);
			ui.entry_url.set_text(&entry_data.url);
			ui.entry_notes.get_buffer().unwrap().set_text(&entry_data.notes);
		}
	}));

	ui.btn_new_entry.connect_clicked(clone!(ui,current_entry_id; |_| {
		let mut current_entry_id = current_entry_id.borrow_mut();
		current_entry_id.clear();

		ui.entry_title.set_text("");
		ui.entry_username.set_text("");
		ui.entry_password.set_text("");
		ui.entry_url.set_text("");
		ui.entry_notes.get_buffer().unwrap().set_text("");
	}));

	ui.btn_save_entry.connect_clicked(clone!(app,ui,current_entry_id; |_| {
		let model = {
			let mut current_entry_id = current_entry_id.borrow_mut();
			let mut app = app.borrow_mut();

			let notes_buffer = ui.entry_notes.get_buffer().unwrap();
			let entry_data = fortress::EntryData::new(
				&ui.entry_title.get_text().unwrap(),
				&ui.entry_username.get_text().unwrap(),
				&ui.entry_password.get_text().unwrap(),
				&ui.entry_url.get_text().unwrap(),
				&notes_buffer.get_text(&notes_buffer.get_start_iter(), &notes_buffer.get_end_iter(), false).unwrap(),
			);

			if current_entry_id.len() == 0 {
				// New entry
				let mut entry = fortress::Entry::new();
				entry.edit(&entry_data);
				current_entry_id.clear();
				current_entry_id.extend_from_slice(&entry.id);
				app.database.add_entry(entry);
			}
			else {
				// Edit entry
				let mut entry = app.database.get_entry_by_id(&current_entry_id).unwrap();
				entry.edit(&entry_data);
			}

			app.database.save_to_path("test.fortressdb").unwrap();

			create_and_fill_model(&app.database)
		};
    	ui.tree.set_model(Some(&model));
	}));

	ui.open_btn_open.connect_clicked(clone!(app,ui; |_| {
		// Open database using the password the user entered
		let password = ui.open_entry_password.get_text().unwrap();
		let model = {
			let mut app = app.borrow_mut();
			let path = app.database_path.clone().unwrap();

			app.database = fortress::Database::load_from_path(path, password.as_bytes()).unwrap();
			create_and_fill_model(&app.database)
		};
		ui.tree.set_model(Some(&model));
		ui.stack.set_visible_child(&ui.stack_child_database);
	}));

	ui.intro_btn_open.connect_clicked(clone!(app,ui; |_| {
		// Select a database file to open
		let dialog = gtk::FileChooserDialog::new(Some("Open Fortress"), Some(&ui.window), gtk::FileChooserAction::Open);

		dialog.add_buttons(&[
			("Open", gtk::ResponseType::Ok.into()),
			("Cancel", gtk::ResponseType::Cancel.into())
		]);

		dialog.set_select_multiple(false);
		let response = dialog.run();
		let ok: i32 = gtk::ResponseType::Ok.into();
		
		if response == ok {
			let file = dialog.get_filename();

			if let Some(file) = dialog.get_filename() {
				let mut app = app.borrow_mut();
				app.database_path = Some(PathBuf::from(file));
				ui.stack.set_visible_child(&ui.stack_child_password);
			}
		}
		dialog.destroy();
	}));

	ui.intro_btn_create.connect_clicked(clone!(app,ui; |_| {
	}));

	if app.borrow().database_path.is_some() {
		ui.stack.set_visible_child(&ui.stack_child_password);
	}
	else {
		ui.stack.set_visible_child(&ui.stack_child_intro);
	}

	ui.window.show_all();
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


#[derive(Default)]
struct App {
	database: fortress::Database,
	database_path: Option<PathBuf>,
}