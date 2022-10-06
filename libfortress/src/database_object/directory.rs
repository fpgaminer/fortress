use crate::ROOT_DIRECTORY_ID;

use super::super::{unix_timestamp, Database, ID};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;


/// A directory is a list of references to Entries and Directories, much like a filesystem directory.
/// History is always ordered (by timestamp) and consistent (no double adds or removes of non-existant IDs).
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Directory {
	id: ID,
	history: Vec<DirectoryHistory>,

	#[serde(skip_serializing)]
	pub entries: HashSet<ID>,

	#[serde(skip_serializing)]
	pub name: Option<String>,
}

impl Directory {
	#[allow(clippy::new_without_default)]
	pub fn new() -> Directory {
		Directory {
			id: OsRng.gen(),
			history: Vec::new(),
			entries: HashSet::new(),
			name: None,
		}
	}

	pub fn new_root() -> Directory {
		Directory {
			id: ROOT_DIRECTORY_ID,
			history: Vec::new(),
			entries: HashSet::new(),
			name: None,
		}
	}

	/// Reconstructs state from history.
	/// Returns None if history is invalid.
	fn from_history(id: ID, history: Vec<DirectoryHistory>) -> Option<Directory> {
		let mut entries = HashSet::new();
		let mut min_next_timestamp = 0;
		let mut name = None;

		for history_item in &history {
			// History must be ordered
			if history_item.time < min_next_timestamp || history_item.time == <u64>::max_value() {
				return None;
			}
			min_next_timestamp = history_item.time + 1;

			match history_item.action {
				DirectoryHistoryAction::Add(id) => {
					if !entries.insert(id) {
						return None;
					}
				},
				DirectoryHistoryAction::Remove(id) => {
					if !entries.remove(&id) {
						return None;
					}
				},
				DirectoryHistoryAction::Rename(ref new_name) => {
					name = Some(new_name.clone());
				},
			};
		}

		Some(Directory { id, entries, history, name })
	}

	pub fn get_id(&self) -> &ID {
		&self.id
	}

	pub fn get_history(&self) -> &[DirectoryHistory] {
		&self.history
	}

	pub fn get_name(&self) -> Option<&str> {
		self.name.as_deref()
	}

	pub fn add(&mut self, id: ID) {
		self.add_with_time(id, unix_timestamp())
	}

	pub fn add_with_time(&mut self, id: ID, time: u64) {
		if !self.entries.insert(id) {
			panic!("Cannot add duplicate ID to directory.");
		}

		if let Some(last) = self.history.last() {
			if time <= last.time {
				panic!("Directory history must be ordered");
			}
		}

		self.history.push(DirectoryHistory {
			action: DirectoryHistoryAction::Add(id),
			time,
		});
	}

	pub fn remove(&mut self, id: ID) {
		self.remove_with_time(id, unix_timestamp())
	}

	pub fn remove_with_time(&mut self, id: ID, time: u64) {
		if !self.entries.remove(&id) {
			panic!("Cannot remove non-existant ID from directory.");
		}

		if let Some(last) = self.history.last() {
			if time <= last.time {
				panic!("Directory history must be ordered");
			}
		}

		self.history.push(DirectoryHistory {
			action: DirectoryHistoryAction::Remove(id),
			time,
		});
	}

	pub fn rename<S: Into<String>>(&mut self, name: S) {
		self.rename_with_time(name, unix_timestamp())
	}

	pub fn rename_with_time<S: Into<String>>(&mut self, name: S, time: u64) {
		let name = name.into();

		if let Some(last) = self.history.last() {
			if time <= last.time {
				panic!("Directory history must be ordered");
			}
		}

		// Don't add a rename if the name is the same
		if self.name.as_deref() == Some(&name) {
			return;
		}

		self.name = Some(name.clone());

		self.history.push(DirectoryHistory {
			action: DirectoryHistoryAction::Rename(name),
			time,
		});
	}

	/// List all Entry entries in this directory.
	pub fn list_entries<'a>(&'a self, database: &Database) -> Vec<&'a ID> {
		self.entries.iter().filter(|id| database.get_entry_by_id(id).is_some()).collect()
	}

	/// List all Directory entries in this directory.
	pub fn list_directories<'a>(&'a self, database: &Database) -> Vec<&'a ID> {
		self.entries.iter().filter(|id| database.get_directory_by_id(id).is_some()).collect()
	}

	/// Merge self and other, returning a new Directory.
	/// Returns None if there is a conflict.
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

	/// Returns true only if it is safe to replace self with other in the Database.
	/// This is only true if doing so is a non-destructive operation (i.e. history is perserved).
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

		true
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
		Directory::from_history(d.id, d.history).ok_or_else(|| serde::de::Error::custom("Invalid history"))
	}
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DirectoryHistory {
	pub action: DirectoryHistoryAction,
	/// Unix timestamp for when this edit occurred (in nanoseconds).
	pub time: u64,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum DirectoryHistoryAction {
	Add(ID),
	Remove(ID),
	Rename(String),
}


#[cfg(test)]
mod tests {
	use super::{Directory, DirectoryHistory, DirectoryHistoryAction};
	use crate::tests::quick_sleep;
	use rand::{rngs::OsRng, Rng};
	use serde_json;

	#[test]
	fn history_must_be_ordered() {
		let mut bad_directory1 = Directory::new();
		bad_directory1.history = vec![
			DirectoryHistory {
				action: DirectoryHistoryAction::Add(OsRng.gen()),
				time: 50,
			},
			DirectoryHistory {
				action: DirectoryHistoryAction::Add(OsRng.gen()),
				time: 0,
			},
		];

		let serialized = serde_json::to_string(&bad_directory1).unwrap();
		assert!(serde_json::from_str::<Directory>(&serialized).is_err());
	}

	// Cannot add IDs that already exist or delete IDs that don't exist
	#[test]
	fn history_must_be_consistent() {
		{
			let mut bad_directory = Directory::new();
			let id1 = OsRng.gen();
			bad_directory.history = vec![
				DirectoryHistory {
					action: DirectoryHistoryAction::Add(id1),
					time: 0,
				},
				DirectoryHistory {
					action: DirectoryHistoryAction::Add(id1),
					time: 5,
				},
			];

			let serialized = serde_json::to_string(&bad_directory).unwrap();
			assert!(serde_json::from_str::<Directory>(&serialized).is_err());
		}

		{
			let mut bad_directory = Directory::new();
			bad_directory.history = vec![DirectoryHistory {
				action: DirectoryHistoryAction::Remove(OsRng.gen()),
				time: 0,
			}];

			let serialized = serde_json::to_string(&bad_directory).unwrap();
			assert!(serde_json::from_str::<Directory>(&serialized).is_err());
		}
	}

	#[test]
	#[should_panic]
	fn bad_add_should_panic1() {
		let mut directory = Directory::new();
		let id = OsRng.gen();
		directory.add(id);
		quick_sleep();
		directory.add(id);
	}

	#[test]
	#[should_panic]
	fn bad_add_should_panic2() {
		let mut directory = Directory::new();
		directory.add_with_time(OsRng.gen(), 42);
		directory.add_with_time(OsRng.gen(), 0);
	}

	#[test]
	#[should_panic]
	fn bad_remove_should_panic1() {
		let mut directory = Directory::new();
		directory.remove(OsRng.gen());
	}

	#[test]
	#[should_panic]
	fn bad_remove_should_panic2() {
		let mut directory = Directory::new();
		let id = OsRng.gen();
		directory.add_with_time(id, 1000);
		directory.remove_with_time(id, 999);
	}

	#[test]
	#[should_panic]
	fn bad_rename_should_panic1() {
		let mut directory = Directory::new();
		directory.rename_with_time("Name1", 1000);
		directory.rename_with_time("Name2", 999);
	}

	// Tests merge and safe_to_replace_with
	#[test]
	fn merge_and_supersets() {
		// Merge should fail if IDs don't match
		{
			let mut directory1 = Directory::new();
			directory1.add(OsRng.gen());
			let mut directory2 = directory1.clone();
			directory2.id = OsRng.gen();
			assert!(directory1.merge(&directory2).is_none());
			assert!(!directory1.safe_to_replace_with(&directory2));
		}

		// Merge should fail on conflict
		{
			let mut directory1 = Directory::new();
			directory1.add(OsRng.gen());
			quick_sleep();
			let mut directory2 = directory1.clone();
			let id = OsRng.gen();
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
			directory1.add(OsRng.gen());
			quick_sleep();
			directory2.add(OsRng.gen());
			assert!(!directory1.safe_to_replace_with(&directory2));
		}

		// Always safe to replace after merging
		{
			let mut directory1 = Directory::new();
			directory1.add(OsRng.gen());
			quick_sleep();
			let id = OsRng.gen();
			directory1.add(id);
			quick_sleep();
			directory1.add(OsRng.gen());
			quick_sleep();
			let mut directory2 = directory1.clone();
			directory2.add(OsRng.gen());
			quick_sleep();
			directory2.remove(id);
			quick_sleep();
			directory1.add(OsRng.gen());

			assert_eq!(directory1.safe_to_replace_with(&directory2), false);
			let merged1 = directory1.merge(&directory2).unwrap();
			let merged2 = directory2.merge(&directory1).unwrap();
			assert!(directory1.safe_to_replace_with(&merged1));
			assert!(directory2.safe_to_replace_with(&merged1));
			assert!(directory1.safe_to_replace_with(&merged2));
			assert!(directory2.safe_to_replace_with(&merged2));
		}
	}

	#[test]
	fn test_rename() {
		let mut directory = Directory::new();
		assert_eq!(directory.get_name(), None);
		directory.rename("Name1");
		assert_eq!(directory.get_name(), Some("Name1"));
		directory.rename("Name2");
		assert_eq!(directory.get_name(), Some("Name2"));

		let serialized = serde_json::to_string(&directory).unwrap();
		let deserialized: Directory = serde_json::from_str(&serialized).unwrap();
		assert_eq!(deserialized.get_name(), Some("Name2"));
	}
}
