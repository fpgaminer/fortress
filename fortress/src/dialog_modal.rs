use gtk::builders::MessageDialogBuilder;
use relm4::{
	gtk,
	gtk::traits::{DialogExt, GtkWindowExt, WidgetExt},
	send, ComponentUpdate, Model, Sender, Widgets,
};
use relm4_components::ParentWindow;


pub struct DialogConfig<ParentModel: Model> {
	pub title: String,
	pub text: String,

	pub buttons: Vec<(String, ParentModel::Msg)>,
}


pub struct DialogModel<ParentModel: Model> {
	config: Option<DialogConfig<ParentModel>>,
}

pub enum DialogMsg<ParentModel: Model> {
	Show(DialogConfig<ParentModel>),
	Hide,
	#[doc(hidden)]
	Response(ParentModel::Msg),
}

impl<ParentModel: Model + 'static> Model for DialogModel<ParentModel> {
	type Msg = DialogMsg<ParentModel>;
	type Widgets = DialogWidgets;
	type Components = ();
}

impl<ParentModel> ComponentUpdate<ParentModel> for DialogModel<ParentModel>
where
	ParentModel: Model + 'static,
	ParentModel::Widgets: ParentWindow,
{
	fn init_model(_parent_model: &ParentModel) -> Self {
		Self { config: None }
	}

	fn update(&mut self, msg: DialogMsg<ParentModel>, _components: &(), _sender: Sender<DialogMsg<ParentModel>>, parent_sender: Sender<ParentModel::Msg>) {
		match msg {
			DialogMsg::Show(config) => {
				self.config = Some(config);
			},
			DialogMsg::Hide => {
				self.config = None;
			},
			DialogMsg::Response(msg) => {
				send!(parent_sender, msg);
			},
		}
	}
}


pub struct DialogWidgets {
	dialog: gtk::MessageDialog,
}

impl<ParentModel> Widgets<DialogModel<ParentModel>, ParentModel> for DialogWidgets
where
	ParentModel: Model + 'static,
	ParentModel::Widgets: ParentWindow,
	ParentModel::Msg: Clone,
{
	type Root = gtk::MessageDialog;

	fn init_view(_model: &DialogModel<ParentModel>, _components: &(), _sender: Sender<DialogMsg<ParentModel>>) -> Self {
		let dialog = MessageDialogBuilder::new().visible(false).build();

		Self { dialog }
	}

	fn root_widget(&self) -> Self::Root {
		self.dialog.clone()
	}

	fn connect_parent(&mut self, parent_widgets: &<ParentModel as ::relm4::Model>::Widgets) {
		self.dialog.set_transient_for(parent_widgets.parent_window().as_ref());
	}

	fn view(&mut self, model: &DialogModel<ParentModel>, sender: Sender<DialogMsg<ParentModel>>) {
		self.dialog.set_visible(false);

		let parent = self.dialog.transient_for();

		if let Some(config) = &model.config {
			let builder = MessageDialogBuilder::new()
				.message_type(gtk::MessageType::Error)
				.visible(false)
				.text(&config.title)
				.secondary_text(&config.text)
				.modal(true);

			let builder = if let Some(parent) = parent { builder.transient_for(&parent) } else { builder };

			self.dialog = builder.build();

			for (idx, (text, _)) in config.buttons.iter().enumerate() {
				self.dialog.add_button(text, gtk::ResponseType::Other(idx as u16));
			}

			let response_messages = config.buttons.iter().map(|(_, msg)| msg.clone()).collect::<Vec<_>>();
			self.dialog.connect_response(move |_, response| {
				if let gtk::ResponseType::Other(idx) = response {
					let msg = response_messages[idx as usize].clone();
					send!(sender, DialogMsg::Response(msg));
				}
			});

			self.dialog.show();
		}
	}
}
