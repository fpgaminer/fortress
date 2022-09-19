use fortresscrypto::{LoginId, LoginKey, NetworkKeySuite};
use serde::{Deserialize, Serialize};


// Encapsulate username, MasterKey, and all cached derivative data
// to enforce invariants on them.
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct SyncParameters {
	username: String,

	// NetworkKeySuite is saved to the database's on-disk serialization since it is very expensive to calculate.
	network_key_suite: NetworkKeySuite,

	// Cache
	#[serde(skip_serializing, skip_deserializing)]
	login_id: LoginId, // Hashed username sent to server for authentication
}

impl SyncParameters {
	pub fn new<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> SyncParameters {
		let username = username.as_ref();
		let password = password.as_ref();

		let network_key_suite = NetworkKeySuite::derive(username.as_bytes(), password.as_bytes());

		SyncParameters {
			username: username.to_string(),
			network_key_suite,
			login_id: fortresscrypto::hash_username_for_login(username.as_bytes()),
		}
	}

	pub fn get_username(&self) -> &str {
		&self.username
	}

	pub fn get_network_key_suite(&self) -> &NetworkKeySuite {
		&self.network_key_suite
	}

	pub fn get_login_key(&self) -> &LoginKey {
		&self.network_key_suite.login_key
	}

	pub fn get_login_id(&self) -> &LoginId {
		&self.login_id
	}
}

impl<'de> serde::Deserialize<'de> for SyncParameters {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		#[derive(Deserialize)]
		struct DeserializableSyncParameters {
			username: String,
			network_key_suite: NetworkKeySuite,
		}

		let params = DeserializableSyncParameters::deserialize(deserializer)?;

		Ok(SyncParameters {
			login_id: fortresscrypto::hash_username_for_login(params.username.as_bytes()),
			username: params.username,
			network_key_suite: params.network_key_suite,
		})
	}
}
