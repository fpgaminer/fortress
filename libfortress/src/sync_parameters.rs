use super::fortresscrypto::{self, MasterKey, NetworkKeySuite, LoginKey};
use super::serde;


// Encapsulate username, MasterKey, and all cached derivative data
// to enforce invariants on them.
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct SyncParameters {
	// MasterKey and username are saved to the database's on-disk serialization
	// because MasterKey is very expensive to calculated.
	username: String,
	master_key: MasterKey,

	// Cache
	#[serde(skip_serializing, skip_deserializing)]
	network_key_suite: NetworkKeySuite,
	#[serde(skip_serializing, skip_deserializing)]
	login_key: LoginKey,
	#[serde(skip_serializing, skip_deserializing)]
	login_username: Vec<u8>,	// Hashed username sent to server for authentication
}

impl SyncParameters {
	pub fn new<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> SyncParameters {
		let username = username.as_ref();
		let password = password.as_ref();

		let master_key = MasterKey::derive(username.as_bytes(), password.as_bytes());

		SyncParameters {
			username: username.to_string(),
			network_key_suite: NetworkKeySuite::derive(&master_key),
			login_key: LoginKey::derive(&master_key),
			login_username: fortresscrypto::hash_username_for_login(username.as_bytes()),
			master_key: master_key,
		}
	}

	pub fn get_username(&self) -> &str {
		&self.username
	}

	pub fn get_master_key(&self) -> &MasterKey {
		&self.master_key
	}

	pub fn get_network_key_suite(&self) -> &NetworkKeySuite {
		&self.network_key_suite
	}

	pub fn get_login_key(&self) -> &LoginKey {
		&self.login_key
	}

	pub fn get_login_username(&self) -> &[u8] {
		&self.login_username
	}
}

impl<'de> serde::Deserialize<'de> for SyncParameters {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where D: serde::Deserializer<'de>
	{
		#[derive(Deserialize)]
		struct DeserializableSyncParameters {
			username: String,
			master_key: MasterKey,
		}

		let params = DeserializableSyncParameters::deserialize(deserializer)?;

		Ok(SyncParameters {
			network_key_suite: NetworkKeySuite::derive(&params.master_key),
			login_key: LoginKey::derive(&params.master_key),
			login_username: fortresscrypto::hash_username_for_login(params.username.as_bytes()),
			username: params.username,
			master_key: params.master_key,
		})
	}
}