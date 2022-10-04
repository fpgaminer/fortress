use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, RelmComponent, Sender, Widgets};
use relm4_components::ParentWindow;

use crate::{
	dialog_modal::{DialogConfig, DialogModel, DialogMsg},
	AppModel, AppMsg,
};


#[derive(Clone)]
pub enum EntryEditorMsg {
	// From parent
	NewEntry,
	EditEntry(libfortress::Entry),
	PasswordGenerated(String),

	// From UI
	GenerateClicked,
	Cancel,
	Save,

	// Internal
	ForceCancel,

	// From dialog
	CloseDialog,
}

pub struct EntryEditorModel {
	// The entry being edited.
	entry: Option<libfortress::Entry>,

	title_entry: gtk::EntryBuffer,
	username_entry: gtk::EntryBuffer,
	password_entry: gtk::EntryBuffer,
	url_entry: gtk::EntryBuffer,
	notes_entry: gtk::TextBuffer,
}

impl Model for EntryEditorModel {
	type Msg = EntryEditorMsg;
	type Widgets = EntryEditorWidgets;
	type Components = EntryEditorComponents;
}

impl ComponentUpdate<AppModel> for EntryEditorModel {
	fn init_model(_parent_model: &AppModel) -> Self {
		let notes_entry = gtk::TextBuffer::new(None);
		notes_entry.set_enable_undo(true);

		EntryEditorModel {
			entry: None,

			title_entry: gtk::EntryBuffer::new(None),
			username_entry: gtk::EntryBuffer::new(None),
			password_entry: gtk::EntryBuffer::new(None),
			url_entry: gtk::EntryBuffer::new(None),
			notes_entry,
		}
	}

	fn update(&mut self, msg: EntryEditorMsg, components: &EntryEditorComponents, sender: Sender<EntryEditorMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			EntryEditorMsg::NewEntry => {},
			EntryEditorMsg::EditEntry(entry) => {
				self.title_entry.set_text(entry.get("title").map(|s| s.as_str()).unwrap_or(""));
				self.username_entry.set_text(entry.get("username").map(|s| s.as_str()).unwrap_or(""));
				self.password_entry.set_text(entry.get("password").map(|s| s.as_str()).unwrap_or(""));
				self.url_entry.set_text(entry.get("url").map(|s| s.as_str()).unwrap_or(""));
				self.notes_entry.set_text(entry.get("notes").map(|s| s.as_str()).unwrap_or(""));

				self.entry = Some(entry);
			},
			EntryEditorMsg::Cancel => {
				// See if there are any unsaved changes.
				let title = self.title_entry.text();
				let username = self.username_entry.text();
				let password = self.password_entry.text();
				let url = self.url_entry.text();
				let notes = self
					.notes_entry
					.text(&self.notes_entry.start_iter(), &self.notes_entry.end_iter(), false)
					.to_string();

				let changes = if let Some(entry) = &self.entry {
					entry.get("title").map(|s| s.as_str()).unwrap_or("") != title
						|| entry.get("username").map(|s| s.as_str()).unwrap_or("") != username
						|| entry.get("password").map(|s| s.as_str()).unwrap_or("") != password
						|| entry.get("url").map(|s| s.as_str()).unwrap_or("") != url
						|| entry.get("notes").map(|s| s.as_str()).unwrap_or("") != notes
				} else {
					!title.is_empty() || !username.is_empty() || !password.is_empty() || !url.is_empty() || !notes.is_empty()
				};

				// If there are unsaved changes, ask the user if they really want to discard them.
				if changes {
					components
						.dialog
						.send(DialogMsg::Show(DialogConfig {
							title: "Discard changes?".to_string(),
							text: "Are you sure you want to discard your changes?".to_string(),
							buttons: vec![
								("Discard".to_owned(), EntryEditorMsg::ForceCancel),
								("Cancel".to_owned(), EntryEditorMsg::CloseDialog),
							],
						}))
						.unwrap();
				} else {
					send!(sender, EntryEditorMsg::ForceCancel);
				}
			},
			EntryEditorMsg::Save => {
				let data = libfortress::EntryHistory::new(
					[
						("title".to_string(), self.title_entry.text()),
						("username".to_string(), self.username_entry.text()),
						("password".to_string(), self.password_entry.text()),
						("url".to_string(), self.url_entry.text()),
						(
							"notes".to_string(),
							self.notes_entry
								.text(&self.notes_entry.start_iter(), &self.notes_entry.end_iter(), false)
								.to_string(),
						),
					]
					.iter()
					.cloned()
					.collect(),
				);

				send!(
					parent_sender,
					AppMsg::EntryEditorSaved {
						id: self.entry.as_ref().map(|e| *e.get_id()),
						data,
					}
				);

				self.clear();
			},
			EntryEditorMsg::ForceCancel => {
				components.dialog.send(DialogMsg::Hide).unwrap();
				self.clear();
				send!(parent_sender, AppMsg::EntryEditorClosed);
			},
			EntryEditorMsg::CloseDialog => {
				components.dialog.send(DialogMsg::Hide).unwrap();
			},
			EntryEditorMsg::PasswordGenerated(password) => {
				self.password_entry.set_text(&password);
			},
			EntryEditorMsg::GenerateClicked => {
				send!(parent_sender, AppMsg::GeneratePassword);
			},
		}
	}
}

impl EntryEditorModel {
	fn clear(&mut self) {
		self.entry = None;
		self.title_entry.set_text("");
		self.username_entry.set_text("");
		self.password_entry.set_text("");
		self.url_entry.set_text("");
		self.notes_entry.set_text("");
	}
}

#[relm4::widget(pub)]
impl Widgets<EntryEditorModel, AppModel> for EntryEditorWidgets {
	additional_fields! {
		main_window: Option<gtk::Window>,
	}

	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Label {
				set_label: "Title",
			},
			append = &gtk::Entry {
				set_buffer: &model.title_entry,
			},

			append = &gtk::Label {
				set_label: "Username",
			},
			append = &gtk::Entry {
				set_buffer: &model.username_entry,
			},

			append = &gtk::Label {
				set_label: "Password",
			},
			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				append = &gtk::Entry {
					set_buffer: &model.password_entry,
					set_input_purpose: gtk::InputPurpose::Password,
					set_hexpand: true,
				},
				append = &gtk::Button {
					set_label: "Generate",
					connect_clicked(sender) => move |_| {
						send!(sender, EntryEditorMsg::GenerateClicked);
					},
				},
			},

			append = &gtk::Label {
				set_label: "URL",
			},
			append = &gtk::Entry {
				set_buffer: &model.url_entry,
				set_input_purpose: gtk::InputPurpose::Url,
			},

			append = &gtk::Label {
				set_label: "Notes",
			},
			append = &gtk::TextView {
				set_buffer: Some(&model.notes_entry),
				set_vexpand: true,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				append = &gtk::Button {
					set_label: "Cancel",
					connect_clicked(sender) => move |_| {
						send!(sender, EntryEditorMsg::Cancel);
					},
				},
				append = &gtk::Button {
					set_label: "Save",
					connect_clicked(sender) => move |_| {
						send!(sender, EntryEditorMsg::Save);
					},
				},
			},
		}
	}

	fn post_init() {
		let main_window = None;
	}

	fn pre_connect_parent() {
		self.main_window = parent_widgets.parent_window();
	}
}


impl ParentWindow for EntryEditorWidgets {
	fn parent_window(&self) -> Option<gtk::Window> {
		self.main_window.clone()
	}
}


#[derive(relm4::Components)]
pub struct EntryEditorComponents {
	dialog: RelmComponent<DialogModel<EntryEditorModel>, EntryEditorModel>,
}
