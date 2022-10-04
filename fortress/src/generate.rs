use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum GenerateMsg {
	CancelClicked,
	GenerateClicked,
	PasswordLengthChanged(i32),
	UppercaseChanged(bool),
	LowercaseChanged(bool),
	NumbersChanged(bool),
}

pub struct GenerateModel {
	password_length: i32,
	uppercase: bool,
	lowercase: bool,
	numbers: bool,
	other_entry: gtk::EntryBuffer,
}

impl Model for GenerateModel {
	type Msg = GenerateMsg;
	type Widgets = GenerateWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for GenerateModel {
	fn init_model(_parent_model: &AppModel) -> Self {
		Self {
			password_length: 20,
			uppercase: true,
			lowercase: true,
			numbers: true,
			other_entry: gtk::EntryBuffer::new(None),
		}
	}

	fn update(&mut self, msg: GenerateMsg, _components: &(), _sender: Sender<GenerateMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			GenerateMsg::CancelClicked => {
				send!(parent_sender, AppMsg::PasswordGenerated(None));
			},
			GenerateMsg::GenerateClicked => {
				let other_chars = self.other_entry.text();

				if !self.lowercase && !self.uppercase && !self.numbers && other_chars.is_empty() {
					// TODO: Display an error
					return;
				}

				let password = libfortress::random_string(self.password_length as usize, self.uppercase, self.lowercase, self.numbers, &other_chars);
				send!(parent_sender, AppMsg::PasswordGenerated(Some(password)));
			},
			GenerateMsg::PasswordLengthChanged(length) => {
				self.password_length = length;
			},
			GenerateMsg::UppercaseChanged(uppercase) => {
				self.uppercase = uppercase;
			},
			GenerateMsg::LowercaseChanged(lowercase) => {
				self.lowercase = lowercase;
			},
			GenerateMsg::NumbersChanged(numbers) => {
				self.numbers = numbers;
			},
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<GenerateModel, AppModel> for GenerateWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Label {
				set_text: "Number of characters:",
			},

			append: foobar = &gtk::SpinButton::with_range(1.0, 1000000.0, 1.0) {
				set_numeric: true,
				set_value: watch!(model.password_length as f64),
				connect_value_changed(sender) => move |spin_button| {
					send!(sender, GenerateMsg::PasswordLengthChanged(spin_button.value_as_int()));
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
			},

			append = &gtk::CheckButton {
				set_label: Some("Uppercase letters (A-Z)"),
				set_active: watch!(model.uppercase),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GenerateMsg::UppercaseChanged(check_button.is_active()));
				},
			},

			append = &gtk::CheckButton {
				set_label: Some("Lowercase letters (a-z)"),
				set_active: watch!(model.lowercase),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GenerateMsg::LowercaseChanged(check_button.is_active()));
				},
			},

			append = &gtk::CheckButton {
				set_label: Some("Numbers (0-9)"),
				set_active: watch!(model.numbers),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GenerateMsg::NumbersChanged(check_button.is_active()));
				},
			},

			append = &gtk::Label {
				set_label: "Other:",
			},
			append = &gtk::Entry {
				set_buffer: &model.other_entry,
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_halign: gtk::Align::End,

				append = &gtk::Button {
					set_label: "Cancel",
					connect_clicked(sender) => move |_| {
						send!(sender, GenerateMsg::CancelClicked);
					},
				},

				append = &gtk::Button {
					set_label: "Generate",
					connect_clicked(sender) => move |_| {
						send!(sender, GenerateMsg::GenerateClicked);
					},
				},
			},
		}
	}
}
