//! Blake3 内容寻址、ChaCha20-Poly1305 加密对象库。

use std::{fs, io::Write};

use camino::{Utf8Path, Utf8PathBuf};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit, Nonce,
};
use rigdeck_core::{ContentHash, ContentStore, CoreError, CoreResult};
use tempfile::NamedTempFile;

use crate::{StoreError, StoreResult};

/// 对象库 256-bit 加密密钥。
///
/// 生产环境从系统钥匙串读取；该值不得写入 SQLite、日志或导出。`Drop` 在值离开
/// 作用域时覆盖内存，降低普通内存转储中的残留时间（不宣称抵御已失陷主机）。
#[derive(Clone)]
pub struct ObjectKey([u8; 32]);

impl ObjectKey {
    /// 从 32 字节密钥创建。
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl std::fmt::Debug for ObjectKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ObjectKey(***)")
    }
}

impl Drop for ObjectKey {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// 内容寻址对象库。
#[derive(Debug, Clone)]
pub struct ObjectStore {
    root: Utf8PathBuf,
    key: ObjectKey,
}

impl ObjectStore {
    /// 创建/打开对象库根目录。
    pub fn open(root: impl Into<Utf8PathBuf>, key: ObjectKey) -> StoreResult<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("objects")).map_err(|error| StoreError::io(&root, error))?;
        Ok(Self { root, key })
    }

    /// 对象库根目录。
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// 加密写入对象；相同内容复用同一路径。
    pub fn put_bytes(&self, bytes: &[u8]) -> StoreResult<ContentHash> {
        let hash = ContentHash::from_bytes(bytes);
        let path = self.path_for(&hash);
        if path.exists() {
            let existing = self.get_bytes(&hash)?;
            if existing != bytes {
                return Err(StoreError::Object(format!(
                    "对象路径已存在但内容不同：{hash}"
                )));
            }
            return Ok(hash);
        }

        let parent = path
            .parent()
            .ok_or_else(|| StoreError::Object(format!("对象路径没有父目录：{path}")))?;
        fs::create_dir_all(parent).map_err(|error| StoreError::io(parent, error))?;
        let encrypted = self.seal(&hash, bytes)?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|error| StoreError::io(parent, error))?;
        temporary
            .write_all(&encrypted)
            .map_err(|error| StoreError::io(temporary.path(), error))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| StoreError::io(temporary.path(), error))?;

        // persist_noclobber 避免并发 writer 覆盖已经完成的相同对象。若另一进程先
        // 创建成功，重新解密并验证即可；对象的不可变性由 plaintext hash 保证。
        if let Err(error) = temporary.persist_noclobber(&path) {
            if path.exists() {
                let existing = self.get_bytes(&hash)?;
                if existing == bytes {
                    return Ok(hash);
                }
            }
            return Err(StoreError::io(&path, error.error));
        }
        Ok(hash)
    }

    /// 读取、认证、解密对象并重新计算 plaintext hash。
    pub fn get_bytes(&self, hash: &ContentHash) -> StoreResult<Vec<u8>> {
        let path = self.path_for(hash);
        let encrypted = fs::read(&path).map_err(|error| StoreError::io(&path, error))?;
        let bytes = self.open_object(hash, &encrypted)?;
        if ContentHash::from_bytes(&bytes) != *hash {
            return Err(StoreError::Object(format!("对象 hash 校验失败：{hash}")));
        }
        Ok(bytes)
    }

    /// 对象是否存在；存在不等于认证与完整性已经通过。
    pub fn contains_hash(&self, hash: &ContentHash) -> bool {
        self.path_for(hash).is_file()
    }

    /// 验证全部对象，返回对象数量。
    pub fn integrity_check(&self) -> StoreResult<usize> {
        let objects = self.root.join("objects");
        let mut count = 0;
        for shard in read_directories(&objects)? {
            for entry in read_files(&shard)? {
                let name = entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        StoreError::Object(format!("对象文件名不是 UTF-8：{}", entry.display()))
                    })?;
                let hash: ContentHash = name
                    .parse()
                    .map_err(|error: CoreError| StoreError::Object(error.to_string()))?;
                self.get_bytes(&hash)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// 返回 `objects/ab/<hash>` 分片路径。
    pub fn path_for(&self, hash: &ContentHash) -> Utf8PathBuf {
        self.root
            .join("objects")
            .join(&hash.as_str()[..2])
            .join(hash.as_str())
    }

    fn seal(&self, hash: &ContentHash, plaintext: &[u8]) -> StoreResult<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new((&self.key.0).into());
        cipher
            .encrypt(
                &nonce_for(&self.key, hash),
                Payload {
                    msg: plaintext,
                    aad: hash.as_str().as_bytes(),
                },
            )
            .map_err(|_| StoreError::Object(format!("对象加密失败：{hash}")))
    }

    fn open_object(&self, hash: &ContentHash, ciphertext: &[u8]) -> StoreResult<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new((&self.key.0).into());
        cipher
            .decrypt(
                &nonce_for(&self.key, hash),
                Payload {
                    msg: ciphertext,
                    aad: hash.as_str().as_bytes(),
                },
            )
            .map_err(|_| StoreError::Object(format!("对象认证/解密失败：{hash}")))
    }
}

impl ContentStore for ObjectStore {
    fn put(&self, bytes: &[u8]) -> CoreResult<ContentHash> {
        self.put_bytes(bytes)
            .map_err(|error| CoreError::ObjectUnavailable(error.to_string()))
    }

    fn get(&self, hash: &ContentHash) -> CoreResult<Vec<u8>> {
        self.get_bytes(hash)
            .map_err(|error| CoreError::ObjectUnavailable(error.to_string()))
    }

    fn contains(&self, hash: &ContentHash) -> CoreResult<bool> {
        Ok(self.contains_hash(hash))
    }
}

fn nonce_for(key: &ObjectKey, hash: &ContentHash) -> Nonce {
    // Nonce 由“对象库密钥 + 内容 hash”确定：同一密钥下，不同内容得到不同 nonce；
    // 同一内容只写同一个不可变对象，因此不会用相同 nonce 加密不同明文。
    let mut hasher = blake3::Hasher::new_keyed(&key.0);
    hasher.update(b"rigdeck-object-nonce-v1\0");
    hasher.update(hash.as_str().as_bytes());
    let digest = hasher.finalize();
    *Nonce::from_slice(&digest.as_bytes()[..12])
}

fn read_directories(root: &Utf8Path) -> StoreResult<Vec<Utf8PathBuf>> {
    let entries = fs::read_dir(root).map_err(|error| StoreError::io(root, error))?;
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| {
            Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| StoreError::Object(format!("路径不是 UTF-8：{}", path.display())))
        })
        .collect()
}

fn read_files(root: &Utf8Path) -> StoreResult<Vec<std::path::PathBuf>> {
    let entries = fs::read_dir(root).map_err(|error| StoreError::io(root, error))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_is_deduplicated() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let store = ObjectStore::open(root, ObjectKey::from_bytes([7; 32])).unwrap();
        let first = store.put_bytes(b"same").unwrap();
        let second = store.put_bytes(b"same").unwrap();
        assert_eq!(first, second);
        assert_eq!(store.integrity_check().unwrap(), 1);
    }

    #[test]
    fn corrupted_object_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let store = ObjectStore::open(root, ObjectKey::from_bytes([7; 32])).unwrap();
        let hash = store.put_bytes(b"original").unwrap();
        fs::write(store.path_for(&hash), b"tampered").unwrap();
        assert!(matches!(store.get_bytes(&hash), Err(StoreError::Object(_))));
    }

    #[test]
    fn object_file_never_contains_plaintext_secret() {
        let temp = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
        let store = ObjectStore::open(root, ObjectKey::from_bytes([9; 32])).unwrap();
        let secret = b"TEST_SECRET_DO_NOT_PERSIST";
        let hash = store.put_bytes(secret).unwrap();
        let on_disk = fs::read(store.path_for(&hash)).unwrap();
        assert!(!on_disk.windows(secret.len()).any(|window| window == secret));
        assert_eq!(store.get_bytes(&hash).unwrap(), secret);
    }
}
