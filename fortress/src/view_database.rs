use std::{
	cell::{Ref, RefCell},
	rc::Rc,
};

use data_encoding::HEXLOWER_PERMISSIVE;
use libfortress::{Database, ID};
use relm4::{
	gtk,
	gtk::{prelude::*, CellRendererText, ListStore, TreeModelFilter, TreeViewColumn},
	send, ComponentUpdate, Model, Sender, Widgets,
};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum ViewDatabaseMsg {
	MenuClicked,
	NewEntryClicked,
	ViewEntry(ID),
	Refresh,
	Refilter,
}

pub struct ViewDatabaseModel {
	database: Rc<RefCell<Option<Database>>>,
	list_store: ListStore,
	list_model: TreeModelFilter,
	search_entry: gtk::EntryBuffer,
}

impl Model for ViewDatabaseModel {
	type Msg = ViewDatabaseMsg;
	type Widgets = ViewDatabaseWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for ViewDatabaseModel {
	fn init_model(parent_model: &AppModel) -> Self {
		let list_store = ListStore::new(&[String::static_type(), String::static_type()]);
		let list_model = TreeModelFilter::new(&list_store, None);
		let search_entry = gtk::EntryBuffer::new(None);

		// TODO: Fuzzy search
		let search_entry_clone = search_entry.clone();
		list_model.set_visible_func(move |model, iter| {
			let title = model.get_value(iter, 1).get::<String>().unwrap().to_lowercase();
			let search_string = search_entry_clone.text().to_lowercase();

			if !search_string.is_empty() {
				title.contains(&search_string)
			} else {
				true
			}
		});

		Self {
			database: parent_model.database.clone(),
			list_store,
			list_model,
			search_entry,
		}
	}

	fn update(&mut self, msg: ViewDatabaseMsg, _components: &(), _sender: Sender<ViewDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			ViewDatabaseMsg::MenuClicked => {
				send!(parent_sender, AppMsg::ShowMenu);
			},
			ViewDatabaseMsg::NewEntryClicked => {
				send!(parent_sender, AppMsg::NewEntry);
			},
			ViewDatabaseMsg::ViewEntry(id) => {
				send!(parent_sender, AppMsg::EditEntry(id));
			},
			ViewDatabaseMsg::Refresh => {
				self.list_store.clear();

				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				let mut entries: Vec<(ID, String, u64)> = database
					.get_root()
					.list_entries(&database)
					.iter()
					.map(|id| {
						let entry = database.get_entry_by_id(id).unwrap();
						(**id, entry["title"].clone(), entry.get_time_created())
					})
					.collect();

				// Sort by time created (and then by ID as a tie breaker)
				entries.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));

				for entry in &entries {
					let hexid = HEXLOWER_PERMISSIVE.encode(&entry.0[..]);
					self.list_store.insert_with_values(None, &[(0, &hexid), (1, &entry.1)]);
				}
			},
			ViewDatabaseMsg::Refilter => {
				self.list_model.refilter();
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<ViewDatabaseModel, AppModel> for ViewDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Entry {
				set_buffer: &model.search_entry,
				set_placeholder_text: Some("Search"),

				connect_changed(sender) => move |_| {
					send!(sender, ViewDatabaseMsg::Refilter);
				},
			},

			append: tree = &gtk::TreeView::with_model(&model.list_model) {
				set_headers_visible: false,
				set_vexpand: true,

				connect_row_activated(sender) => move |tree, _, _| {
					let selection = tree.selection();
					if let Some((model, iter)) = selection.selected() {
						let id = model.get_value(&iter, 0).get::<String>().unwrap();
						let id = ID::from_slice(&HEXLOWER_PERMISSIVE.decode(id.as_bytes()).unwrap()).unwrap();

						send!(sender, ViewDatabaseMsg::ViewEntry(id));
					}
				},
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_spacing: 5,
				set_hexpand: true,

				append = &gtk::Button {
					set_label: "Menu",
					set_margin_start: 40,
					set_margin_end: 40,
					set_margin_top: 40,
					connect_clicked(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::MenuClicked);
					},
				},

				append = &gtk::Button {
					set_label: "New entry",
					set_margin_start: 40,
					set_margin_end: 40,
					set_margin_top: 40,
					connect_clicked(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::NewEntryClicked);
					},
				},
			},

			connect_map(sender) => move |_| {
				send!(sender, ViewDatabaseMsg::Refresh);
			},
		}
	}

	fn post_init() {
		// TODO: Is there anyway to do this in the view macro?
		let column = TreeViewColumn::new();
		let cell = CellRendererText::new();

		column.pack_start(&cell, true);
		column.add_attribute(&cell, "text", 1);
		column.set_resizable(true);
		tree.append_column(&column);
	}
}
