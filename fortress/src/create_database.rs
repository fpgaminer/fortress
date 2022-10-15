use std::path::PathBuf;

use libfortress::Database;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum CreateDatabaseMsg {
	CreateClicked {
		username: String,
		password: String,
		repeat_password: String,
	},
}

pub struct CreateDatabaseModel {
	database_path: PathBuf,
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
		}
	}

	fn update(&mut self, msg: CreateDatabaseMsg, _components: &(), _sender: Sender<CreateDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			CreateDatabaseMsg::CreateClicked {
				username,
				password,
				repeat_password,
			} => {
				if password != repeat_password {
					send!(parent_sender, AppMsg::ShowError("Passwords do not match".to_string()));
					return;
				}

				let mut database = Database::new_with_password(username, password);

				database.get_root_mut().rename("My Passwords");

				if let Err(err) = database.save_to_path(&self.database_path) {
					send!(parent_sender, AppMsg::ShowError(format!("Failed to create database: {}", err)));
					return;
				}


				send!(parent_sender, AppMsg::DatabaseCreated(database));
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<CreateDatabaseModel, AppModel> for CreateDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,
			set_halign: gtk::Align::Center,
			set_valign: gtk::Align::Center,
			add_css_class: "create-database",

			append = &gtk::Label {
				set_label: "Create Fortress",
				set_halign: gtk::Align::Center,
				add_css_class: "h1",
			},

			append = &gtk::Label {
				set_label: "Enter your username and password to create your Fortress.",
				set_halign: gtk::Align::Start,
				add_css_class: "h-sub",
			},

			append: username = &gtk::Entry {
				set_hexpand: true,
				set_placeholder_text: Some("Username"),
			},

			append: password = &gtk::PasswordEntry {
				set_halign: gtk::Align::Fill,
				set_hexpand: true,
				set_show_peek_icon: true,
				set_placeholder_text: Some("Password"),

				connect_unmap => move |entry| {
					entry.set_text("");
				},
			},

			append: repeat_password = &gtk::PasswordEntry {
				set_halign: gtk::Align::Fill,
				set_hexpand: true,
				set_show_peek_icon: true,
				set_placeholder_text: Some("Repeat your password"),

				connect_unmap => move |entry| {
					entry.set_text("");
				},
			},

			append = &gtk::Button {
				set_label: "Create",
				set_halign: gtk::Align::Fill,
				add_css_class: "action-btn",
				set_hexpand: true,
				connect_clicked(username, password, repeat_password, sender) => move |_| {
					send!(sender, CreateDatabaseMsg::CreateClicked {
						username: username.text().to_string(),
						password: password.text().to_string(),
						repeat_password: repeat_password.text().to_string(),
					});
				},
			},
		}
	}
}
