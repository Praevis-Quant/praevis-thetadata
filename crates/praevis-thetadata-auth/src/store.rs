use crate::{Environment, Session, User};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const SERVICE: &str = "praevis.thetadata.auth.v1";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("profile must be 1..=64 ASCII letters, digits, hyphens, or underscores")]
    InvalidProfile,
    #[error("native session persistence is supported on Windows and Linux")]
    UnsupportedPlatform,
    // Do not format keyring errors: some include the raw secret when decoding fails.
    #[error("native credential store operation failed")]
    CredentialManager,
    #[error(
        "Linux Secret Service is unavailable or locked; start a D-Bus user session and unlock the default keyring"
    )]
    SecretServiceUnavailable,
    #[error("stored session is invalid or has an unsupported format; authenticate again")]
    InvalidRecord,
    #[error("session environment does not match the store")]
    EnvironmentMismatch,
    #[error("system clock is earlier than the Unix epoch")]
    Clock,
}

/// Native per-user storage, scoped by profile and environment. Does not save login credentials.
pub struct SessionStore {
    profile: String,
    environment: Environment,
}

pub struct StoredSession {
    session: Session,
    saved_at_unix: u64,
}

impl StoredSession {
    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn into_session(self) -> Session {
        self.session
    }
    pub fn saved_at_unix(&self) -> u64 {
        self.saved_at_unix
    }
}

impl std::fmt::Debug for StoredSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredSession")
            .field("session", &self.session)
            .field("saved_at_unix", &self.saved_at_unix)
            .finish()
    }
}

// Private persistence format, distinct from the public Session API.
#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    session_id: String,
    user: User,
    environment: Environment,
    saved_at_unix: u64,
}

impl SessionStore {
    pub fn new(profile: impl Into<String>, environment: Environment) -> Result<Self, StoreError> {
        let profile = profile.into();
        if profile.is_empty()
            || profile.len() > 64
            || !profile
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(StoreError::InvalidProfile);
        }
        Ok(Self {
            profile,
            environment,
        })
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Human-readable native backend, also used by CLI status output.
    pub fn backend_name(&self) -> &'static str {
        if cfg!(windows) {
            "Windows Credential Manager"
        } else if cfg!(target_os = "linux") {
            "Linux Secret Service"
        } else {
            "Unsupported platform"
        }
    }

    /// Stable target that all applications use to share the same stored session.
    pub fn target(&self) -> String {
        let environment = match self.environment {
            Environment::Prod => "PROD",
            Environment::Stage => "STAGE",
        };
        format!("{SERVICE}:{environment}:{}", self.profile)
    }

    #[cfg(windows)]
    fn entry(&self) -> Result<keyring::Entry, StoreError> {
        // Explicit target avoids depending on the keyring crate's naming convention.
        keyring::Entry::new_with_target(&self.target(), SERVICE, &self.profile)
            .map_err(|_| StoreError::CredentialManager)
    }

    #[cfg(target_os = "linux")]
    fn entry(&self) -> Result<keyring::Entry, StoreError> {
        // Secret Service interprets `target` as a COLLECTION name. Keep the
        // existing default/login collection and scope items by service instead.
        keyring::Entry::new(&self.target(), &self.profile).map_err(storage_error)
    }

    pub fn save(&self, session: &Session) -> Result<(), StoreError> {
        if session.environment != self.environment {
            return Err(StoreError::EnvironmentMismatch);
        }
        let saved_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::Clock)?
            .as_secs();
        let encoded = serde_json::to_string(&Record {
            version: 1,
            session_id: session.session_id.clone(),
            user: session.user.clone(),
            environment: session.environment,
            saved_at_unix,
        })
        .map_err(|_| StoreError::InvalidRecord)?;
        self.write_secret(&encoded)
    }

    pub fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        self.read_secret()?
            .map(|encoded| self.decode(&encoded))
            .transpose()
    }

    fn decode(&self, encoded: &str) -> Result<StoredSession, StoreError> {
        let record: Record =
            serde_json::from_str(encoded).map_err(|_| StoreError::InvalidRecord)?;
        if record.version != 1
            || record.session_id.trim().is_empty()
            || record.user.email.trim().is_empty()
            || record.environment != self.environment
        {
            return Err(StoreError::InvalidRecord);
        }
        Ok(StoredSession {
            session: Session {
                session_id: record.session_id,
                user: record.user,
                environment: record.environment,
            },
            saved_at_unix: record.saved_at_unix,
        })
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn write_secret(&self, encoded: &str) -> Result<(), StoreError> {
        self.entry()?.set_password(encoded).map_err(storage_error)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn write_secret(&self, _: &str) -> Result<(), StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn read_secret(&self) -> Result<Option<String>, StoreError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(storage_error(error)),
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn read_secret(&self) -> Result<Option<String>, StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }

    /// Remove only this profile/environment's local session. Does not revoke it on the server.
    #[cfg(any(windows, target_os = "linux"))]
    pub fn clear(&self) -> Result<bool, StoreError> {
        match self.entry()?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(storage_error(error)),
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn clear(&self) -> Result<bool, StoreError> {
        Err(StoreError::UnsupportedPlatform)
    }
}

#[cfg(any(windows, target_os = "linux"))]
fn storage_error(error: keyring::Error) -> StoreError {
    // Never expose native errors: some variants carry the stored secret.
    if cfg!(target_os = "linux")
        && matches!(
            error,
            keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_)
        )
    {
        StoreError::SecretServiceUnavailable
    } else {
        StoreError::CredentialManager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_profiles_and_environments() {
        let prod = SessionStore::new("default", Environment::Prod).unwrap();
        let stage = SessionStore::new("default", Environment::Stage).unwrap();
        let other = SessionStore::new("other", Environment::Prod).unwrap();
        assert_ne!(prod.target(), stage.target());
        assert_ne!(prod.target(), other.target());
        assert!(SessionStore::new("../bad", Environment::Prod).is_err());
    }

    #[test]
    fn rejects_corrupt_future_and_wrong_environment_records() {
        let store = SessionStore::new("default", Environment::Prod).unwrap();
        let mut record = serde_json::json!({"version":1,"session_id":"test-token","user":{"email":"test@example.com"},"environment":"PROD","saved_at_unix":42});
        assert_eq!(
            store
                .decode(&record.to_string())
                .unwrap()
                .session()
                .session_id(),
            "test-token"
        );
        assert!(store.decode("secret invalid data").is_err());
        record["version"] = 2.into();
        assert!(store.decode(&record.to_string()).is_err());
        record["version"] = 1.into();
        record["environment"] = "STAGE".into();
        assert!(store.decode(&record.to_string()).is_err());
    }
}
