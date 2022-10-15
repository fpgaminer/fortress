use std::path::PathBuf;

use libfortress::{fortresscrypto::CryptoError, Database, FortressError};
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum OpenDatabaseMsg {
	OpenClicked(String),
}

pub struct OpenDatabaseModel {
	database_path: PathBuf,
}

impl Model for OpenDatabaseModel {
	type Msg = OpenDatabaseMsg;
	type Widgets = OpenDatabaseWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for OpenDatabaseModel {
	fn init_model(parent_model: &AppModel) -> Self {
		Self {
			database_path: parent_model.database_path.clone(),
		}
	}

	fn update(&mut self, msg: OpenDatabaseMsg, _components: &(), _sender: Sender<OpenDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			OpenDatabaseMsg::OpenClicked(password) => match Database::load_from_path(&self.database_path, &password) {
				Ok(database) => {
					send!(parent_sender, AppMsg::DatabaseOpened(database));
				},
				Err(err) => {
					let message = match err {
						FortressError::CryptoError(CryptoError::DecryptionError) => "Incorrect password.".to_owned(),
						FortressError::CryptoError(CryptoError::BadChecksum) => "File is corrupted.".to_owned(),
						err => format!("{}", err),
					};

					send!(parent_sender, AppMsg::ShowError(format!("Failed to open database: {}", message)));
				},
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<OpenDatabaseModel, AppModel> for OpenDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,
			set_halign: gtk::Align::Center,
			set_valign: gtk::Align::Center,
			add_css_class: "open-database",

			append = &gtk::Label {
				set_label: "Enter Password",
				set_halign: gtk::Align::Center,
				add_css_class: "h1",
			},

			append = &gtk::Label {
				set_label: "Enter your password to unlock the Fortress.",
				set_halign: gtk::Align::Start,
				add_css_class: "h-sub",
			},

			append: password_entry = &gtk::PasswordEntry {
				set_halign: gtk::Align::Fill,
				set_hexpand: true,
				set_show_peek_icon: true,
				connect_activate(sender) => move |entry| {
					send!(sender, OpenDatabaseMsg::OpenClicked(entry.text().to_string()));
				},
				connect_unmap => move |entry| {
					// Clear the password after the database is opened, just to be safe.
					entry.set_text("");
				},
			},

			append = &gtk::Button {
				set_label: "Unlock",
				set_halign: gtk::Align::Fill,
				add_css_class: "action-btn",
				set_hexpand: true,
				connect_clicked(password_entry, sender) => move |_| {
					send!(sender, OpenDatabaseMsg::OpenClicked(password_entry.text().to_string()));
				},
			},
		}
	}
}
