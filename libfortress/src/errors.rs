use fortresscrypto::CryptoError;

use crate::ApiError;


#[derive(Debug)]
pub enum FortressError {
	IOError(std::io::Error),
	CryptoError(CryptoError),
	SerializationError(serde_json::Error),
	SyncBadUrl,
	SyncApiError(ApiError),
	SyncInconsistentServer,
	SyncConflict,
}

impl From<std::io::Error> for FortressError {
	fn from(error: std::io::Error) -> FortressError {
		FortressError::IOError(error)
	}
}

impl From<CryptoError> for FortressError {
	fn from(error: CryptoError) -> FortressError {
		match error {
			CryptoError::IOError(e) => FortressError::IOError(e),
			_ => FortressError::CryptoError(error),
		}
	}
}

impl From<serde_json::Error> for FortressError {
	fn from(error: serde_json::Error) -> FortressError {
		FortressError::SerializationError(error)
	}
}

impl From<ApiError> for FortressError {
	fn from(error: ApiError) -> FortressError {
		FortressError::SyncApiError(error)
	}
}

impl std::error::Error for FortressError {}

impl std::fmt::Display for FortressError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			FortressError::IOError(e) => write!(f, "IO error: {e}"),
			FortressError::CryptoError(e) => write!(f, "Cryptography error: {e}"),
			FortressError::SerializationError(e) => write!(f, "Serialization error: {e}"),
			FortressError::SyncBadUrl => write!(f, "Bad Sync URL"),
			FortressError::SyncApiError(e) => write!(f, "Sync API error: {e}"),
			FortressError::SyncInconsistentServer => write!(f, "Sync server is inconsistent"),
			FortressError::SyncConflict => write!(f, "Sync Conflict"),
		}
	}
}
