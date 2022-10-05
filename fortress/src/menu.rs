use std::{
	cell::{Ref, RefCell, RefMut},
	path::PathBuf,
	rc::Rc,
};

use libfortress::Database;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};
use url::Url;

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum MenuMsg {
	// From UI
	SyncClicked,
	ChangeUsernameAndPasswordClicked,
	CloseClicked,
	Refresh,
	ShowSyncKeysClicked,
}

pub struct MenuModel {
	sync_url_entry: gtk::EntryBuffer,
	username_entry: gtk::EntryBuffer,
	password_entry: gtk::EntryBuffer,
	password_again_entry: gtk::EntryBuffer,
	sync_keys_entry: gtk::EntryBuffer,
	show_sync_keys: bool,

	database: Rc<RefCell<Option<Database>>>,
	database_path: PathBuf,
}

impl Model for MenuModel {
	type Msg = MenuMsg;
	type Widgets = MenuWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for MenuModel {
	fn init_model(parent_model: &AppModel) -> Self {
		Self {
			sync_url_entry: gtk::EntryBuffer::new(None),
			username_entry: gtk::EntryBuffer::new(None),
			password_entry: gtk::EntryBuffer::new(None),
			password_again_entry: gtk::EntryBuffer::new(None),
			sync_keys_entry: gtk::EntryBuffer::new(None),
			show_sync_keys: false,

			database: parent_model.database.clone(),
			database_path: parent_model.database_path.clone(),
		}
	}

	#[allow(clippy::needless_return)]
	fn update(&mut self, msg: MenuMsg, _components: &(), _sender: Sender<MenuMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			MenuMsg::SyncClicked => {
				if self.save_sync_url(&parent_sender).is_err() {
					return;
				}

				let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
					Ok(database) => database,
					Err(_) => return,
				};

				if let Err(err) = database.sync() {
					send!(parent_sender, AppMsg::ShowError(format!("Failed to sync: {}", err)));
					return;
				}

				if let Err(err) = database.save_to_path(&self.database_path) {
					// TODO: This is a fatal error.  We should use a different dialog that allows the user to try and save again, or quit the application.
					send!(parent_sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
					return;
				}
			},
			MenuMsg::ChangeUsernameAndPasswordClicked => {
				let username = self.username_entry.text();
				let password = self.password_entry.text();
				let password_again = self.password_again_entry.text();

				if password != password_again {
					send!(parent_sender, AppMsg::ShowError("Passwords do not match".to_string()));
					return;
				}

				let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
					Ok(database) => database,
					Err(_) => return,
				};

				database.change_password(&username, &password);

				if let Err(err) = database.save_to_path(&self.database_path) {
					// TODO: This is a fatal error.  We should use a different dialog that allows the user to try and save again, or quit the application.
					send!(parent_sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
					return;
				}
			},
			MenuMsg::CloseClicked => {
				if self.save_sync_url(&parent_sender).is_err() {
					return;
				}

				self.username_entry.set_text("");
				self.password_entry.set_text("");
				self.password_again_entry.set_text("");
				send!(parent_sender, AppMsg::CloseMenu);
			},
			MenuMsg::Refresh => {
				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				let sync_keys = format!("{}:{}", database.get_login_id().to_hex(), database.get_login_key().to_hex());
				let sync_url = database.get_sync_url().map(Url::as_str).unwrap_or_default();

				self.sync_keys_entry.set_text(&sync_keys);
				self.sync_url_entry.set_text(sync_url);
				self.username_entry.set_text(database.get_username());
			},
			MenuMsg::ShowSyncKeysClicked => {
				self.show_sync_keys = !self.show_sync_keys;
			},
		}
	}
}

impl MenuModel {
	fn save_sync_url(&self, parent_sender: &Sender<AppMsg>) -> Result<(), ()> {
		let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
			Ok(database) => database,
			Err(_) => return Err(()),
		};

		let sync_url = if self.sync_url_entry.text().is_empty() {
			None
		} else {
			match Url::parse(&self.sync_url_entry.text()) {
				Ok(url) => Some(url),
				Err(_) => {
					send!(parent_sender, AppMsg::ShowError("Invalid sync URL".to_string()));
					return Err(());
				},
			}
		};

		database.set_sync_url(sync_url);

		if let Err(err) = database.save_to_path(&self.database_path) {
			send!(parent_sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
			Err(())
		} else {
			Ok(())
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<MenuModel, AppModel> for MenuWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::Label {
					set_text: "Sync URL:",
				},
				append = &gtk::Entry {
					set_buffer: &model.sync_url_entry,
					set_hexpand: true,
					set_input_purpose: gtk::InputPurpose::Url,
				},
			},

			append = &gtk::Button {
				set_label: "Sync",
				set_hexpand: true,
				connect_clicked(sender) => move |_| {
					send!(sender, MenuMsg::SyncClicked);
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::Label {
					set_text: "Username:",
				},
				append = &gtk::Entry {
					set_buffer: &model.username_entry,
					set_hexpand: true,
				},
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::Label {
					set_text: "Password:",
				},
				append = &gtk::Entry {
					set_buffer: &model.password_entry,
					set_hexpand: true,
					set_visibility: false,
					set_input_purpose: gtk::InputPurpose::Password,
				},
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::Label {
					set_text: "Password (again):",
				},
				append = &gtk::Entry {
					set_buffer: &model.password_again_entry,
					set_hexpand: true,
					set_visibility: false,
					set_input_purpose: gtk::InputPurpose::Password,
				},
			},

			append = &gtk::Button {
				set_label: "Change username and password",
				set_hexpand: true,
				connect_clicked(sender) => move |_| {
					send!(sender, MenuMsg::ChangeUsernameAndPasswordClicked);
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::Label {
					set_text: "Sync keys:",
				},
				append = &gtk::Entry {
					set_buffer: &model.sync_keys_entry,
					set_hexpand: true,
					set_visibility: watch!(model.show_sync_keys),
					set_input_purpose: gtk::InputPurpose::Password,
					set_editable: false,
					set_can_focus: false,
				},
				append = &gtk::Button {
					set_label: watch!(if model.show_sync_keys { "Hide" } else { "Show" }),
					connect_clicked(sender) => move |_| {
						send!(sender, MenuMsg::ShowSyncKeysClicked);
					},
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
			},

			append = &gtk::Button {
				set_label: "Close",
				set_hexpand: true,
				connect_clicked(sender) => move |_| {
					send!(sender, MenuMsg::CloseClicked);
				},
			},

			// This event is emitted moreorless when this component gets switched to by the parent GtkStack.
			connect_map(sender) => move |_| {
				send!(sender, MenuMsg::Refresh);
			},
		}
	}
}
