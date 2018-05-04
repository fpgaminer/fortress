use std;
use std::collections::{HashMap, BTreeMap};
use std::borrow::Borrow;
use std::hash::Hash;
use super::serde;
use super::database_object::DatabaseObject;
use super::ID;


// We wrap HashMap to enforce some invariants.
// The HashMap should never be modified directly; all modifications are performed through this wrapper.
// This allows us to enforce important invariants.  For example, by enforcing that the ID of the DatabaseObject always matches
// the key in the HashMap, we can ensure that the DatabaseObject's internal invariants are respected.
// This is because it's not possible to directly modify the ID of an object.  So the only way to update objects in the database is to
// either grab a mutable reference to it or use this struct's update function to "replace" the object.  In the former case,
// the DatabaseObject enforces its own invariants itself.  In the latter case you can only replace an object with a clone of itself,
// otherwise the IDs wouldn't match, so again it can enforce its own invariants.
// All of this ensures DatabaseObject's invariants are respected.
// Most important, DatabaseObject's ensure their history is never destructively modified; so we can be sure, through these APIs,
// that user data is always perserved.
// NOTE: It's of course possible to maliciously invalidate these invariants by, for example,
// serializing a DatabaseObject, modifying the serialized representation, and then Deserializing,
// but the point is to make it difficult and unnatural to bypass the invariants; it shouldn't
// happen accidentally.
#[derive(Eq, PartialEq, Debug)]
pub struct DatabaseObjectMap {
	inner: HashMap<ID, DatabaseObject>,
}

impl DatabaseObjectMap {
	pub fn new() -> DatabaseObjectMap {
		DatabaseObjectMap {
			inner: HashMap::new(),
		}
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&DatabaseObject>
		where Q: Hash + Eq,
			  ID: Borrow<Q>
	{
		self.inner.get(key)
	}

	pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut DatabaseObject>
		where Q: Hash + Eq,
		      ID: Borrow<Q>
	{
		self.inner.get_mut(key)
	}

	pub fn len(&self) -> usize {
		self.inner.len()
	}

	// Update an object in the map (or insert if it didn't already exist)
	pub fn update(&mut self, object: DatabaseObject) {
		self.inner.insert(object.get_id().clone(), object);
	}
}

impl serde::Serialize for DatabaseObjectMap {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where S: serde::Serializer
	{
		// Deterministic serialization of the hashmap by ordering keys
		let ordered: BTreeMap<_, _> = self.inner.iter().collect();
		ordered.serialize(serializer)
	}
}

impl<'de> serde::Deserialize<'de> for DatabaseObjectMap {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where D: serde::Deserializer<'de>
	{
		Ok(DatabaseObjectMap {
			inner: HashMap::deserialize(deserializer)?,
		})
	}
}

impl<'a> IntoIterator for &'a DatabaseObjectMap {
	type Item = (&'a ID, &'a DatabaseObject);
	type IntoIter = std::collections::hash_map::Iter<'a, ID, DatabaseObject>;

	fn into_iter(self) -> std::collections::hash_map::Iter<'a, ID, DatabaseObject> {
		self.inner.iter()
	}
}