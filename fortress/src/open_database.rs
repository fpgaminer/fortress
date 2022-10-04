use std::path::PathBuf;

use libfortress::{fortresscrypto::CryptoError, Database, FortressError};
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum OpenDatabaseMsg {
	OpenClicked,
}

pub struct OpenDatabaseModel {
	database_path: PathBuf,

	password_entry: gtk::EntryBuffer,
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

			password_entry: gtk::EntryBuffer::new(None),
		}
	}

	fn update(&mut self, msg: OpenDatabaseMsg, _components: &(), _sender: Sender<OpenDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			OpenDatabaseMsg::OpenClicked => {
				let password = self.password_entry.text();

				match Database::load_from_path(&self.database_path, &password) {
					Ok(database) => {
						send!(parent_sender, AppMsg::DatabaseOpened(database));

						self.password_entry.set_text("");
					},
					Err(err) => {
						let message = match err {
							FortressError::CryptoError(CryptoError::DecryptionError) => "Incorrect password.".to_owned(),
							FortressError::CryptoError(CryptoError::BadChecksum) => "File is corrupted.".to_owned(),
							err => format!("{}", err),
						};

						send!(parent_sender, AppMsg::ShowError(format!("Failed to open database: {}", message)));
					},
				}
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<OpenDatabaseModel, AppModel> for OpenDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_spacing: 5,
				set_hexpand: true,

				append = &gtk::Label {
					set_label: "Password:",
				},
				append = &gtk::Entry {
					set_buffer: &model.password_entry,
					set_hexpand: true,
					set_visibility: false,
					set_input_purpose: gtk::InputPurpose::Password,
					connect_activate(sender) => move |_| {
						send!(sender, OpenDatabaseMsg::OpenClicked);
					},
				},
			},

			append = &gtk::Button {
				set_label: "Open",
				set_margin_start: 40,
				set_margin_end: 40,
				set_margin_top: 40,
				connect_clicked(sender) => move |_| {
					send!(sender, OpenDatabaseMsg::OpenClicked);
				},
			},
		}
	}
}
