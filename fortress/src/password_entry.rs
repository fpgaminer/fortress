use relm4::{
	gtk::{
		self,
		traits::{EntryExt, WidgetExt},
	},
	send, MicroModel, MicroWidgets, Sender,
};

pub struct PasswordEntryModel {
	// TODO: Use PasswordEntryBuffer
	pub buffer: gtk::EntryBuffer,
	pub visibility: bool,
}

pub enum PasswordEntryMsg {
	Toggle,
	Hide,
}

impl MicroModel for PasswordEntryModel {
	type Msg = PasswordEntryMsg;
	type Widgets = PasswordEntryWidgets;
	type Data = ();

	fn update(&mut self, msg: Self::Msg, _data: &(), _sender: Sender<Self::Msg>) {
		match msg {
			PasswordEntryMsg::Toggle => {
				self.visibility = !self.visibility;
			},
			PasswordEntryMsg::Hide => {
				self.visibility = false;
			},
		}
	}
}

impl PasswordEntryModel {
	pub fn new() -> Self {
		Self {
			buffer: gtk::EntryBuffer::new(None),
			visibility: false,
		}
	}
}

#[relm4::micro_widget(pub)]
#[derive(Debug)]
impl MicroWidgets<PasswordEntryModel> for PasswordEntryWidgets {
	view! {
		gtk::Entry {
			set_buffer: &model.buffer,
			set_input_purpose: gtk::InputPurpose::Password,
			set_visibility: watch!(model.visibility),
			set_hexpand: true,
			set_secondary_icon_name: watch!(Some(if model.visibility { "view-conceal-symbolic" } else { "view-reveal-symbolic" })),
			add_css_class: "password",

			connect_icon_release(sender) => move |_,_| {
				send!(sender, PasswordEntryMsg::Toggle);
			},

			connect_unmap(sender) => move |_| {
				send!(sender, PasswordEntryMsg::Hide);
			},
		}
	}
}
