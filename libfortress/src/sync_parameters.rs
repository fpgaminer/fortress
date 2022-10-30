use fortresscrypto::{LoginId, LoginKey, NetworkKeySuite};
use serde::{Deserialize, Serialize};


// Encapsulate username, NetworkKeySuite, and all cached derivative data
// to enforce invariants on them.
#[derive(Serialize, Eq, PartialEq, Debug, Clone)]
pub struct SyncParameters {
	username: String,

	// NetworkKeySuite is saved to the database's on-disk serialization since it is very expensive to calculate.
	network_key_suite: Option<NetworkKeySuite>,

	// Cache
	#[serde(skip_serializing, skip_deserializing)]
	login_id: LoginId, // Hashed username sent to server for authentication
}

impl SyncParameters {
	pub fn new<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> SyncParameters {
		let username = username.as_ref();
		let password = password.as_ref();

		let network_key_suite = Some(NetworkKeySuite::derive(username.as_bytes(), password.as_bytes()));

		SyncParameters {
			username: username.to_string(),
			network_key_suite,
			login_id: fortresscrypto::hash_username_for_login(username.as_bytes()),
		}
	}

	pub fn derive<P: AsRef<str>>(&mut self, password: P) {
		let password = password.as_ref();

		self.network_key_suite = Some(NetworkKeySuite::derive(self.username.as_bytes(), password.as_bytes()));
	}

	pub fn freeze(&self) -> Option<FrozenSyncParameters> {
		self.network_key_suite.as_ref().map(|network_key_suite| FrozenSyncParameters {
			login_id: self.login_id,
			login_key: network_key_suite.login_key.clone(),
		})
	}

	pub fn get_username(&self) -> &str {
		&self.username
	}

	pub fn get_network_key_suite(&self) -> Option<&NetworkKeySuite> {
		self.network_key_suite.as_ref()
	}

	pub fn get_login_key(&self) -> Option<&LoginKey> {
		self.network_key_suite.as_ref().map(|nks| &nks.login_key)
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
			network_key_suite: Option<NetworkKeySuite>,
		}

		let params = DeserializableSyncParameters::deserialize(deserializer)?;

		Ok(SyncParameters {
			login_id: fortresscrypto::hash_username_for_login(params.username.as_bytes()),
			username: params.username,
			network_key_suite: params.network_key_suite,
		})
	}
}


/// This is used by Database to store old sync parameters during password change.
/// The biggest difference is that network_key_suite is not optional.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FrozenSyncParameters {
	login_id: LoginId,
	login_key: LoginKey,
}

impl FrozenSyncParameters {
	pub fn get_login_id(&self) -> &LoginId {
		&self.login_id
	}

	pub fn get_login_key(&self) -> &LoginKey {
		&self.login_key
	}
}
