use rand::{OsRng, Rng};
use std::collections::HashSet;
use super::super::{serde, ID, Database};


// A directory is a list of references to Entries and Directories, much like a filesystem directory.
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Directory {
	id: ID,
	history: Vec<DirectoryHistory>,

	#[serde(skip_serializing)]
	pub entries: HashSet<ID>,
}

impl Directory {
	pub fn new() -> Directory {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		Directory {
			id: rng.gen(),
			entries: HashSet::new(),
			history: Vec::new(),
		}
	}

	pub fn get_id(&self) -> &ID {
		&self.id
	}

	pub fn get_history(&self) -> &[DirectoryHistory] {
		&self.history
	}

	pub fn add(&mut self, id: ID) {
		self.entries.insert(id);
		self.history.push(DirectoryHistory {
			id: id,
			action: DirectoryHistoryAction::Add,
		});
	}

	pub fn remove(&mut self, id: ID) {
		if !self.entries.remove(&id) {
			panic!("Attempt to remove an ID from directory that doesn't exist");
		}

		self.history.push(DirectoryHistory {
			id: id,
			action: DirectoryHistoryAction::Remove,
		});
	}

	// List all Entry entries in this directory
	pub fn list_entries<'a>(&'a self, database: &Database) -> Vec<&'a ID> {
		self.entries.iter().filter(|id| {
			database.get_entry_by_id(id).is_some()
		}).collect()
	}
}

impl<'de> serde::Deserialize<'de> for Directory {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		struct DirectoryDeserialized {
			id: ID,
			history: Vec<DirectoryHistory>,
		}

		let d: DirectoryDeserialized = serde::Deserialize::deserialize(deserializer)?;
		let mut entries = HashSet::new();

		// Re-construct current state from history
		for history in &d.history {
			match history.action {
				DirectoryHistoryAction::Add => entries.insert(history.id),
				DirectoryHistoryAction::Remove => entries.remove(&history.id),
			};
		}

		Ok(Directory {
			id: d.id,
			entries: entries,
			history: d.history,
		})
	}
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DirectoryHistory {
	pub id: ID,
	pub action: DirectoryHistoryAction,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum DirectoryHistoryAction {
	Add,
	Remove,
}