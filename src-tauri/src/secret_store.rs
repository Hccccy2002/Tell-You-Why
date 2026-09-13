use std::fmt;
use thiserror::Error;

const SERVICE_PREFIX: &str = "com.tellyouwhy.desktop";
const USERNAME: &str = "api-key";

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("API Key 格式不完整，请重新复制完整 Key")]
    InvalidFormat,
    #[error("系统凭据库暂时不可用")]
    Unavailable,
    #[error("尚未配置 API Key")]
    NotFound,
}

pub trait SecretStore: Send + Sync {
    fn save(&self, credential_ref: &str, secret: &str) -> Result<(), SecretError>;
    fn get(&self, credential_ref: &str) -> Result<SecretValue, SecretError>;
    fn delete(&self, credential_ref: &str) -> Result<(), SecretError>;
}

pub struct SecretValue(String);

impl SecretValue {
    pub(crate) fn simulated() -> Self {
        Self(String::new())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    #[cfg(test)]
    pub fn for_test(value: &str) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Debug, Default)]
pub struct WindowsCredentialStore;

impl SecretStore for WindowsCredentialStore {
    fn save(&self, credential_ref: &str, secret: &str) -> Result<(), SecretError> {
        validate_secret(secret)?;
        let entry = entry(credential_ref)?;
        entry
            .set_password(secret)
            .map_err(|_| SecretError::Unavailable)
    }

    fn get(&self, credential_ref: &str) -> Result<SecretValue, SecretError> {
        let entry = entry(credential_ref)?;
        match entry.get_password() {
            Ok(value) => Ok(SecretValue(value)),
            Err(keyring::Error::NoEntry) => Err(SecretError::NotFound),
            Err(_) => Err(SecretError::Unavailable),
        }
    }

    fn delete(&self, credential_ref: &str) -> Result<(), SecretError> {
        let entry = entry(credential_ref)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(SecretError::Unavailable),
        }
    }
}

fn entry(credential_ref: &str) -> Result<keyring::Entry, SecretError> {
    if credential_ref.is_empty()
        || !credential_ref.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':')
        })
    {
        return Err(SecretError::InvalidFormat);
    }
    keyring::Entry::new(&format!("{SERVICE_PREFIX}.{credential_ref}"), USERNAME)
        .map_err(|_| SecretError::Unavailable)
}

fn validate_secret(secret: &str) -> Result<(), SecretError> {
    if !(8..=512).contains(&secret.len())
        || secret
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(SecretError::InvalidFormat);
    }
    Ok(())
}

pub fn credential_ref(provider_id: &str, region: &str) -> Result<String, SecretError> {
    let value = format!("{provider_id}:{region}");
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':'))
    {
        Ok(value)
    } else {
        Err(SecretError::InvalidFormat)
    }
}

pub fn last_four(secret: &str) -> String {
    secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[cfg(test)]
pub mod tests_support {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MemorySecretStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl SecretStore for MemorySecretStore {
        fn save(&self, credential_ref: &str, secret: &str) -> Result<(), SecretError> {
            validate_secret(secret)?;
            self.values
                .lock()
                .map_err(|_| SecretError::Unavailable)?
                .insert(credential_ref.into(), secret.into());
            Ok(())
        }

        fn get(&self, credential_ref: &str) -> Result<SecretValue, SecretError> {
            self.values
                .lock()
                .map_err(|_| SecretError::Unavailable)?
                .get(credential_ref)
                .cloned()
                .map(SecretValue)
                .ok_or(SecretError::NotFound)
        }

        fn delete(&self, credential_ref: &str) -> Result<(), SecretError> {
            self.values
                .lock()
                .map_err(|_| SecretError::Unavailable)?
                .remove(credential_ref);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tests_support::MemorySecretStore;

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = SecretValue("sk-test-value".into());
        assert_eq!(format!("{secret:?}"), "SecretValue([REDACTED])");
    }

    #[test]
    fn memory_store_round_trip_and_delete() {
        let store = MemorySecretStore::default();
        store
            .save("deepseek:default", "sk-test-secret")
            .expect("save should work");
        assert_eq!(
            store
                .get("deepseek:default")
                .expect("get should work")
                .expose(),
            "sk-test-secret"
        );
        store
            .delete("deepseek:default")
            .expect("delete should work");
        assert!(matches!(
            store.get("deepseek:default"),
            Err(SecretError::NotFound)
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "writes and removes one temporary mock credential in Windows Credential Manager"]
    fn windows_credential_store_round_trip_and_delete() {
        let store = WindowsCredentialStore;
        let credential_ref = format!("credential-smoke-{}", uuid::Uuid::new_v4());
        let mock_secret = "sk-mock-credential-smoke";

        store
            .save(&credential_ref, mock_secret)
            .expect("temporary mock credential should be saved");
        let loaded = store.get(&credential_ref);
        let deleted = store.delete(&credential_ref);

        assert_eq!(
            loaded
                .expect("temporary mock credential should be readable")
                .expose(),
            mock_secret
        );
        deleted.expect("temporary mock credential should be removed");
        assert!(matches!(
            store.get(&credential_ref),
            Err(SecretError::NotFound)
        ));
    }
}
