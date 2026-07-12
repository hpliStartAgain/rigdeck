//! 系统钥匙串与进程内 secret 值。

use std::{collections::BTreeMap, sync::Mutex};

use rigdeck_core::SecretRef;

/// Secret 操作结果。
pub type SecretResult<T> = Result<T, SecretError>;

/// Secret vault 错误。
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// 引用不存在。
    #[error("SecretRef 不存在：{0}")]
    NotFound(String),
    /// 系统钥匙串不可用或拒绝访问。
    #[error("系统钥匙串错误：{0}")]
    Keyring(String),
    /// 内存锁被 panic 污染。
    #[error("内存 secret vault 锁不可用")]
    LockPoisoned,
}

/// 仅在进程内存在的 secret 字节。
pub struct SecretValue(Vec<u8>);

impl SecretValue {
    /// 从字节创建 secret；调用者把所有权移动进该类型。
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// 借用 secret 字节。
    ///
    /// 返回借用切片而不是复制，调用结束后借用自动失效；调用方仍需避免把切片转成
    /// 可日志记录的 String。
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretValue(***)")
    }
}

impl Clone for SecretValue {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// Secret 存储契约。
pub trait SecretVault: Send + Sync {
    /// 保存/替换值。
    fn put(&self, reference: &SecretRef, value: &SecretValue) -> SecretResult<()>;
    /// 读取值。
    fn get(&self, reference: &SecretRef) -> SecretResult<SecretValue>;
    /// 删除值；不存在时仍成功，保证卸载幂等。
    fn delete(&self, reference: &SecretRef) -> SecretResult<()>;
}

/// Windows Credential Manager / macOS Keychain 的原生 vault。
#[derive(Debug, Clone)]
pub struct NativeSecretVault {
    service: String,
}

impl NativeSecretVault {
    /// 创建 RigDeck 原生 vault。
    pub fn new() -> Self {
        Self {
            service: "app.rigdeck".to_owned(),
        }
    }

    fn entry(&self, reference: &SecretRef) -> SecretResult<keyring::Entry> {
        keyring::Entry::new(&self.service, reference.as_str())
            .map_err(|error| SecretError::Keyring(error.to_string()))
    }
}

impl Default for NativeSecretVault {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretVault for NativeSecretVault {
    fn put(&self, reference: &SecretRef, value: &SecretValue) -> SecretResult<()> {
        self.entry(reference)?
            .set_secret(value.expose())
            .map_err(|error| SecretError::Keyring(error.to_string()))
    }

    fn get(&self, reference: &SecretRef) -> SecretResult<SecretValue> {
        self.entry(reference)?
            .get_secret()
            .map(SecretValue::new)
            .map_err(|error| match error {
                keyring::Error::NoEntry => SecretError::NotFound(reference.as_str().to_owned()),
                other => SecretError::Keyring(other.to_string()),
            })
    }

    fn delete(&self, reference: &SecretRef) -> SecretResult<()> {
        match self.entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(SecretError::Keyring(error.to_string())),
        }
    }
}

/// 仅用于测试/预览的内存 vault；绝不能作为生产 fallback。
#[derive(Debug, Default)]
pub struct InMemorySecretVault {
    values: Mutex<BTreeMap<String, SecretValue>>,
}

impl SecretVault for InMemorySecretVault {
    fn put(&self, reference: &SecretRef, value: &SecretValue) -> SecretResult<()> {
        self.values
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .insert(reference.as_str().to_owned(), value.clone());
        Ok(())
    }

    fn get(&self, reference: &SecretRef) -> SecretResult<SecretValue> {
        self.values
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .get(reference.as_str())
            .cloned()
            .ok_or_else(|| SecretError::NotFound(reference.as_str().to_owned()))
    }

    fn delete(&self, reference: &SecretRef) -> SecretResult<()> {
        self.values
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .remove(reference.as_str());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_vault_round_trip_and_idempotent_delete() {
        let vault = InMemorySecretVault::default();
        let reference = SecretRef::new("keychain:test").unwrap();
        let value = SecretValue::new(b"sensitive".to_vec());
        vault.put(&reference, &value).unwrap();
        assert_eq!(vault.get(&reference).unwrap().expose(), b"sensitive");
        vault.delete(&reference).unwrap();
        vault.delete(&reference).unwrap();
        assert!(matches!(
            vault.get(&reference),
            Err(SecretError::NotFound(_))
        ));
    }

    #[test]
    fn debug_never_exposes_secret() {
        let value = SecretValue::new(b"sensitive".to_vec());
        assert_eq!(format!("{value:?}"), "SecretValue(***)");
    }
}
