mod directory;
mod entry;

use serde::{Deserialize, Serialize};

pub use self::{
	directory::{Directory, DirectoryHistoryAction},
	entry::{Entry, EntryHistory},
};

use super::ID;


#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
#[serde(tag = "type")]
pub enum DatabaseObject {
	Entry(Entry),
	Directory(Directory),
}

impl DatabaseObject {
	pub fn get_id(&self) -> &ID {
		match *self {
			DatabaseObject::Entry(ref e) => e.get_id(),
			DatabaseObject::Directory(ref d) => d.get_id(),
		}
	}

	pub fn as_directory(&self) -> Option<&Directory> {
		match self {
			DatabaseObject::Directory(d) => Some(d),
			_ => None,
		}
	}

	pub fn as_directory_mut(&mut self) -> Option<&mut Directory> {
		match self {
			DatabaseObject::Directory(d) => Some(d),
			_ => None,
		}
	}

	pub fn as_entry(&self) -> Option<&Entry> {
		match self {
			DatabaseObject::Entry(e) => Some(e),
			_ => None,
		}
	}

	pub fn as_entry_mut(&mut self) -> Option<&mut Entry> {
		match self {
			DatabaseObject::Entry(e) => Some(e),
			_ => None,
		}
	}
}
