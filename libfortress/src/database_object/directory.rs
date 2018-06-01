use rand::{OsRng, Rng};
use std::collections::HashSet;
use super::super::{serde, ID, Database, unix_timestamp};


// A directory is a list of references to Entries and Directories, much like a filesystem directory.
// History is always ordered (by timestamp) and consistent (no double adds or removes of non-existant IDs).
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
		self.add_with_time(id, unix_timestamp())
	}

	pub fn add_with_time(&mut self, id: ID, time: u64) {
		if !self.entries.insert(id) {
			panic!("Attempt to add an ID to directory that already exists.");
		}

		if let Some(last) = self.history.last() {
			if time <= last.time {
				panic!("Directory history must be ordered");
			}
		}

		self.history.push(DirectoryHistory {
			id: id,
			action: DirectoryHistoryAction::Add,
			time: time,
		});
	}

	pub fn remove(&mut self, id: ID) {
		self.remove_with_time(id, unix_timestamp())
	}

	pub fn remove_with_time(&mut self, id: ID, time: u64) {
		if !self.entries.remove(&id) {
			panic!("Attempt to remove an ID from directory that doesn't exist");
		}

		if let Some(last) = self.history.last() {
			if time <= last.time {
				panic!("Directory history must be ordered");
			}
		}

		self.history.push(DirectoryHistory {
			id: id,
			action: DirectoryHistoryAction::Remove,
			time: time,
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
		let mut min_next_timestamp = 0;

		for history in &d.history {
			// History must be ordered
			if history.time < min_next_timestamp || history.time == <u64>::max_value() {
				return Err(serde::de::Error::custom("Invalid history"));
			}
			min_next_timestamp = history.time + 1;

			match history.action {
				DirectoryHistoryAction::Add => if !entries.insert(history.id) {
					return Err(serde::de::Error::custom("Invalid history"));
				},
				DirectoryHistoryAction::Remove => if !entries.remove(&history.id) {
					return Err(serde::de::Error::custom("Invalid history"));
				},
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
	pub time: u64,    // Unix timestamp for when this edit occured (nanoseconds)
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum DirectoryHistoryAction {
	Add,
	Remove,
}


#[cfg(test)]
mod tests {
	use rand::{OsRng, Rng};
	use serde_json;
	use super::{Directory, DirectoryHistory, DirectoryHistoryAction};

	#[test]
	fn history_must_be_ordered() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		let mut bad_directory1 = Directory::new();
		bad_directory1.history = vec![
			DirectoryHistory {
				id: rng.gen(), action: DirectoryHistoryAction::Add, time: 50,
			},
			DirectoryHistory {
				id: rng.gen(), action: DirectoryHistoryAction::Add, time: 0,
			}
		];

		let serialized = serde_json::to_string(&bad_directory1).unwrap();
		assert!(serde_json::from_str::<Directory>(&serialized).is_err());
	}

	// Cannot add IDs that already exist or delete IDs that don't exist
	#[test]
	fn history_must_be_consistent() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		{
			let mut bad_directory = Directory::new();
			let id1 = rng.gen();
			bad_directory.history = vec![
				DirectoryHistory {
					id: id1, action: DirectoryHistoryAction::Add, time: 0,
				},
				DirectoryHistory {
					id: id1, action: DirectoryHistoryAction::Add, time: 5,
				}
			];

			let serialized = serde_json::to_string(&bad_directory).unwrap();
			assert!(serde_json::from_str::<Directory>(&serialized).is_err());
		}

		{
			let mut bad_directory = Directory::new();
			bad_directory.history = vec![
				DirectoryHistory {
					id: rng.gen(), action: DirectoryHistoryAction::Remove, time: 0,
				},
			];

			let serialized = serde_json::to_string(&bad_directory).unwrap();
			assert!(serde_json::from_str::<Directory>(&serialized).is_err());
		}
	}

	#[test]
	#[should_panic]
	fn bad_add_should_panic1() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let mut directory = Directory::new();
		let id = rng.gen();
		directory.add(id);
		directory.add(id);
	}

	#[test]
	#[should_panic]
	fn bad_add_should_panic2() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let mut directory = Directory::new();
		directory.add_with_time(rng.gen(), 42);
		directory.add_with_time(rng.gen(), 0);
	}

	#[test]
	#[should_panic]
	fn bad_remove_should_panic1() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let mut directory = Directory::new();
		directory.remove(rng.gen());
	}

	#[test]
	#[should_panic]
	fn bad_remove_should_panic2() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let mut directory = Directory::new();
		let id = rng.gen();
		directory.add_with_time(id, 1000);
		directory.remove_with_time(id, 999);
	}

}