use std::path::PathBuf;

use libfortress::Database;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum CreateDatabaseMsg {
	CreateClicked,
}

pub struct CreateDatabaseModel {
	database_path: PathBuf,

	username_entry: gtk::EntryBuffer,
	password_entry: gtk::EntryBuffer,
	repeat_password_entry: gtk::EntryBuffer,
}

impl Model for CreateDatabaseModel {
	type Msg = CreateDatabaseMsg;
	type Widgets = CreateDatabaseWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for CreateDatabaseModel {
	fn init_model(parent_model: &AppModel) -> Self {
		Self {
			database_path: parent_model.database_path.clone(),

			username_entry: gtk::EntryBuffer::new(None),
			password_entry: gtk::EntryBuffer::new(None),
			repeat_password_entry: gtk::EntryBuffer::new(None),
		}
	}

	fn update(&mut self, msg: CreateDatabaseMsg, _components: &(), _sender: Sender<CreateDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			CreateDatabaseMsg::CreateClicked => {
				let username = self.username_entry.text();
				let password = self.password_entry.text();
				let repeat_password = self.repeat_password_entry.text();

				if password != repeat_password {
					send!(parent_sender, AppMsg::ShowError("Passwords do not match".to_string()));
					return;
				}

				let database = Database::new_with_password(username, password);
				if let Err(err) = database.save_to_path(&self.database_path) {
					send!(parent_sender, AppMsg::ShowError(format!("Failed to create database: {}", err)));
					return;
				}

				send!(parent_sender, AppMsg::DatabaseCreated(database));

				self.username_entry.set_text("");
				self.password_entry.set_text("");
				self.repeat_password_entry.set_text("");
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<CreateDatabaseModel, AppModel> for CreateDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_spacing: 5,
				set_hexpand: true,

				append = &gtk::Label {
					set_label: "Username:",
				},
				append = &gtk::Entry {
					set_buffer: &model.username_entry,
					set_hexpand: true,
				},
			},

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
						send!(sender, CreateDatabaseMsg::CreateClicked);
					},
				},
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_spacing: 5,
				set_hexpand: true,

				append = &gtk::Label {
					set_label: "Repeat password:",
				},
				append = &gtk::Entry {
					set_buffer: &model.repeat_password_entry,
					set_hexpand: true,
					set_visibility: false,
					set_input_purpose: gtk::InputPurpose::Password,
					connect_activate(sender) => move |_| {
						send!(sender, CreateDatabaseMsg::CreateClicked);
					},
				},
			},

			append = &gtk::Button {
				set_label: "Create",
				set_margin_start: 40,
				set_margin_end: 40,
				set_margin_top: 40,
				connect_clicked(sender) => move |_| {
					send!(sender, CreateDatabaseMsg::CreateClicked);
				},
			},
		}
	}
}
