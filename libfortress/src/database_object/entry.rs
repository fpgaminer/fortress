use super::super::{serde, unix_timestamp, ID};
use rand::{OsRng, Rng};
use std::{
	borrow::Borrow,
	cmp,
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
	pub fn new() -> Entry {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		Entry::inner_new(rng.gen(), Vec::new(), unix_timestamp()).unwrap()
	}

	fn inner_new(id: ID, history: Vec<EntryHistory>, time_created: u64) -> Option<Entry> {
		let mut entry = Entry {
			id: id,
			history: history.clone(),
			time_created: time_created,

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

		self.apply_history(&new_data);
		self.history.push(new_data);
	}

	// Used internally to apply an EntryHistory on top of this object's current state.
	fn apply_history(&mut self, new_data: &EntryHistory) {
		for (key, value) in &new_data.data {
			self.state.insert(key.to_string(), value.to_string());
		}
	}

	// Attempts to merge self and other and return a new Entry.
	// Right now we don't handle any conflicts.  Either self or other must be strictly younger than the other.
	// Returns None if the merge failed.
	pub fn merge(&self, other: &Entry) -> Option<Entry> {
		if self.id != other.id {
			return None;
		}

		// Make sure there are no conflicts
		let shared_history_len = cmp::min(self.history.len(), other.history.len());

		if self.history[..shared_history_len] != other.history[..shared_history_len] {
			return None;
		}

		let mut new_entry = self.clone();

		for history in &other.history[shared_history_len..] {
			new_entry.edit(history.clone());
		}

		return Some(new_entry);
	}

	// Returns true only if it is non-destructive to replace self with other in a Database.
	// This is true only if all of our history is contained within other.
	pub fn safe_to_replace_with(&self, other: &Entry) -> bool {
		if self.id != other.id {
			return false;
		}

		if other.history.len() < self.history.len() {
			return false;
		}

		if self.history[..] != other.history[..self.history.len()] {
			return false;
		}

		return true;
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

		Entry::inner_new(entry.id, entry.history, entry.time_created).ok_or(serde::de::Error::custom("Invalid history"))
	}
}


#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct EntryHistory {
	pub time: u64, // Unix timestamp for when this edit occured (nanoseconds)
	#[serde(serialize_with = "ordered_map")]
	pub data: HashMap<String, String>,
}

impl EntryHistory {
	pub fn new(data: HashMap<String, String>) -> EntryHistory {
		EntryHistory {
			time: unix_timestamp(),
			data: data,
		}
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
	use serde::Serialize;

	let ordered: BTreeMap<_, _> = value.iter().collect();
	ordered.serialize(serializer)
}


#[cfg(test)]
mod tests {
	use super::{Entry, EntryHistory};
	use rand::{OsRng, Rng};
	use serde_json;
	use unix_timestamp;

	fn random_entry_history<T: Rng>(rng: &mut T, time: Option<u64>) -> EntryHistory {
		EntryHistory {
			data: [
				(
					rng.gen_iter::<char>().take(256).collect::<String>(),
					rng.gen_iter::<char>().take(256).collect::<String>(),
				),
				(
					rng.gen_iter::<char>().take(256).collect::<String>(),
					rng.gen_iter::<char>().take(256).collect::<String>(),
				),
			]
			.iter()
			.cloned()
			.collect(),
			time: time.unwrap_or(unix_timestamp()),
		}
	}

	#[test]
	fn history_must_be_ordered() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		let mut entry = Entry::new();
		entry.history = vec![random_entry_history(&mut rng, Some(50)), random_entry_history(&mut rng, Some(0))];

		let serialized = serde_json::to_string(&entry).unwrap();
		assert!(serde_json::from_str::<Entry>(&serialized).is_err());
	}

	#[test]
	#[should_panic]
	fn bad_edit_should_panic() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");
		let mut entry = Entry::new();
		entry.edit(random_entry_history(&mut rng, Some(42)));
		entry.edit(random_entry_history(&mut rng, Some(0)));
	}

	// Tests merge and safe_to_replace_with
	#[test]
	fn merge_and_supersets() {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		// Merge should fail if IDs don't match
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(&mut rng, None));
			let mut entry2 = entry1.clone();
			entry2.id = rng.gen();

			assert!(entry1.merge(&entry2).is_none());
			assert!(!entry1.safe_to_replace_with(&entry2));
		}

		// Merge should fail on conflict
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(&mut rng, None));
			let mut entry2 = entry1.clone();
			entry2.edit(random_entry_history(&mut rng, None));
			entry1.edit(random_entry_history(&mut rng, None));

			assert!(entry1.merge(&entry2).is_none());
			assert!(!entry1.safe_to_replace_with(&entry2));
		}

		// Always safe to replace after merging
		{
			let mut entry1 = Entry::new();
			entry1.edit(random_entry_history(&mut rng, None));
			entry1.edit(random_entry_history(&mut rng, None));
			let mut entry2 = entry1.clone();
			entry2.edit(random_entry_history(&mut rng, None));

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
