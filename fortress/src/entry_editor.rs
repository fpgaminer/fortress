use libfortress::ID;
use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, MicroComponent, Model, RelmComponent, Sender, WidgetPlus, Widgets};
use relm4_components::ParentWindow;

use crate::{
	dialog_modal::{DialogConfig, DialogModel, DialogMsg},
	generate_popover::{GeneratePopoverModel, GeneratePopoverParent},
	password_entry::PasswordEntryModel,
	AppModel, AppMsg,
};


#[derive(Clone)]
pub enum EntryEditorMsg {
	// From parent
	NewEntry { parent: ID },
	EditEntry(libfortress::Entry),
	PasswordGenerated(String),

	// From UI
	Cancel,
	Save,
	EntryChanged,

	// Internal
	ForceCancel,

	// From dialog
	CloseDialog,
}

enum Mode {
	// Parent directory
	New(ID),
	// The entry being editted
	Edit(libfortress::Entry),
	None,
}

pub struct EntryEditorModel {
	mode: Mode,

	title_entry: gtk::EntryBuffer,
	username_entry: gtk::EntryBuffer,
	password: MicroComponent<PasswordEntryModel>,
	url_entry: gtk::EntryBuffer,
	notes_entry: gtk::TextBuffer,

	modified: bool,
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
			mode: Mode::None,

			title_entry: gtk::EntryBuffer::new(None),
			username_entry: gtk::EntryBuffer::new(None),
			password: MicroComponent::new(PasswordEntryModel::new(), ()),
			url_entry: gtk::EntryBuffer::new(None),
			notes_entry,

			modified: false,
		}
	}

	fn update(&mut self, msg: EntryEditorMsg, components: &EntryEditorComponents, sender: Sender<EntryEditorMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			EntryEditorMsg::NewEntry { parent } => {
				// TODO: Should this reflect the current state of the generate dialog? Or maybe some kind of setting? Or at least break this out to global constants.
				self.password
					.model_mut()
					.unwrap()
					.buffer
					.set_text(&libfortress::random_string(20, true, true, true, ""));
				self.modified = false;
				self.mode = Mode::New(parent);
			},
			EntryEditorMsg::EditEntry(entry) => {
				self.title_entry.set_text(entry.get("title").map(|s| s.as_str()).unwrap_or_else(|| ""));
				self.username_entry.set_text(entry.get("username").map(|s| s.as_str()).unwrap_or_else(|| ""));
				self.password
					.model_mut()
					.unwrap()
					.buffer
					.set_text(entry.get("password").map(|s| s.as_str()).unwrap_or(""));
				self.url_entry.set_text(entry.get("url").map(|s| s.as_str()).unwrap_or_else(|| ""));
				self.notes_entry.set_text(entry.get("notes").map(|s| s.as_str()).unwrap_or_else(|| ""));
				self.modified = false;

				self.mode = Mode::Edit(entry);
			},
			EntryEditorMsg::Cancel => {
				// If there are unsaved changes, ask the user if they really want to discard them.
				if self.modified {
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
				let title = self.title_entry.text();
				let username = self.username_entry.text();
				let password = self.password.model().unwrap().buffer.text();
				let url = self.url_entry.text();
				let notes = self
					.notes_entry
					.text(&self.notes_entry.start_iter(), &self.notes_entry.end_iter(), false)
					.to_string();

				let data = libfortress::EntryHistory::new(
					[
						("title".to_string(), title),
						("username".to_string(), username),
						("password".to_string(), password),
						("url".to_string(), url),
						("notes".to_string(), notes),
					]
					.iter()
					.cloned()
					.collect(),
				);

				match &self.mode {
					Mode::New(parent) => send!(parent_sender, AppMsg::EntryEditorSavedNew { parent: *parent, data }),
					Mode::Edit(entry) => send!(parent_sender, AppMsg::EntryEditorSavedEdit { id: *entry.get_id(), data }),
					Mode::None => (),
				}

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
				self.password.model_mut().unwrap().buffer.set_text(&password);
			},
			EntryEditorMsg::EntryChanged => {
				self.update_modified();
			},
		}
	}
}

impl EntryEditorModel {
	fn clear(&mut self) {
		self.mode = Mode::None;
		self.title_entry.set_text("");
		self.username_entry.set_text("");
		self.password.model_mut().unwrap().buffer.set_text("");
		self.url_entry.set_text("");
		self.notes_entry.set_text("");
	}

	fn update_modified(&mut self) {
		let title = self.title_entry.text();
		let username = self.username_entry.text();
		let password = self.password.model().unwrap().buffer.text();
		let url = self.url_entry.text();
		let notes = self
			.notes_entry
			.text(&self.notes_entry.start_iter(), &self.notes_entry.end_iter(), false)
			.to_string();

		self.modified = match &self.mode {
			Mode::Edit(entry) => {
				Some(&title) != entry.get("title")
					|| Some(&username) != entry.get("username")
					|| Some(&password) != entry.get("password")
					|| Some(&url) != entry.get("url")
					|| Some(&notes) != entry.get("notes")
			},
			Mode::New(_) => true,
			Mode::None => false,
		};
	}
}

impl GeneratePopoverParent<EntryEditorModel> for EntryEditorModel {
	fn password_generated_msg(password: String) -> <EntryEditorModel as Model>::Msg {
		EntryEditorMsg::PasswordGenerated(password)
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

			append = &gtk::CenterBox {
				set_margin_all: 10,

				set_start_widget = Some(&gtk::Box) {
					set_orientation: gtk::Orientation::Horizontal,
					set_spacing: 10,

					append: back_btn = &gtk::Button {
						set_icon_name: watch!(if model.modified { "window-close-symbolic" } else { "go-previous-symbolic" }),

						connect_clicked(sender) => move |_| {
							send!(sender, EntryEditorMsg::Cancel);
						},
					},

					append: save_btn = &gtk::Button {
						// TODO: Better icon
						set_icon_name: "document-save-symbolic",
						set_visible: watch!(model.modified),

						connect_clicked(sender) => move |_| {
							send!(sender, EntryEditorMsg::Save);
						},
					},
				},

				set_center_widget = Some(&gtk::Label) {
					set_text: "Entry",
					add_css_class: "h1",
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Vertical,
				set_spacing: 5,
				set_margin_start: 10,
				set_margin_end: 10,
				set_margin_top: 10,

				append = &gtk::Label {
					set_label: "Title",
					set_halign: gtk::Align::Start,
				},
				append = &gtk::Entry {
					set_buffer: &model.title_entry,
					connect_changed(sender) => move |_| send!(sender, EntryEditorMsg::EntryChanged),
				},

				append = &gtk::Label {
					set_label: "Username",
					set_halign: gtk::Align::Start,
					set_margin_top: 5,
				},
				append = &gtk::Entry {
					set_buffer: &model.username_entry,
					connect_changed(sender) => move |_| send!(sender, EntryEditorMsg::EntryChanged),
				},

				append = &gtk::Label {
					set_label: "Password",
					set_halign: gtk::Align::Start,
					set_margin_top: 5,
				},
				append = &gtk::Box {
					set_orientation: gtk::Orientation::Horizontal,
					append: model.password.root_widget(),
					append = &gtk::MenuButton {
						set_label: "Generate",
						set_direction: gtk::ArrowType::Down,
						set_popover: generate_popover = Some(&gtk::Popover) {
							set_position: gtk::PositionType::Right,

							set_child: Some(components.generate_popover.root_widget())
						}
					},
				},

				append = &gtk::Label {
					set_label: "URL",
					set_halign: gtk::Align::Start,
					set_margin_top: 5,
				},
				append = &gtk::Entry {
					set_buffer: &model.url_entry,
					set_input_purpose: gtk::InputPurpose::Url,
					connect_changed(sender) => move |_| send!(sender, EntryEditorMsg::EntryChanged),
				},

				append = &gtk::Label {
					set_label: "Notes",
					set_halign: gtk::Align::Start,
					set_margin_top: 5,
				},
				append = &gtk::TextView {
					set_buffer: Some(&model.notes_entry),
					set_vexpand: true,
				},
			},
		}
	}

	fn post_init() {
		let main_window = None;

		let cloned_sender = sender.clone();
		model.notes_entry.connect_changed(move |_| {
			send!(cloned_sender, EntryEditorMsg::EntryChanged);
		});

		let cloned_generate_popover = generate_popover.clone();
		model.password.root_widget().connect_changed(move |_| {
			// Hide the generate popover (useful for after the Generate button is clicked)
			cloned_generate_popover.popdown();
			send!(sender, EntryEditorMsg::EntryChanged);
		});
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
	generate_popover: RelmComponent<GeneratePopoverModel, EntryEditorModel>,
}
