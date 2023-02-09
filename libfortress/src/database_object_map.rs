use super::{database_object::DatabaseObject, ID};
use std::{self, borrow::Borrow, collections::HashMap, hash::Hash};


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
#[derive(Eq, PartialEq, Debug, Clone, Default)]
pub struct DatabaseObjectMap {
	inner: HashMap<ID, DatabaseObject>,
}

impl DatabaseObjectMap {
	pub fn new() -> DatabaseObjectMap {
		DatabaseObjectMap { inner: HashMap::new() }
	}

	pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&DatabaseObject>
	where
		Q: Hash + Eq,
		ID: Borrow<Q>,
	{
		self.inner.get(key)
	}

	pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut DatabaseObject>
	where
		Q: Hash + Eq,
		ID: Borrow<Q>,
	{
		self.inner.get_mut(key)
	}

	pub fn len(&self) -> usize {
		self.inner.len()
	}

	/// Update an object in the map (or insert if it didn't already exist)
	/// NOTE: Does not allow you to overwrite an existing object if that operation would be destructive (e.g. older version, conflicting history, etc).
	pub fn update(&mut self, object: DatabaseObject) {
		match (self.inner.get(object.get_id()), &object) {
			(Some(DatabaseObject::Entry(existing)), DatabaseObject::Entry(new_object)) => {
				if !existing.safe_to_replace_with(new_object) {
					panic!("Attempted to overwrite an existing DatabaseObject with an older version.");
				}
			},
			(Some(DatabaseObject::Directory(existing)), DatabaseObject::Directory(new_object)) => {
				if !existing.safe_to_replace_with(new_object) {
					panic!("Attempted to overwrite an existing DatabaseObject with an older version.");
				}
			},
			(None, _) => {},
			_ => {
				panic!("Attempted to overwrite an existing DatabaseObject with a different type object.");
			},
		}

		self.inner.insert(*object.get_id(), object);
	}

	pub fn values(&self) -> impl Iterator<Item = &DatabaseObject> {
		self.inner.values()
	}

	pub fn values_mut(&mut self) -> impl Iterator<Item = &mut DatabaseObject> {
		self.inner.values_mut()
	}
}

impl serde::Serialize for DatabaseObjectMap {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		// Deterministic serialization of the hashmap by ordering keys
		// Also, we serialize to a Vec since we already have the IDs in the objects themselves
		let mut ordered = self.inner.values().collect::<Vec<_>>();
		ordered.sort_unstable_by(|a, b| a.get_id().cmp(b.get_id()));

		ordered.serialize(serializer)
	}
}

impl<'de> serde::Deserialize<'de> for DatabaseObjectMap {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		Ok(DatabaseObjectMap {
			inner: Vec::deserialize(deserializer)?
				.into_iter()
				.map(|object: DatabaseObject| (*object.get_id(), object))
				.collect(),
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


#[cfg(test)]
mod tests {
	use rand::{rngs::OsRng, Rng};

	use crate::Directory;

	use super::{
		super::{DatabaseObject, Entry, EntryHistory},
		DatabaseObjectMap,
	};

	#[test]
	#[should_panic]
	fn cannot_overwrite_with_older_entry() {
		let mut object_map = DatabaseObjectMap::new();

		let mut entry = Entry::new();
		let old_entry = entry.clone();
		entry.edit(EntryHistory::new(
			[("title".to_string(), "Panic at the HashMap".to_string())].iter().cloned().collect(),
		));

		object_map.update(DatabaseObject::Entry(entry));

		// TODO: It would be nice to not use [should_panic] on this whole test function
		// and rather just indicate that this particular statement should panic.
		// I was not able to find a nice way to do that yet.
		object_map.update(DatabaseObject::Entry(old_entry));
	}

	#[test]
	#[should_panic]
	fn cannot_overwrite_with_older_directory() {
		let mut object_map = DatabaseObjectMap::new();

		let mut directory = Directory::new();
		let old_directory = directory.clone();
		directory.add(OsRng.gen());

		object_map.update(DatabaseObject::Directory(directory));
		object_map.update(DatabaseObject::Directory(old_directory));
	}

	// TODO
	/*#[test]
	#[should_panic]
	fn cannot_overwrite_with_different_type() {
		let mut object_map = DatabaseObjectMap::new();
	}*/
}
