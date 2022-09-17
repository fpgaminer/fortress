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

	// Returns None if history is invalid
	fn from_history(id: ID, history: Vec<DirectoryHistory>) -> Option<Directory> {
		// Re-construct state from history
		let mut entries = HashSet::new();
		let mut min_next_timestamp = 0;

		for history_item in &history {
			// History must be ordered
			if history_item.time < min_next_timestamp || history_item.time == <u64>::max_value() {
				return None;
			}
			min_next_timestamp = history_item.time + 1;

			match history_item.action {
				DirectoryHistoryAction::Add => if !entries.insert(history_item.id) {
					return None;
				},
				DirectoryHistoryAction::Remove => if !entries.remove(&history_item.id) {
					return None;
				},
			};
		}

		Some(Directory {
			id: id,
			entries: entries,
			history: history,
		})
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

	// Merge self and other, returning a new Directory
	// Returns None if there is a conflict.
	pub fn merge(&self, other: &Directory) -> Option<Directory> {
		if self.id != other.id {
			return None;
		}

		// Concat histories
		let mut merged_history = [&self.history[..], &other.history[..]].concat();

		// Sort by timestamp
		merged_history.sort_unstable_by(|a, b| a.time.cmp(&b.time));

		// Remove duplicates (the same timestamp and operation)
		merged_history.dedup();

		// Re-build state and validate
		// If we are unable to re-build state that means the merged history was
		// invalid due to a conflict.
		Directory::from_history(self.id, merged_history)
	}

	// Returns true only if it is safe to replace self with other in the Database.
	// This is only true if doing so is a non-destructive operation (i.e. history is perserved).
	pub fn safe_to_replace_with(&self, other: &Directory) -> bool {
		if self.id != other.id {
			return false;
		}

		let mut other_iter = other.history.iter();

		// Sequentially search other's history for our history.
		for item in &self.history {
			if !other_iter.any(|other_item| other_item == item) {
				return false;
			}
		}

		return true;
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
		
		// Re-builds state and validates
		Directory::from_history(d.id, d.history).ok_or(serde::de::Error::custom("Invalid history"))
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
	use crate::tests::quick_sleep;

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
		quick_sleep();
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

	// Tests merge and safe_to_replace_with
	#[test]
	fn merge_and_supersets() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Merge should fail if IDs don't match
		{
			let mut directory1 = Directory::new();
			directory1.add(rng.gen());
			let mut directory2 = directory1.clone();
			directory2.id = rng.gen();
			assert!(directory1.merge(&directory2).is_none());
			assert!(!directory1.safe_to_replace_with(&directory2));
		}

		// Merge should fail on conflict
		{
			let mut directory1 = Directory::new();
			directory1.add(rng.gen());
			quick_sleep();
			let mut directory2 = directory1.clone();
			let id = rng.gen();
			directory1.add(id);
			quick_sleep();
			directory2.add(id);
			assert!(directory1.merge(&directory2).is_none());
			assert!(!directory1.safe_to_replace_with(&directory2));
		}

		// Not safe to replace when history is different
		{
			let mut directory1 = Directory::new();
			let mut directory2 = directory1.clone();
			directory1.add(rng.gen());
			quick_sleep();
			directory2.add(rng.gen());
			assert!(!directory1.safe_to_replace_with(&directory2));
		}

		// Always safe to replace after merging
		{
			let mut directory1 = Directory::new();
			directory1.add(rng.gen());
			quick_sleep();
			let id = rng.gen();
			directory1.add(id);
			quick_sleep();
			directory1.add(rng.gen());
			quick_sleep();
			let mut directory2 = directory1.clone();
			directory2.add(rng.gen());
			quick_sleep();
			directory2.remove(id);
			quick_sleep();
			directory1.add(rng.gen());

			assert_eq!(directory1.safe_to_replace_with(&directory2), false);
			let merged1 = directory1.merge(&directory2).unwrap();
			let merged2 = directory2.merge(&directory1).unwrap();
			assert!(directory1.safe_to_replace_with(&merged1));
			assert!(directory2.safe_to_replace_with(&merged1));
			assert!(directory1.safe_to_replace_with(&merged2));
			assert!(directory2.safe_to_replace_with(&merged2));
		}
	}
}