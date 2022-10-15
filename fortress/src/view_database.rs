use std::{
	cell::{Ref, RefCell, RefMut},
	path::PathBuf,
	rc::Rc,
};

use libfortress::{Database, Directory, ID};
use relm4::{
	actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup},
	factory::{FactoryPrototype, FactoryVec},
	gtk,
	gtk::{builders::GestureClickBuilder, gdk, gio, pango, prelude::*, ListStore, TreeModelFilter},
	send, ComponentUpdate, Model, Sender, Widgets,
};

use crate::{AppModel, AppMsg};


#[derive(Clone)]
pub enum ViewDatabaseMsg {
	SettingsClicked,
	NewEntryClicked,
	NewDirectoryClicked,
	ViewEntry(ID),
	Refresh(gtk::Entry),
	Refilter,
	DirectoryEdited(usize, String),
	DirectoryEditCancelled(usize),
	FocusWidget(gtk::Widget),
	MoveEntry { entry_id: ID, directory_id: ID },
	DirectorySelected(Option<i32>),
	CopyUsername { clipboard: gdk::Clipboard, id: ID },
	CopyPassword { clipboard: gdk::Clipboard, id: ID },
}

pub struct ViewDatabaseModel {
	database: Rc<RefCell<Option<Database>>>,
	database_path: PathBuf,

	entries: ListStore,
	entries_model: TreeModelFilter,
	search_entry: gtk::EntryBuffer,
	entries_menu: gio::Menu,
	entries_move_submenu: gio::Menu,

	directories: FactoryVec<DirectoryItem>,
	selected_directory: Option<ID>,
}

impl Model for ViewDatabaseModel {
	type Msg = ViewDatabaseMsg;
	type Widgets = ViewDatabaseWidgets;
	type Components = ();
}

impl ComponentUpdate<AppModel> for ViewDatabaseModel {
	fn init_model(parent_model: &AppModel) -> Self {
		let entries = ListStore::new(&[
			gtk::glib::Bytes::static_type(),
			String::static_type(),
			String::static_type(),
			String::static_type(),
		]);
		let entries_model = TreeModelFilter::new(&entries, None);
		let search_entry = gtk::EntryBuffer::new(None);

		// TODO: Fuzzy search
		let search_entry_clone = search_entry.clone();
		entries_model.set_visible_func(move |model, iter| {
			let title = model.get_value(iter, 1).get::<String>().unwrap().to_lowercase();
			let search_string = search_entry_clone.text().to_lowercase();

			if !search_string.is_empty() {
				title.contains(&search_string)
			} else {
				true
			}
		});

		let entries_menu = gio::Menu::new();
		let entries_move_submenu = gio::Menu::new();
		entries_menu.append(Some("Copy Username"), Some(&CopyUsernameAction::action_name()));
		entries_menu.append(Some("Copy Password"), Some(&CopyPasswordAction::action_name()));
		entries_menu.append_submenu(Some("Move"), &entries_move_submenu);

		Self {
			database: parent_model.database.clone(),
			database_path: parent_model.database_path.clone(),

			entries,
			entries_model,
			search_entry,
			entries_menu,
			entries_move_submenu,

			directories: FactoryVec::new(),
			selected_directory: None,
		}
	}

	fn update(&mut self, msg: ViewDatabaseMsg, _components: &(), _sender: Sender<ViewDatabaseMsg>, parent_sender: Sender<AppMsg>) {
		match msg {
			ViewDatabaseMsg::SettingsClicked => {
				send!(parent_sender, AppMsg::ShowMenu);
			},
			ViewDatabaseMsg::NewEntryClicked => {
				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				let parent = self.selected_directory.unwrap_or_else(|| *database.get_root().get_id());

				send!(parent_sender, AppMsg::NewEntry { parent });
			},
			ViewDatabaseMsg::NewDirectoryClicked => {
				// Safety check to make sure we aren't already in progress on a new directory.
				if self.directories.iter().last().filter(|d| *d == &DirectoryItem::New).is_none() {
					self.directories.push(DirectoryItem::New);
				}
			},
			ViewDatabaseMsg::ViewEntry(id) => {
				send!(parent_sender, AppMsg::EditEntry(id));
			},
			ViewDatabaseMsg::Refresh(search_box) => {
				self.refresh_entries();
				self.refresh_directories();
				search_box.grab_focus();
			},
			ViewDatabaseMsg::Refilter => {
				self.entries_model.refilter();
			},
			ViewDatabaseMsg::DirectoryEdited(index, new_name) => {
				// Scope control
				{
					// Update the database
					let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
						Ok(database) => database,
						Err(_) => return,
					};

					match self.directories.get_mut(index) {
						Some(DirectoryItem::Directory { id, name }) => {
							let directory = database.get_directory_by_id_mut(id).expect("Internal error: directory not found");
							directory.rename(new_name.clone());

							// Update model
							*name = new_name;
						},
						Some(DirectoryItem::New) => {
							let mut directory = Directory::new();
							directory.rename(new_name);
							database.add_directory(directory);
						},
						Some(DirectoryItem::All) | None => (),
					}

					// Save the database
					if let Err(err) = database.save_to_path(&self.database_path) {
						// TODO: This is a fatal error. We should show a dialog that lets the user retry or quit.
						send!(parent_sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
						return;
					}
				}

				self.refresh_directories();
			},
			ViewDatabaseMsg::DirectoryEditCancelled(index) => {
				if let Some(DirectoryItem::New) = self.directories.get(index) {
					// User cancelled adding new directory.
					self.directories.pop();
				}
			},
			ViewDatabaseMsg::FocusWidget(widget) => {
				widget.grab_focus();
			},
			ViewDatabaseMsg::MoveEntry { entry_id, directory_id } => {
				// Lifetime control
				{
					let mut database = match RefMut::filter_map(self.database.borrow_mut(), |database| database.as_mut()) {
						Ok(database) => database,
						Err(_) => return,
					};

					let old_parent = database.get_parent_directory_mut(&entry_id).map(|d| *d.get_id());

					// Add to new parent first (so the entry isn't dangling during the operation)
					if let Some(parent) = database.get_directory_by_id_mut(&directory_id) {
						parent.add(entry_id);
					}

					// Remove from old parent
					if let Some(parent) = old_parent.and_then(|id| database.get_directory_by_id_mut(&id)) {
						parent.remove(entry_id);
					}

					// Save the database
					if let Err(err) = database.save_to_path(&self.database_path) {
						// TODO: This is a fatal error. We should show a dialog that lets the user retry or quit.
						send!(parent_sender, AppMsg::ShowError(format!("Failed to save database: {}", err)));
						return;
					}
				}

				// Refresh
				self.refresh_entries();
				self.refresh_directories();
			},
			ViewDatabaseMsg::DirectorySelected(index) => {
				let directory_item = index.and_then(|i| self.directories.get(i as usize));

				match directory_item {
					Some(DirectoryItem::All) => self.selected_directory = None,
					Some(DirectoryItem::Directory { id, .. }) => self.selected_directory = Some(*id),
					Some(DirectoryItem::New) | None => (),
				}

				self.refresh_entries();
			},
			#[allow(unused_variables)]
			ViewDatabaseMsg::CopyUsername { clipboard, id } => {
				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				if let Some(username) = database.get_entry_by_id(&id).and_then(|entry| entry.get("username")) {
					#[cfg(target_os = "macos")]
					crate::pasteboard::copy_text_to_pasteboard(username);
					#[cfg(not(target_os = "macos"))]
					clipboard.set_text(username);
				}
			},
			#[allow(unused_variables)]
			ViewDatabaseMsg::CopyPassword { clipboard, id } => {
				let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
					Ok(database) => database,
					Err(_) => return,
				};

				if let Some(password) = database.get_entry_by_id(&id).and_then(|entry| entry.get("password")) {
					#[cfg(target_os = "macos")]
					crate::pasteboard::copy_text_to_pasteboard(password);
					#[cfg(not(target_os = "macos"))]
					clipboard.set_text(password);
				}
			},
		}
	}
}

impl ViewDatabaseModel {
	fn refresh_entries(&mut self) {
		self.entries.clear();

		let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
			Ok(database) => database,
			Err(_) => return,
		};

		let mut entries = if let Some(directory) = self.selected_directory.and_then(|id| database.get_directory_by_id(&id)) {
			directory
				.list_entries(&database)
				.into_iter()
				.map(|id| database.get_entry_by_id(id))
				.collect::<Option<Vec<_>>>()
				.expect("Internal error")
		} else {
			database.list_entries().collect()
		};

		// Sort by time created (and then by ID as a tie breaker)
		entries.sort_by(|a, b| a.get_time_created().cmp(&b.get_time_created()).then(a.get_id().cmp(b.get_id())));

		let getter = |entry: &libfortress::Entry, key: &str| {
			if let Some(value) = entry.get(key) {
				if !value.is_empty() {
					return value.to_owned();
				}
			}

			"-".to_owned()
		};

		for entry in entries {
			let id = gtk::glib::Bytes::from(entry.get_id().as_ref());
			let title = getter(entry, "title");
			let username = getter(entry, "username");
			let url = getter(entry, "url");

			self.entries.insert_with_values(None, &[(0, &id), (1, &title), (2, &username), (3, &url)]);
		}
	}

	fn refresh_directories(&mut self) {
		self.directories.clear();
		self.entries_move_submenu.remove_all();

		let database = match Ref::filter_map(self.database.borrow(), |database| database.as_ref()) {
			Ok(database) => database,
			Err(_) => return,
		};

		let root = database.get_root();
		let directory_name = |dir: &Directory| dir.get_name().map(str::to_string).unwrap_or_else(|| "Unnamed".to_string());

		// root is added later
		let mut directories: Vec<_> = database.list_directories().filter(|d| d.get_id() != root.get_id()).collect();

		// Sort folders by name
		directories.sort_by_key(|a| directory_name(a));

		// Root folder at the top
		directories.insert(0, root);

		// But first the "All" category
		self.directories.push(DirectoryItem::All);

		for directory in directories {
			self.directories.push(DirectoryItem::Directory {
				id: *directory.get_id(),
				name: directory_name(directory),
			});

			self.entries_move_submenu
				.append_item(&RelmAction::<MoveEntryAction>::to_menu_item_with_target_value(
					&directory_name(directory),
					&directory.get_id().as_ref().to_vec(),
				));
		}
	}
}


#[relm4::widget(pub)]
impl Widgets<ViewDatabaseModel, AppModel> for ViewDatabaseWidgets {
	view! {
		gtk::Box {
			set_orientation: gtk::Orientation::Vertical,

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,
				set_margin_top: 10,
				set_margin_bottom: 10,
				set_margin_end: 15,
				set_halign: gtk::Align::End,
				set_spacing: 15,

				append: search_box = &gtk::Entry {
					set_buffer: &model.search_entry,
					set_placeholder_text: Some("Search..."),
					set_primary_icon_name: Some("system-search"),
					set_width_chars: 40,

					connect_changed(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::Refilter);
					},
				},

				append = &gtk::Button {
					set_icon_name: "list-add-symbolic",
					set_tooltip_text: Some("New entry"),

					connect_clicked(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::NewEntryClicked);
					},
				},

				append = &gtk::Button {
					set_icon_name: "folder-new-symbolic",
					set_tooltip_text: Some("New folder"),

					connect_clicked(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::NewDirectoryClicked);
					},
				},

				append = &gtk::Button {
					set_icon_name: "emblem-system-symbolic",
					set_tooltip_text: Some("Settings"),

					connect_clicked(sender) => move |_| {
						send!(sender, ViewDatabaseMsg::SettingsClicked);
					},
				},
			},

			append = &gtk::Separator {
				set_orientation: gtk::Orientation::Horizontal,
			},

			append = &gtk::Box {
				set_orientation: gtk::Orientation::Horizontal,

				append = &gtk::ScrolledWindow {
					set_hscrollbar_policy: gtk::PolicyType::Never,
					set_vexpand: true,
					add_css_class: "directory-list",
					set_child: directory_list = Some(&gtk::ListBox) {
						set_selection_mode: gtk::SelectionMode::Browse,
						factory!(model.directories),

						connect_row_selected(sender) => move |_, row| {
							if let Some(row) = row {
								send!(sender, ViewDatabaseMsg::DirectorySelected(Some(row.index())));
							}
						},
					}
				},

				append = &gtk::Separator {
					set_orientation: gtk::Orientation::Vertical,
					set_vexpand: true,
				},

				append: tree = &gtk::TreeView::with_model(&model.entries_model) {
					set_vexpand: true,
					set_hexpand: true,
					set_grid_lines: gtk::TreeViewGridLines::Horizontal,
					//set_hover_selection: true,  // TODO: I like this, but had trouble getting it working alongside the context menu
					add_css_class: "entry-list",

					connect_row_activated(sender) => move |tree, _, _| {
						let selection = tree.selection();
						if let Some((model, iter)) = selection.selected() {
							let id = model.get::<gtk::glib::Bytes>(&iter, 0);
							let id = ID::from_slice(id.as_ref()).expect("Internal error");

							send!(sender, ViewDatabaseMsg::ViewEntry(id));
						}
					},
				},
			},

			connect_map(sender, search_box) => move |_| {
				send!(sender, ViewDatabaseMsg::Refresh(search_box.clone()));
			},
		}
	}

	fn post_init() {
		// Entry list context menu
		relm4::view! {
			tree_popover = &gtk::PopoverMenu::from_model(Some(&model.entries_menu)) {
				add_css_class: "menu",
				set_position: gtk::PositionType::Bottom,

				insert_before: args!(&tree, None as Option<&gtk::Widget>),
			}
		}

		// Entry list context menu gesture
		let gesture = GestureClickBuilder::new().button(3).build();

		{
			let tree = tree.clone();

			gesture.connect_pressed(move |_, n, x, y| {
				if n != 1 {
					return;
				}

				let (bx, by) = tree.convert_widget_to_bin_window_coords(x as i32, y as i32);

				if let Some((Some(path), _, _, _)) = tree.path_at_pos(bx as i32, by as i32) {
					let selection = tree.selection();
					let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);

					tree_popover.set_pointing_to(Some(&rect));
					tree_popover.popup();

					selection.select_path(&path);
				}
			});
		}

		tree.add_controller(&gesture);

		// TODO: Is there anyway to do this in the view macro?
		// Entry list columns
		for (i, title) in ["Title", "Username", "URL"].iter().enumerate() {
			let column = gtk::TreeViewColumn::builder().title(title).resizable(true).build();

			let cell = gtk::CellRendererText::builder().ellipsize(pango::EllipsizeMode::End).width_chars(30).build();
			column.pack_start(&cell, true);
			column.add_attribute(&cell, "text", i as i32 + 1);

			tree.append_column(&column);
		}

		// Actions
		let group = RelmActionGroup::<ViewDatabaseActionGroup>::new();

		{
			let tree = tree.clone();
			let sender = sender.clone();
			group.add_action(RelmAction::<CopyUsernameAction>::new_stateless(move |_| {
				if let Some((tree_model, tree_iter)) = tree.selection().selected() {
					let entry_id = tree_model.get::<gtk::glib::Bytes>(&tree_iter, 0);
					let entry_id = ID::from_slice(entry_id.as_ref()).expect("Internal error");

					send!(
						sender,
						ViewDatabaseMsg::CopyUsername {
							id: entry_id,
							clipboard: tree.clipboard()
						}
					);
				}
			}));
		}

		{
			let tree = tree.clone();
			let sender = sender.clone();
			group.add_action(RelmAction::<CopyPasswordAction>::new_stateless(move |_| {
				if let Some((tree_model, tree_iter)) = tree.selection().selected() {
					let entry_id = tree_model.get::<gtk::glib::Bytes>(&tree_iter, 0);
					let entry_id = ID::from_slice(entry_id.as_ref()).expect("Internal error");

					send!(
						sender,
						ViewDatabaseMsg::CopyPassword {
							id: entry_id,
							clipboard: tree.clipboard()
						}
					);
				}
			}));
		}

		{
			let tree = tree.clone();
			group.add_action(RelmAction::<MoveEntryAction>::new_with_target_value(move |_, value| {
				let directory_id = ID::from_slice(value.as_ref()).expect("Internal error");

				if let Some((tree_model, tree_iter)) = tree.selection().selected() {
					let entry_id = tree_model.get::<gtk::glib::Bytes>(&tree_iter, 0);
					let entry_id = ID::from_slice(entry_id.as_ref()).expect("Internal error");

					send!(sender, ViewDatabaseMsg::MoveEntry { entry_id, directory_id });
				}
			}));
		}

		let actions = group.into_action_group();
		tree.insert_action_group(ViewDatabaseActionGroup::group_name(), Some(&actions));
	}

	fn post_view() {
		// Make sure a directory is selected
		if self.directory_list.selected_row().is_none() {
			// If nothing selected, select the second option (should be the root directory)
			if let Some(row) = self.directory_list.row_at_index(1) {
				self.directory_list.select_row(Some(&row));
			}
		}
	}
}

relm4::new_action_group!(ViewDatabaseActionGroup, "view-database");
relm4::new_stateless_action!(CopyUsernameAction, ViewDatabaseActionGroup, "copy-username");
relm4::new_stateless_action!(CopyPasswordAction, ViewDatabaseActionGroup, "copy-password");
relm4::new_stateful_action!(MoveEntryAction, ViewDatabaseActionGroup, "move-entry", Vec<u8>, ());


#[derive(PartialEq)]
enum DirectoryItem {
	Directory { id: ID, name: String },
	New,
	All,
}

#[derive(Debug)]
struct DirectoryItemWidgets {
	row: gtk::ListBoxRow,
	label: gtk::Label,
}

impl FactoryPrototype for DirectoryItem {
	type View = gtk::ListBox;
	type Msg = ViewDatabaseMsg;
	type Factory = FactoryVec<Self>;
	type Widgets = DirectoryItemWidgets;
	type Root = gtk::ListBoxRow;

	fn init_view(&self, key: &usize, sender: Sender<Self::Msg>) -> Self::Widgets {
		let key = *key;
		let hbox = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).build();
		let icon = gtk::Image::from_icon_name("folder-symbolic");
		let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::SlideLeftRight).build();
		let label = gtk::Label::builder()
			.width_chars(20)
			.ellipsize(pango::EllipsizeMode::End)
			.margin_top(12)
			.margin_bottom(12)
			.margin_start(10)
			.halign(gtk::Align::Start)
			.xalign(0.0)
			.build();
		let edit = gtk::Entry::builder().width_chars(20).build();

		stack.add_named(&label, Some("label"));
		stack.add_named(&edit, Some("edit"));

		// Complete the edit when the user presses enter
		let stack_cloned = stack.clone();
		let sender_cloned = sender.clone();
		edit.connect_activate(move |edit| {
			stack_cloned.set_visible_child_name("label");
			send!(sender_cloned, ViewDatabaseMsg::DirectoryEdited(key, edit.text().to_string()));
		});

		// When the Entry loses focus, cancel the edit
		// TODO: Also cancel if Escape is pressed
		let stack_cloned = stack.clone();
		let sender_cloned = sender.clone();
		edit.connect_has_focus_notify(move |edit| {
			let has_focus = edit.has_focus() || edit.first_child().map(|w| w.has_focus()).unwrap_or(false);

			if !has_focus && stack_cloned.visible_child_name().as_deref() == Some("edit") {
				stack_cloned.set_visible_child_name("label");

				send!(sender_cloned, ViewDatabaseMsg::DirectoryEditCancelled(key));
			}
		});

		hbox.append(&icon);
		hbox.append(&stack);

		let row = gtk::ListBoxRow::builder().selectable(true).child(&hbox).build();

		match self {
			Self::Directory { id: _, name } => {
				label.set_label(name);
				edit.set_text(name);
				stack.set_visible_child_name("label");

				// Enter edit mode when the user double clicks us
				let gesture = gtk::GestureClick::builder().button(gtk::gdk::BUTTON_PRIMARY).build();

				let stack_cloned = stack.clone();
				let label_cloned = label.clone();
				gesture.connect_pressed(move |gesture, n, _, _| {
					if n == 2 && stack_cloned.visible_child_name().as_deref() == Some("label") {
						gesture.set_state(gtk::EventSequenceState::Claimed);

						edit.set_text(&label_cloned.label());
						stack_cloned.set_visible_child_name("edit");
						send!(sender, ViewDatabaseMsg::FocusWidget(edit.clone().upcast()));
					}
				});

				stack.add_controller(&gesture);
			},
			Self::New => {
				stack.set_visible_child_name("edit");
				send!(sender, ViewDatabaseMsg::FocusWidget(edit.upcast()));
			},
			Self::All => {
				label.set_label("< All >");
				stack.set_visible_child_name("label");
			},
		}

		DirectoryItemWidgets { row, label }
	}

	fn position(&self, _key: &usize) {}

	fn view(&self, _key: &usize, widgets: &Self::Widgets) {
		match self {
			Self::Directory { id: _, name } => {
				widgets.label.set_text(name.as_str());
			},
			Self::New | Self::All => {},
		}
	}

	fn root_widget(widgets: &Self::Widgets) -> &Self::Root {
		&widgets.row
	}
}
