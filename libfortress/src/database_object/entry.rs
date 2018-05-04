use rand::{OsRng, Rng};
use super::super::{time, serde, ID};
use std::collections::{HashMap, BTreeMap};
use std::hash::Hash;
use std::borrow::Borrow;
use std::ops::Index;


#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct Entry {
	id: ID,
	history: Vec<EntryHistory>,
	time_created: i64,

	// The current state of the entry
	#[serde(skip_serializing, skip_deserializing)]
	state: HashMap<String, String>,
}

impl Entry {
	pub fn new() -> Entry {
		let mut rng = OsRng::new().expect("OsRng failed to initialize");

		Entry::inner_new(rng.gen(), Vec::new(), time::now_utc().to_timespec().sec)
	}

	fn inner_new(id: ID, history: Vec<EntryHistory>, time_created: i64) -> Entry {
		Entry {
			id: id,
			history: history,
			time_created: time_created,

			state: HashMap::new(),
		}
	}

	// Keeping fields private and providing getters makes these fields readonly to the outside world.
	pub fn get_id(&self) -> &ID {
		&self.id
	}

	pub fn get_time_created(&self) -> i64 {
		self.time_created
	}

	pub fn get_state(&self) -> &HashMap<String, String> {
		&self.state
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&String>
		where Q: Hash + Eq,
			  String: Borrow<Q>
	{
		self.state.get(key)
	}

	pub fn get_history(&self) -> &[EntryHistory] {
		&self.history
	}

	pub fn edit(&mut self, mut new_data: EntryHistory) {
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
}

impl<'a, Q: ?Sized> Index<&'a Q> for Entry
	where Q: Eq + Hash,
		  String: Borrow<Q>
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
			time_created: i64,
		}

		let entry: PartialDeserialized = serde::Deserialize::deserialize(deserializer)?;
		let history = entry.history.clone();
		let mut entry = Entry::inner_new(entry.id, entry.history, entry.time_created);

		// Re-construct current state from history
		for history in &history {
			entry.apply_history(history);
		}

		Ok(entry)
	}
}


#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct EntryHistory {
	pub time: i64,
	#[serde(serialize_with = "ordered_map")]
	pub data: HashMap<String, String>,
}

impl EntryHistory {
	pub fn new(data: HashMap<String, String>) -> EntryHistory {
		EntryHistory {
			time: time::now_utc().to_timespec().sec,
			data: data,
		}
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&String>
		where Q: Hash + Eq,
			  String: Borrow<Q>
	{
		self.data.get(key)
	}
}

impl<'a, Q: ?Sized> Index<&'a Q> for EntryHistory
	where Q: Eq + Hash,
		  String: Borrow<Q>
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
	where S: serde::Serializer,
	      K: Eq + Hash + Ord + serde::Serialize,
		  V: serde::Serialize
{
	use serde::Serialize;

	let ordered: BTreeMap<_, _> = value.iter().collect();
	ordered.serialize(serializer)
}