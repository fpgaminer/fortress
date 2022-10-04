use super::super::{unix_timestamp, ID};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use std::{
	borrow::Borrow,
	collections::{BTreeMap, HashMap},
	hash::Hash,
	ops::Index,
};


// History is always ordered (by timestamp).
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Entry {
	id: ID,
	history: Vec<EntryHistory>,
	time_created: u64, // Unix timestamp for when this entry was created (nanoseconds)

	// The current state of the entry
	#[serde(skip_serializing, skip_deserializing)]
	state: HashMap<String, String>,
}

impl Entry {
	#[allow(clippy::new_without_default)]
	pub fn new() -> Entry {
		Entry::inner_new(OsRng.gen(), Vec::new(), unix_timestamp()).unwrap()
	}

	fn inner_new(id: ID, history: Vec<EntryHistory>, time_created: u64) -> Option<Entry> {
		let mut entry = Entry {
			id,
			history: history.clone(),
			time_created,

			state: HashMap::new(),
		};
		let mut min_next_timestamp = 0;

		for history_item in &history {
			// History must be ordered
			if history_item.time < min_next_timestamp || history_item.time == <u64>::max_value() {
				return None;
			}
			min_next_timestamp = history_item.time + 1;

			entry.apply_history(history_item);
		}

		Some(entry)
	}

	// Keeping fields private and providing getters makes these fields readonly to the outside world.
	pub fn get_id(&self) -> &ID {
		&self.id
	}

	pub fn get_time_created(&self) -> u64 {
		self.time_created
	}

	pub fn get_state(&self) -> &HashMap<String, String> {
		&self.state
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&String>
	where
		Q: Hash + Eq,
		String: Borrow<Q>,
	{
		self.state.get(key)
	}

	pub fn get_history(&self) -> &[EntryHistory] {
		&self.history
	}

	pub fn edit(&mut self, mut new_data: EntryHistory) {
		if let Some(last) = self.history.last() {
			if new_data.time <= last.time {
				panic!("Entry history must be ordered");
			}
		}

		// Remove any fields from the EntryHistory if they don't actually cause any changes to our state
		new_data.data.retain(|k, v| self.state.get(k) != Some(v));

		if !new_data.data.is_empty() {
			self.apply_history(&new_data);
			self.history.push(new_data);
		}
	}

	// Used internally to apply an EntryHistory on top of this object's current state.
	fn apply_history(&mut self, new_data: &EntryHistory) {
		for (key, value) in &new_data.data {
			self.state.insert(key.to_string(), value.to_string());
		}
	}

	/// Attempts to merge self and other and return a new Entry.
	/// Returns None if the merge failed.
	pub fn merge(&self, other: &Entry) -> Option<Entry> {
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
		// invalid due to a conflict (two edits at the same time).
		Entry::inner_new(self.id, merged_history, self.time_created)
	}

	/// Returns true only if it is non-destructive to replace self with other in a Database.
	/// This is true only if all of our history is contained within other.
	pub fn safe_to_replace_with(&self, other: &Entry) -> bool {
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

impl<'a, Q: ?Sized> Index<&'a Q> for Entry
where
	Q: Eq + Hash,
	String: Borrow<Q>,
{
	type Output = String;

	#[inline]
	fn index(&self, key: &Q) -> &String {
		self.get(key).expect("no entry found for key")
	}
}

impl<'de> serde::Deserialize<'de> for Entry {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		struct PartialDeserialized {
			id: ID,
			history: Vec<EntryHistory>,
			time_created: u64,
		}

		let entry: PartialDeserialized = serde::Deserialize::deserialize(deserializer)?;

		Entry::inner_new(entry.id, entry.history, entry.time_created).ok_or_else(|| serde::de::Error::custom("Invalid history"))
	}
}


#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct EntryHistory {
	/// Unix timestamp for when this edit occured (nanoseconds)
	pub time: u64,
	#[serde(serialize_with = "ordered_map")]
	pub data: HashMap<String, String>,
}

impl EntryHistory {
	pub fn new(data: HashMap<String, String>) -> EntryHistory {
		EntryHistory { time: unix_timestamp(), data }
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&String>
	where
		Q: Hash + Eq,
		String: Borrow<Q>,
	{
		self.data.get(key)
	}
}

impl<'a, Q: ?Sized> Index<&'a Q> for EntryHistory
where
	Q: Eq + Hash,
	String: Borrow<Q>,
{
	type Output = String;

	#[inline]
	fn index(&self, key: &Q) -> &String {
		self.get(key).expect("no entry found for key")
	}
}

// We have to use this so that the serialization for EntryHistory is deterministic (always the same for the same input).
// If we didn't, the serialized form would change each time, which would cause problems for synchronization.
fn ordered_map<S, K, V>(value: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
	S: serde::Serializer,
	K: Eq + Hash + Ord + serde::Serialize,
	V: serde::Serialize,
{
	let ordered: BTreeMap<_, _> = value.iter().collect();
	ordered.serialize(serializer)
}


#[cfg(test)]
mod tests {
	use std::collections::HashMap;

	use super::{Entry, EntryHistory};
	use crate::{tests::random_uniform_string, unix_timestamp};
	use rand::{rngs::OsRng, thread_rng, Rng};
	use serde_json;

	fn random_entry_history(time: Option<u64>) -> EntryHistory {
		let mut history = EntryHistory {
			time: time.unwrap_or_else(unix_timestamp),
			data: HashMap::new(),
		};

		for _ in 0..thread_rng().gen_range(1..10) {
			history.data.insert(random_uniform_string(1..256), random_uniform_string(0..256));
		}

		history
	}

	#[test]
	fn history_must_be_ordered() {
		let mut entry = Entry::new();
		entry.history = vec![random_entry_history(Some(50)), random_entry_history(Some(0))];

		let serialized = serde_json::to_string(&entry).unwrap();
		assert!(serde_json::from_str::<Entry>(&serialized).is_err());
	}

	#[test]
	#[should_panic]
	fn bad_edit_should_panic() {
		let mut entry = Entry::new();
		entry.edit(random_entry_history(Some(42)));
		entry.edit(random_entry_history(Some(0)));
	}

	// Tests merge and safe_to_replace_with
	#[test]
	fn merge_and_supersets() {
		// Merge should fail if IDs don't match
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(None));
			let mut entry2 = entry1.clone();
			entry2.id = OsRng.gen();

			assert!(entry1.merge(&entry2).is_none());
			assert!(!entry1.safe_to_replace_with(&entry2));
		}

		// Merge should fail on conflict
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(Some(1)));
			let mut entry2 = entry1.clone();
			entry2.edit(random_entry_history(Some(2)));
			entry1.edit(random_entry_history(Some(2)));

			assert!(entry1.merge(&entry2).is_none());
			assert!(!entry1.safe_to_replace_with(&entry2));
		}

		// Always safe to replace after merging
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(None));
			entry1.edit(random_entry_history(None));
			let mut entry2 = entry1.clone();
			entry2.edit(random_entry_history(None));

			assert_eq!(entry2.safe_to_replace_with(&entry1), false);
			let merged1 = entry1.merge(&entry2).unwrap();
			let merged2 = entry2.merge(&entry1).unwrap();
			assert!(entry1.safe_to_replace_with(&merged1));
			assert!(entry2.safe_to_replace_with(&merged1));
			assert!(entry1.safe_to_replace_with(&merged2));
			assert!(entry2.safe_to_replace_with(&merged2));
		}
	}
}
