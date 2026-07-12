//! 内容 hash 与文本规范化。

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{CoreError, CoreResult};

/// Blake3 内容 hash 的十六进制表示。
///
/// 使用 newtype（而不是裸 `String`）能让编译器阻止把任意字符串误传为 hash。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// 直接计算原始字节的 Blake3 hash。
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    /// 返回稳定的十六进制字符串。
    pub fn as_str(&self) -> &str {
        // `&str` 只是借用内部字符串，不复制数据，也不会转移 `self` 的所有权。
        &self.0
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ContentHash").field(&self.0).finish()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ContentHash {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(CoreError::InvalidInput(format!(
                "Blake3 hash 必须是 64 位十六进制字符串：{value}"
            )));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

/// 计算用于语义比较的规范化 hash。
///
/// UTF-8 文本会去掉 BOM，并把 CRLF/CR 统一为 LF；二进制数据保持原样。raw hash
/// 仍由 [`ContentHash::from_bytes`] 计算，因此换行和编码差异不会丢失。
pub fn normalized_hash(bytes: &[u8]) -> ContentHash {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let text = text.strip_prefix('\u{feff}').unwrap_or(text);
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            ContentHash::from_bytes(normalized.as_bytes())
        }
        Err(_) => ContentHash::from_bytes(bytes),
    }
}

/// 同时验证 raw/normalized hash 是否与内容一致。
pub fn verify_hashes(bytes: &[u8], raw: &ContentHash, normalized: &ContentHash) -> CoreResult<()> {
    if &ContentHash::from_bytes(bytes) != raw || &normalized_hash(bytes) != normalized {
        return Err(CoreError::VerificationFailed(
            "资产修订 hash 与内容不一致".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_hash_ignores_bom_and_line_endings() {
        let a = normalized_hash(b"hello\r\nworld\r\n");
        let b = normalized_hash("\u{feff}hello\nworld\n".as_bytes());
        assert_eq!(a, b);
        assert_ne!(
            ContentHash::from_bytes(b"hello\r\nworld\r\n"),
            ContentHash::from_bytes("\u{feff}hello\nworld\n".as_bytes())
        );
    }

    #[test]
    fn hash_parser_rejects_arbitrary_strings() {
        assert!("not-a-hash".parse::<ContentHash>().is_err());
    }
}
