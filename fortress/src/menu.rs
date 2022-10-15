use std::{
	cell::{Ref, RefCell, RefMut},
	path::PathBuf,
	rc::Rc,
};

use libfortress::Database;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, WidgetPlus, Widgets};
use url::Url;

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum MenuMsg {
	// From UI
	SyncClicked,
	ChangeUsernameAndPasswordClicked {
		username: String,
		password: String,
		repeat_password: String,
	},
	CloseClicked,
	Refresh,
}

pub struct MenuModel {
	sync_url_entry: gtk::EntryBuffer,
	username: String,
	sync_keys: String,

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
			username: String::new(),
			sync_keys: String::new(),

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
			MenuMsg::ChangeUsernameAndPasswordClicked {
				username,
				password,
				repeat_password,
			} => {
				if password != repeat_password {
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

				// TODO: Show a success dialog and clear the UI fields.
			},
			MenuMsg::CloseClicked => {
				if self.save_sync_url(&parent_sender).is_err() {
					return;
				}

				self.username.clear();
				self.sync_keys.clear();

				send!(parent_sender, AppMsg::CloseMenu);
			},
			MenuMsg::Refresh => {
				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				let sync_keys = format!("{}:{}", database.get_login_id().to_hex(), database.get_login_key().to_hex());
				let sync_url = database.get_sync_url().map(Url::as_str).unwrap_or_default();

				self.sync_keys = sync_keys;
				self.username = database.get_username().to_owned();
				self.sync_url_entry.set_text(sync_url);
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
			set_vexpand: true,

			append = &gtk::CenterBox {
				set_margin_all: 10,

				set_start_widget = Some(&gtk::Button) {
					set_icon_name: "go-previous-symbolic",

					connect_clicked(sender) => move |_| {
						send!(sender, MenuMsg::CloseClicked);
					},
				},

				set_center_widget = Some(&gtk::Label) {
					set_text: "Settings",
					add_css_class: "h1",
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Vertical,
				set_halign: gtk::Align::Center,
				set_valign: gtk::Align::Center,
				set_vexpand: true,
				set_spacing: 10,

				append = &gtk::Label {
					set_text: "Sync",
					add_css_class: "h2",
					set_hexpand: true,
					set_halign: gtk::Align::Start,
				},

				append = &gtk::Grid {
					set_row_spacing: 5,
					set_column_spacing: 10,

					attach(1, 1, 1, 1) = &gtk::Label {
						set_text: "Sync URL",
						set_halign: gtk::Align::End,
					},
					attach(2, 1, 1, 1) = &gtk::Entry {
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
					set_margin_top: 20,
					set_margin_bottom: 20,
				},

				append = &gtk::Label {
					set_text: "Username and Password",
					add_css_class: "h2",
					set_hexpand: true,
					set_halign: gtk::Align::Start,
				},

				append = &gtk::Grid {
					set_row_spacing: 5,
					set_column_spacing: 10,

					attach(1, 1, 1, 1) = &gtk::Label {
						set_text: "Username",
						set_halign: gtk::Align::End,
					},
					attach(2, 1, 1, 1): username = &gtk::Entry {
						set_text: watch!(&model.username),
						set_hexpand: true,
						set_width_chars: 30,
					},
					attach(1, 2, 1, 1) = &gtk::Label {
						set_text: "Password",
						set_halign: gtk::Align::End,
					},
					attach(2, 2, 1, 1): password = &gtk::PasswordEntry {
						set_hexpand: true,
						set_show_peek_icon: true,

						connect_unmap => |entry| {
							entry.set_text("");
						},
					},
					attach(1, 3, 1, 1) = &gtk::Label {
						set_text: "Password (again)",
						set_halign: gtk::Align::End,
					},
					attach(2, 3, 1, 1): repeat_password = &gtk::PasswordEntry {
						set_hexpand: true,
						set_show_peek_icon: true,

						connect_unmap => |entry| {
							entry.set_text("");
						},
					},
				},

				append = &gtk::Button {
					set_label: "Change",
					set_hexpand: true,
					connect_clicked(username, password, repeat_password, sender) => move |_| {
						send!(sender, MenuMsg::ChangeUsernameAndPasswordClicked {
							username: username.text().to_string(),
							password: password.text().to_string(),
							repeat_password: repeat_password.text().to_string(),
						});
					},
				},

				append = &gtk::Separator {
					set_orientation: gtk::Orientation::Horizontal,
					set_margin_top: 20,
					set_margin_bottom: 20,
				},

				append = &gtk::Label {
					set_text: "Sync Keys",
					add_css_class: "h2",
					set_hexpand: true,
					set_halign: gtk::Align::Start,
				},

				append = &gtk::PasswordEntry {
					set_hexpand: true,
					set_editable: false,
					set_can_focus: false,
					set_show_peek_icon: true,
					set_text: watch!(&model.sync_keys),
				},
			},

			// This event is emitted moreorless when this component gets switched to by the parent GtkStack.
			connect_map(sender) => move |_| {
				send!(sender, MenuMsg::Refresh);
			},
		}
	}
}
