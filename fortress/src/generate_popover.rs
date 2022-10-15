use relm4::{gtk, gtk::prelude::*, send, ComponentUpdate, Model, Sender, Widgets};


#[derive(Clone)]
pub enum GeneratePopoverMsg {
	GenerateClicked,
	LengthChanged(i32),
	UppercaseChanged(bool),
	LowercaseChanged(bool),
	NumbersChanged(bool),
}

pub struct GeneratePopoverModel {
	password_length: i32,
	uppercase: bool,
	lowercase: bool,
	numbers: bool,
	other_entry: gtk::EntryBuffer,
}

impl Model for GeneratePopoverModel {
	type Msg = GeneratePopoverMsg;
	type Widgets = GeneratePopoverWidgets;
	type Components = ();
}

impl<ParentModel> ComponentUpdate<ParentModel> for GeneratePopoverModel
where
	ParentModel: Model,
	ParentModel: GeneratePopoverParent<ParentModel>,
{
	fn init_model(_parent_model: &ParentModel) -> Self {
		Self {
			password_length: 20,
			uppercase: true,
			lowercase: true,
			numbers: true,
			other_entry: gtk::EntryBuffer::new(None),
		}
	}

	fn update(&mut self, msg: Self::Msg, _components: &(), _sender: Sender<Self::Msg>, parent_sender: Sender<ParentModel::Msg>) {
		match msg {
			GeneratePopoverMsg::GenerateClicked => {
				let other_chars = self.other_entry.text();

				if !self.lowercase && !self.uppercase && !self.numbers && other_chars.is_empty() {
					// TODO: Display an error
					return;
				}

				let password = libfortress::random_string(self.password_length as usize, self.uppercase, self.lowercase, self.numbers, &other_chars);
				send!(parent_sender, ParentModel::password_generated_msg(password));
			},
			GeneratePopoverMsg::LengthChanged(length) => {
				self.password_length = length;
			},
			GeneratePopoverMsg::UppercaseChanged(uppercase) => {
				self.uppercase = uppercase;
			},
			GeneratePopoverMsg::LowercaseChanged(lowercase) => {
				self.lowercase = lowercase;
			},
			GeneratePopoverMsg::NumbersChanged(numbers) => {
				self.numbers = numbers;
			},
		}
	}
}


#[relm4::widget(pub)]
impl<ParentModel> Widgets<GeneratePopoverModel, ParentModel> for GeneratePopoverWidgets
where
	ParentModel: Model,
{
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,
			set_spacing: 5,
			add_css_class: "generate-popover",

			append = &gtk::Label {
				set_text: "Length",
				set_halign: gtk::Align::Start,
			},

			append = &gtk::SpinButton::with_range(1.0, 1000000.0, 1.0) {
				set_numeric: true,
				set_value: watch!(model.password_length as f64),
				connect_value_changed(sender) => move |spin_button| {
					send!(sender, GeneratePopoverMsg::LengthChanged(spin_button.value_as_int()));
				},
			},

			append = &gtk::CheckButton {
				set_label: Some("Uppercase (A-Z)"),
				set_active: watch!(model.uppercase),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GeneratePopoverMsg::UppercaseChanged(check_button.is_active()));
				},
			},

			append = &gtk::CheckButton {
				set_label: Some("(a-z)"),
				set_active: watch!(model.lowercase),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GeneratePopoverMsg::LowercaseChanged(check_button.is_active()));
				},
			},

			append = &gtk::CheckButton {
				set_label: Some("(0-9)"),
				set_active: watch!(model.numbers),
				connect_toggled(sender) => move |check_button| {
					send!(sender, GeneratePopoverMsg::NumbersChanged(check_button.is_active()));
				},
			},

			append = &gtk::Label {
				set_label: "Other",
				set_halign: gtk::Align::Start,
			},
			append = &gtk::Entry {
				set_buffer: &model.other_entry,
			},

			append = &gtk::Button {
				set_label: "Generate",
				connect_clicked(sender) => move |_| {
					send!(sender, GeneratePopoverMsg::GenerateClicked);
				},
			},
		}
	}
}


pub trait GeneratePopoverParent<ParentModel: Model> {
	fn password_generated_msg(password: String) -> ParentModel::Msg;
}
