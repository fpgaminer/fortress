use std::{
	cell::{Ref, RefCell, RefMut},
	io,
	path::PathBuf,
	rc::Rc,
};

use libfortress::Database;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg, Config};


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
	config_path: PathBuf,
	config: Rc<RefCell<Config>>,
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
			config_path: parent_model.config_path.clone(),
			config: parent_model.config.clone(),
		}
	}

	fn update(&mut self, msg: MenuMsg, _components: &(), _sender: Sender<MenuMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			MenuMsg::SyncClicked => {
				if let Err(err) = self.save_config() {
					send!(parent_sender, AppMsg::ShowError(format!("Failed to save config: {}", err)));
					return;
				}

				let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
					Ok(database) => database,
					Err(_) => return,
				};

				let config = self.config.borrow();

				if let Err(err) = database.sync(&config.sync_url) {
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
				if let Err(err) = self.save_config() {
					send!(parent_sender, AppMsg::ShowError(format!("Failed to save config: {}", err)));
					return;
				}

				self.username_entry.set_text("");
				self.password_entry.set_text("");
				self.password_again_entry.set_text("");
				send!(parent_sender, AppMsg::CloseMenu);
			},
			MenuMsg::Refresh => {
				let config = self.config.borrow();
				self.sync_url_entry.set_text(&config.sync_url);

				let database = Ref::filter_map(self.database.borrow(), |database| database.as_ref());

				let sync_keys = match database {
					Ok(ref database) => format!("{}:{}", database.get_login_id().to_hex(), database.get_login_key().to_hex()),
					Err(_) => "".to_string(),
				};

				self.sync_keys_entry.set_text(&sync_keys);

				if let Ok(database) = database {
					self.username_entry.set_text(&database.get_username());
				}
			},
			MenuMsg::ShowSyncKeysClicked => {
				self.show_sync_keys = !self.show_sync_keys;
			},
		}
	}
}

impl MenuModel {
	fn save_config(&self) -> Result<(), io::Error> {
		let mut config = self.config.borrow_mut();
		let sync_url = self.sync_url_entry.text();

		if config.sync_url == sync_url {
			return Ok(());
		}

		config.sync_url = sync_url;

		config.save_to_path(&self.config_path)
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
