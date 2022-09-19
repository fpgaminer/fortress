use std::error::Error;

#[derive(Debug)]
pub enum CryptoError {
	/// The encrypted data was corrupted or tampered with.
	DecryptionError,
	/// Truncated data was provided.
	TruncatedData,
	/// Bad Scrypt parameters were provided.
	BadScryptParameters,
	/// IO error.
	IOError(std::io::Error),
	/// Bad checksum.
	BadChecksum,
	/// Unsupported version.
	UnsupportedVersion,
}

impl From<std::io::Error> for CryptoError {
	fn from(e: std::io::Error) -> Self {
		CryptoError::IOError(e)
	}
}

impl Error for CryptoError {}

impl std::fmt::Display for CryptoError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			CryptoError::DecryptionError => write!(f, "Decryption error"),
			CryptoError::TruncatedData => write!(f, "Truncated data"),
			CryptoError::BadScryptParameters => write!(f, "Bad Scrypt parameters"),
			CryptoError::IOError(e) => write!(f, "IO error: {}", e),
			CryptoError::BadChecksum => write!(f, "Bad checksum"),
			CryptoError::UnsupportedVersion => write!(f, "Unsupported version"),
		}
	}
}
