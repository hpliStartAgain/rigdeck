//! 共享文本文件的边界块编解码器。
//!
//! 编解码器只替换 RigDeck 明确拥有的块，块外字节保持原样。实现按原始字节偏移
//! 操作，而不是先把整份文档格式化一遍，因此能够保留 BOM、CRLF 和用户排版。

use rigdeck_adapter_sdk::{AdapterError, AdapterErrorCode, AdapterResult};

const START_PREFIX: &str = "<!-- rigdeck:start asset=";
const END_PREFIX: &str = "<!-- rigdeck:end asset=";
const SUFFIX: &str = " -->";

/// 托管块的结构状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedBlockState {
    /// 没有该资产的块。
    Absent,
    /// 找到且边界完整。
    Present {
        /// 块中记录的修订 hash。
        revision: String,
    },
}

/// `validate_managed_document` 的细粒度诊断，供调用方区分
/// duplicated / moved / damaged。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedBlockIssue {
    /// 同一资产出现多个块。
    Duplicated,
    /// 块边界损坏、嵌套或未配对。
    Damaged,
}

/// 验证文档中所有 RigDeck 块都具有唯一且配对的边界，返回细粒度诊断。
pub fn classify_managed_document(input: &[u8]) -> Result<(), ManagedBlockIssue> {
    let blocks = parse_blocks(input).map_err(|_| ManagedBlockIssue::Damaged)?;
    let mut assets = std::collections::BTreeSet::new();
    for block in blocks {
        if !assets.insert(block.asset.clone()) {
            return Err(ManagedBlockIssue::Duplicated);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct BlockRange {
    asset: String,
    revision: String,
    start: usize,
    end: usize,
}

/// 检查指定资产块是否存在，并拒绝嵌套、重复或损坏的边界。
pub fn inspect_managed_block(input: &[u8], asset_id: &str) -> AdapterResult<ManagedBlockState> {
    let blocks = parse_blocks(input)?;
    let matches: Vec<_> = blocks
        .iter()
        .filter(|block| block.asset == asset_id)
        .collect();
    match matches.as_slice() {
        [] => Ok(ManagedBlockState::Absent),
        [block] => Ok(ManagedBlockState::Present {
            revision: block.revision.clone(),
        }),
        _ => Err(damaged(format!("资产 {asset_id} 出现重复托管块"))),
    }
}

/// 创建或替换一个托管块。
pub fn upsert_managed_block(
    input: &[u8],
    asset_id: &str,
    revision: &str,
    content: &[u8],
) -> AdapterResult<Vec<u8>> {
    validate_marker_value(asset_id, "asset ID")?;
    validate_marker_value(revision, "revision")?;
    std::str::from_utf8(content).map_err(|_| {
        AdapterError::new(AdapterErrorCode::ValidationFailed, "托管文本块必须是 UTF-8")
    })?;

    let blocks = parse_blocks(input)?;
    let matching: Vec<_> = blocks
        .iter()
        .filter(|block| block.asset == asset_id)
        .collect();
    if matching.len() > 1 {
        return Err(damaged(format!("资产 {asset_id} 出现重复托管块")));
    }

    let newline = preferred_newline(input);
    let rendered = render_block(asset_id, revision, content, newline);
    if let Some(existing) = matching.first() {
        let mut output =
            Vec::with_capacity(input.len() - (existing.end - existing.start) + rendered.len());
        output.extend_from_slice(&input[..existing.start]);
        output.extend_from_slice(&rendered);
        output.extend_from_slice(&input[existing.end..]);
        return Ok(output);
    }

    let mut output = input.to_vec();
    if !output.is_empty() && !output.ends_with(b"\n") && !output.ends_with(b"\r") {
        output.extend_from_slice(newline);
    }
    output.extend_from_slice(&rendered);
    Ok(output)
}

/// 只删除指定资产的托管块；块不存在时保持幂等。
pub fn remove_managed_block(input: &[u8], asset_id: &str) -> AdapterResult<Vec<u8>> {
    let blocks = parse_blocks(input)?;
    let matching: Vec<_> = blocks
        .iter()
        .filter(|block| block.asset == asset_id)
        .collect();
    match matching.as_slice() {
        [] => Ok(input.to_vec()),
        [block] => {
            let mut output = Vec::with_capacity(input.len() - (block.end - block.start));
            output.extend_from_slice(&input[..block.start]);
            output.extend_from_slice(&input[block.end..]);
            Ok(output)
        }
        _ => Err(damaged(format!("资产 {asset_id} 出现重复托管块"))),
    }
}

/// 文本中是否包含 RigDeck marker。该函数只用于扫描展示，严格验证仍应调用
/// [`inspect_managed_block`]。
pub fn contains_managed_marker(input: &[u8]) -> bool {
    input
        .windows(START_PREFIX.len())
        .any(|window| window == START_PREFIX.as_bytes())
}

/// 验证文档中所有 RigDeck 块都具有唯一且配对的边界。
pub fn validate_managed_document(input: &[u8]) -> AdapterResult<()> {
    let blocks = parse_blocks(input)?;
    let mut assets = std::collections::BTreeSet::new();
    for block in blocks {
        if !assets.insert(block.asset.clone()) {
            return Err(damaged(format!("资产 {} 出现重复托管块", block.asset)));
        }
    }
    Ok(())
}

fn parse_blocks(input: &[u8]) -> AdapterResult<Vec<BlockRange>> {
    let text = std::str::from_utf8(input).map_err(|_| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            "包含托管块的共享文本必须是 UTF-8",
        )
    })?;
    let bom_len = usize::from(text.starts_with('\u{feff}')) * 3;
    let mut open: Option<(String, String, usize)> = None;
    let mut blocks = Vec::new();

    for (start, end, line) in lines_with_offsets(input, bom_len) {
        let trimmed = trim_line_ending(line);
        if let Some(marker) = std::str::from_utf8(trimmed)
            .ok()
            .and_then(parse_start_marker)
        {
            if open.is_some() {
                return Err(damaged("托管块不能嵌套"));
            }
            open = Some((marker.0, marker.1, start));
        } else if let Some(asset) = std::str::from_utf8(trimmed).ok().and_then(parse_end_marker) {
            let Some((open_asset, revision, block_start)) = open.take() else {
                return Err(damaged("发现没有起始 marker 的结束 marker"));
            };
            if open_asset != asset {
                return Err(damaged(format!(
                    "托管块首尾资产不一致：{open_asset} / {asset}"
                )));
            }
            blocks.push(BlockRange {
                asset,
                revision,
                start: block_start,
                end,
            });
        }
    }
    if let Some((asset, _, _)) = open {
        return Err(damaged(format!("资产 {asset} 的托管块缺少结束 marker")));
    }
    Ok(blocks)
}

fn lines_with_offsets(input: &[u8], first_start: usize) -> Vec<(usize, usize, &[u8])> {
    let mut lines = Vec::new();
    let mut start = first_start.min(input.len());
    for (index, byte) in input.iter().enumerate().skip(start) {
        if *byte == b'\n' {
            let end = index + 1;
            lines.push((start, end, &input[start..end]));
            start = end;
        }
    }
    if start < input.len() {
        lines.push((start, input.len(), &input[start..]));
    }
    lines
}

fn trim_line_ending(mut line: &[u8]) -> &[u8] {
    if line.ends_with(b"\n") {
        line = &line[..line.len() - 1];
    }
    if line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    line
}

fn parse_start_marker(line: &str) -> Option<(String, String)> {
    let body = line.strip_prefix(START_PREFIX)?.strip_suffix(SUFFIX)?;
    let (asset, revision) = body.split_once(" revision=")?;
    if asset.is_empty() || revision.is_empty() {
        return None;
    }
    Some((asset.to_owned(), revision.to_owned()))
}

fn parse_end_marker(line: &str) -> Option<String> {
    let asset = line.strip_prefix(END_PREFIX)?.strip_suffix(SUFFIX)?;
    (!asset.is_empty()).then(|| asset.to_owned())
}

fn render_block(asset_id: &str, revision: &str, content: &[u8], newline: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(content.len() + asset_id.len() * 2 + revision.len() + 80);
    output.extend_from_slice(
        format!("{START_PREFIX}{asset_id} revision={revision}{SUFFIX}").as_bytes(),
    );
    output.extend_from_slice(newline);
    output.extend_from_slice(content);
    if !content.is_empty() && !content.ends_with(b"\n") && !content.ends_with(b"\r") {
        output.extend_from_slice(newline);
    }
    output.extend_from_slice(format!("{END_PREFIX}{asset_id}{SUFFIX}").as_bytes());
    output.extend_from_slice(newline);
    output
}

fn preferred_newline(input: &[u8]) -> &'static [u8] {
    if input.windows(2).any(|window| window == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    }
}

fn validate_marker_value(value: &str, label: &str) -> AdapterResult<()> {
    if value.is_empty() || value.contains(['\r', '\n', '<', '>']) {
        return Err(AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("{label} 不能破坏托管 marker"),
        ));
    }
    Ok(())
}

fn damaged(message: impl Into<String>) -> AdapterError {
    AdapterError::new(AdapterErrorCode::ValidationFailed, message)
        .with_recovery("将损坏块作为冲突展示；用户确认前不要覆盖共享文件")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_preserves_bom_crlf_and_outside_bytes() {
        let original = b"\xef\xbb\xbf# user\r\nkeep = true\r\n";
        let result = upsert_managed_block(original, "asset-1", "rev-a", b"hello\nworld").unwrap();
        assert!(result.starts_with(original));
        assert!(result.windows(2).any(|window| window == b"\r\n"));
        assert_eq!(
            inspect_managed_block(&result, "asset-1").unwrap(),
            ManagedBlockState::Present {
                revision: "rev-a".to_owned()
            }
        );
    }

    #[test]
    fn update_and_remove_touch_only_owned_block() {
        let with_a = upsert_managed_block(b"user\n", "a", "1", b"old").unwrap();
        let with_b = upsert_managed_block(&with_a, "b", "1", b"other").unwrap();
        let updated = upsert_managed_block(&with_b, "a", "2", b"new").unwrap();
        assert!(String::from_utf8_lossy(&updated).contains("other"));
        let removed = remove_managed_block(&updated, "a").unwrap();
        assert!(String::from_utf8_lossy(&removed).contains("user\n"));
        assert!(String::from_utf8_lossy(&removed).contains("asset=b"));
        assert!(!String::from_utf8_lossy(&removed).contains("asset=a"));
    }

    #[test]
    fn damaged_or_duplicate_blocks_fail_closed() {
        let missing_end = b"<!-- rigdeck:start asset=a revision=1 -->\nvalue\n";
        assert!(upsert_managed_block(missing_end, "a", "2", b"new").is_err());

        let once = upsert_managed_block(b"", "a", "1", b"x").unwrap();
        let mut twice = once.clone();
        twice.extend_from_slice(&once);
        assert!(remove_managed_block(&twice, "a").is_err());
    }
}
