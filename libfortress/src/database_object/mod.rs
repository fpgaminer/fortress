mod directory;
mod entry;

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
		match self {
			&DatabaseObject::Entry(ref e) => e.get_id(),
			&DatabaseObject::Directory(ref d) => d.get_id(),
		}
	}
}
