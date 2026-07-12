//! JSON/JSONC、TOML 与 YAML 的结构化条目补丁。
//!
//! 这些函数只取得 `section.entry` 的所有权。JSONC 使用轻量词法器定位原始字节，
//! TOML 使用保留装饰信息的 `toml_edit`，YAML 使用缩进边界定位；任何歧义都会停止
//! 写入，避免“解析成功但把整份用户配置重新格式化”的隐性数据损失。

use std::str::FromStr;

use rigdeck_adapter_sdk::{AdapterError, AdapterErrorCode, AdapterResult};
use serde_json::Value as JsonValue;
use toml_edit::{Array, DocumentMut, Item, Table, Value as TomlValue};

/// 新增或替换一个结构化配置条目。
pub fn upsert_structured_entry(
    input: &[u8],
    native_format: &str,
    section: &str,
    key: &str,
    json_payload: &[u8],
) -> AdapterResult<Vec<u8>> {
    validate_structured_key(section)?;
    validate_structured_key(key)?;
    let payload: JsonValue = serde_json::from_slice(json_payload).map_err(|error| {
        AdapterError::new(
            AdapterErrorCode::ValidationFailed,
            format!("结构化条目 payload 不是有效 JSON：{error}"),
        )
    })?;
    match native_format {
        "json" | "jsonc" => upsert_jsonc(input, section, key, &payload),
        "toml" => upsert_toml(input, section, key, &payload),
        "yaml" | "yml" => upsert_yaml(input, section, key, &payload),
        _ => Err(AdapterError::new(
            AdapterErrorCode::UnsupportedCapability,
            format!("没有 {native_format} 结构化补丁器"),
        )),
    }
}

/// 删除一个结构化配置条目；条目不存在时保持幂等。
pub fn remove_structured_entry(
    input: &[u8],
    native_format: &str,
    section: &str,
    key: &str,
) -> AdapterResult<Vec<u8>> {
    validate_structured_key(section)?;
    validate_structured_key(key)?;
    match native_format {
        "json" | "jsonc" => remove_jsonc(input, section, key),
        "toml" => remove_toml(input, section, key),
        "yaml" | "yml" => remove_yaml(input, section, key),
        _ => Err(AdapterError::new(
            AdapterErrorCode::UnsupportedCapability,
            format!("没有 {native_format} 结构化补丁器"),
        )),
    }
}

/// 判断结构化条目是否存在。解析失败不是“不存在”，而是显式错误。
pub fn structured_entry_exists(
    input: &[u8],
    native_format: &str,
    section: &str,
    key: &str,
) -> AdapterResult<bool> {
    match native_format {
        "json" | "jsonc" => {
            let parsed = JsoncDocument::parse(input)?;
            let Some(section_member) = parsed.member(0, section)? else {
                return Ok(false);
            };
            let object = parsed.object_at_value(&section_member)?;
            Ok(parsed.member(object, key)?.is_some())
        }
        "toml" => {
            let text = utf8(input, "TOML")?;
            let document = DocumentMut::from_str(text).map_err(toml_error)?;
            Ok(document
                .get(section)
                .and_then(Item::as_table_like)
                .is_some_and(|table| table.contains_key(key)))
        }
        "yaml" | "yml" => Ok(find_yaml_entry(input, section, key)?.is_some()),
        _ => Err(AdapterError::new(
            AdapterErrorCode::UnsupportedCapability,
            format!("没有 {native_format} 结构化补丁器"),
        )),
    }
}

fn upsert_jsonc(
    input: &[u8],
    section: &str,
    key: &str,
    payload: &JsonValue,
) -> AdapterResult<Vec<u8>> {
    let seed = if input.iter().all(u8::is_ascii_whitespace) {
        b"{}\n".as_slice()
    } else {
        input
    };
    let document = JsoncDocument::parse(seed)?;
    let payload = serde_json::to_string_pretty(payload).map_err(|error| {
        AdapterError::new(AdapterErrorCode::ValidationFailed, error.to_string())
    })?;
    let output = if let Some(section_member) = document.member(0, section)? {
        let section_object = document.object_at_value(&section_member)?;
        if let Some(entry) = document.member(section_object, key)? {
            replace_range(
                seed,
                document.tokens[entry.value_start].start,
                document.tokens[entry.value_end - 1].end,
                payload.as_bytes(),
            )
        } else {
            document.insert_member(seed, section_object, key, &payload)?
        }
    } else {
        let nested = format!(
            "{{\n  {}: {}\n}}",
            serde_json::to_string(key).unwrap_or_default(),
            indent_multiline(&payload, "  ")
        );
        document.insert_member(seed, 0, section, &nested)?
    };
    JsoncDocument::parse(&output)?;
    Ok(output)
}

fn remove_jsonc(input: &[u8], section: &str, key: &str) -> AdapterResult<Vec<u8>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let document = JsoncDocument::parse(input)?;
    let Some(section_member) = document.member(0, section)? else {
        return Ok(input.to_vec());
    };
    let section_object = document.object_at_value(&section_member)?;
    let Some(entry) = document.member(section_object, key)? else {
        return Ok(input.to_vec());
    };
    let output = document.remove_member(input, section_object, &entry)?;
    JsoncDocument::parse(&output)?;
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
    Comma,
    String(String),
    Primitive,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct JsonMember {
    key: String,
    key_token: usize,
    value_start: usize,
    value_end: usize,
}

struct JsoncDocument {
    tokens: Vec<Token>,
}

impl JsoncDocument {
    fn parse(input: &[u8]) -> AdapterResult<Self> {
        let text = utf8(input, "JSONC")?;
        let tokens = lex_jsonc(text)?;
        if tokens.is_empty() {
            return Err(structured_error("JSONC 根对象为空"));
        }
        let document = Self { tokens };
        let end = document.parse_value(0)?;
        if end != document.tokens.len() || !matches!(document.tokens[0].kind, TokenKind::LeftBrace)
        {
            return Err(structured_error("JSONC 必须包含唯一根对象"));
        }
        Ok(document)
    }

    fn parse_value(&self, index: usize) -> AdapterResult<usize> {
        let token = self
            .tokens
            .get(index)
            .ok_or_else(|| structured_error("JSONC value 意外结束"))?;
        match token.kind {
            TokenKind::LeftBrace => self.parse_object(index).map(|value| value.0 + 1),
            TokenKind::LeftBracket => self.parse_array(index),
            TokenKind::String(_) | TokenKind::Primitive => Ok(index + 1),
            _ => Err(structured_error("JSONC value 起始 token 无效")),
        }
    }

    fn parse_array(&self, open: usize) -> AdapterResult<usize> {
        let mut index = open + 1;
        if matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::RightBracket)
        ) {
            return Ok(index + 1);
        }
        loop {
            index = self.parse_value(index)?;
            match self.tokens.get(index).map(|token| &token.kind) {
                Some(TokenKind::Comma) => {
                    index += 1;
                    if matches!(
                        self.tokens.get(index).map(|token| &token.kind),
                        Some(TokenKind::RightBracket)
                    ) {
                        return Ok(index + 1);
                    }
                }
                Some(TokenKind::RightBracket) => return Ok(index + 1),
                _ => return Err(structured_error("JSONC array 缺少逗号或右括号")),
            }
        }
    }

    fn parse_object(&self, open: usize) -> AdapterResult<(usize, Vec<JsonMember>)> {
        if !matches!(self.tokens[open].kind, TokenKind::LeftBrace) {
            return Err(structured_error("期望 JSONC object"));
        }
        let mut index = open + 1;
        let mut members = Vec::new();
        if matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::RightBrace)
        ) {
            return Ok((index, members));
        }
        loop {
            let key_token = index;
            let key = match self.tokens.get(index).map(|token| &token.kind) {
                Some(TokenKind::String(value)) => value.clone(),
                _ => return Err(structured_error("JSONC object key 必须是双引号字符串")),
            };
            index += 1;
            if !matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Colon)
            ) {
                return Err(structured_error("JSONC object key 后缺少冒号"));
            }
            index += 1;
            let value_start = index;
            let value_end = self.parse_value(index)?;
            members.push(JsonMember {
                key,
                key_token,
                value_start,
                value_end,
            });
            index = value_end;
            match self.tokens.get(index).map(|token| &token.kind) {
                Some(TokenKind::Comma) => {
                    index += 1;
                    if matches!(
                        self.tokens.get(index).map(|token| &token.kind),
                        Some(TokenKind::RightBrace)
                    ) {
                        return Ok((index, members));
                    }
                }
                Some(TokenKind::RightBrace) => return Ok((index, members)),
                _ => return Err(structured_error("JSONC object 缺少逗号或右括号")),
            }
        }
    }

    fn member(&self, open: usize, key: &str) -> AdapterResult<Option<JsonMember>> {
        let (_, members) = self.parse_object(open)?;
        let matches: Vec<_> = members
            .into_iter()
            .filter(|member| member.key == key)
            .collect();
        match matches.as_slice() {
            [] => Ok(None),
            [member] => Ok(Some(member.clone())),
            _ => Err(structured_error(format!("JSONC key 重复：{key}"))),
        }
    }

    fn object_at_value(&self, member: &JsonMember) -> AdapterResult<usize> {
        let token = &self.tokens[member.value_start];
        if matches!(token.kind, TokenKind::LeftBrace) {
            Ok(member.value_start)
        } else {
            Err(structured_error(format!(
                "JSONC section {} 不是对象",
                member.key
            )))
        }
    }

    fn insert_member(
        &self,
        input: &[u8],
        object_open: usize,
        key: &str,
        payload: &str,
    ) -> AdapterResult<Vec<u8>> {
        let (close_token, members) = self.parse_object(object_open)?;
        let close_start = self.tokens[close_token].start;
        let insert_at = indentation_start(input, close_start);
        let close_indent = std::str::from_utf8(&input[insert_at..close_start]).unwrap_or("");
        let child_indent = members
            .first()
            .map(|member| {
                let start = indentation_start(input, self.tokens[member.key_token].start);
                std::str::from_utf8(&input[start..self.tokens[member.key_token].start])
                    .unwrap_or("")
                    .to_owned()
            })
            .unwrap_or_else(|| format!("{close_indent}  "));
        let newline = preferred_newline(input);
        let key = serde_json::to_string(key).map_err(|error| {
            AdapterError::new(AdapterErrorCode::ValidationFailed, error.to_string())
        })?;
        let member_text = format!(
            "{child_indent}{key}: {}",
            indent_multiline(payload, &child_indent)
        );

        let comma_at = members.last().and_then(|member| {
            let next = member.value_end;
            (!matches!(
                self.tokens.get(next).map(|token| &token.kind),
                Some(TokenKind::Comma)
            ))
            .then_some(self.tokens[member.value_end - 1].end)
        });
        let mut output = Vec::with_capacity(input.len() + member_text.len() + 8);
        let split = comma_at.unwrap_or(insert_at);
        output.extend_from_slice(&input[..split]);
        if comma_at.is_some() {
            output.push(b',');
        }
        output.extend_from_slice(&input[split..insert_at]);
        if !output.ends_with(b"\n") && !output.ends_with(b"\r") {
            output.extend_from_slice(newline);
        }
        output.extend_from_slice(member_text.as_bytes());
        output.extend_from_slice(newline);
        output.extend_from_slice(&input[insert_at..]);
        Ok(output)
    }

    fn remove_member(
        &self,
        input: &[u8],
        object_open: usize,
        member: &JsonMember,
    ) -> AdapterResult<Vec<u8>> {
        let (_, members) = self.parse_object(object_open)?;
        let position = members
            .iter()
            .position(|candidate| candidate.key_token == member.key_token)
            .ok_or_else(|| structured_error("JSONC member 定位失败"))?;
        let start = indentation_start(input, self.tokens[member.key_token].start);
        let next_token = member.value_end;
        let (end, remove_previous_comma) = if matches!(
            self.tokens.get(next_token).map(|token| &token.kind),
            Some(TokenKind::Comma)
        ) {
            (line_end(input, self.tokens[next_token].end), None)
        } else {
            let previous = position.checked_sub(1).map(|previous| &members[previous]);
            let comma = previous.and_then(|previous| {
                self.tokens.get(previous.value_end).and_then(|token| {
                    matches!(token.kind, TokenKind::Comma).then_some((token.start, token.end))
                })
            });
            (
                line_end(input, self.tokens[member.value_end - 1].end),
                comma,
            )
        };
        let mut output = Vec::with_capacity(input.len() - (end - start));
        if let Some((comma_start, comma_end)) = remove_previous_comma {
            output.extend_from_slice(&input[..comma_start]);
            output.extend_from_slice(&input[comma_end..start]);
        } else {
            output.extend_from_slice(&input[..start]);
        }
        output.extend_from_slice(&input[end..]);
        Ok(output)
    }
}

fn lex_jsonc(input: &str) -> AdapterResult<Vec<Token>> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut index = usize::from(input.starts_with('\u{feff}')) * 3;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let start = index;
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 >= bytes.len() {
                    return Err(structured_error(format!(
                        "JSONC 块注释未闭合，起始偏移 {start}"
                    )));
                }
                index += 2;
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                let kind = match bytes[index] {
                    b'{' => TokenKind::LeftBrace,
                    b'}' => TokenKind::RightBrace,
                    b'[' => TokenKind::LeftBracket,
                    b']' => TokenKind::RightBracket,
                    b':' => TokenKind::Colon,
                    b',' => TokenKind::Comma,
                    _ => unreachable!(),
                };
                tokens.push(Token {
                    kind,
                    start: index,
                    end: index + 1,
                });
                index += 1;
            }
            b'"' => {
                let start = index;
                index += 1;
                let mut escaped = false;
                while index < bytes.len() {
                    if escaped {
                        escaped = false;
                    } else if bytes[index] == b'\\' {
                        escaped = true;
                    } else if bytes[index] == b'"' {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                if !input[start..index.min(input.len())].ends_with('"') {
                    return Err(structured_error("JSONC 字符串未闭合"));
                }
                let raw = &input[start..index];
                let value: String = serde_json::from_str(raw)
                    .map_err(|error| structured_error(format!("JSONC 字符串无效：{error}")))?;
                tokens.push(Token {
                    kind: TokenKind::String(value),
                    start,
                    end: index,
                });
            }
            b'/' => return Err(structured_error("JSONC 中出现无效 `/`")),
            _ => {
                let start = index;
                while index < bytes.len()
                    && !bytes[index].is_ascii_whitespace()
                    && !matches!(bytes[index], b'{' | b'}' | b'[' | b']' | b':' | b',' | b'/')
                {
                    index += 1;
                }
                if start == index {
                    return Err(structured_error("JSONC primitive 无效"));
                }
                let primitive = &input[start..index];
                if serde_json::from_str::<JsonValue>(primitive).is_err() {
                    return Err(structured_error(format!(
                        "JSONC primitive 无效：{primitive}"
                    )));
                }
                tokens.push(Token {
                    kind: TokenKind::Primitive,
                    start,
                    end: index,
                });
            }
        }
    }
    Ok(tokens)
}

fn upsert_toml(
    input: &[u8],
    section: &str,
    key: &str,
    payload: &JsonValue,
) -> AdapterResult<Vec<u8>> {
    let text = utf8(input, "TOML")?;
    let mut document = if text.trim().is_empty() {
        DocumentMut::new()
    } else {
        DocumentMut::from_str(text).map_err(toml_error)?
    };
    if !document.contains_key(section) {
        document[section] = Item::Table(Table::new());
    }
    let table = document[section]
        .as_table_mut()
        .ok_or_else(|| structured_error(format!("TOML section {section} 不是 table")))?;
    table.insert(key, json_to_toml_item(payload)?);
    Ok(document.to_string().into_bytes())
}

fn remove_toml(input: &[u8], section: &str, key: &str) -> AdapterResult<Vec<u8>> {
    let text = utf8(input, "TOML")?;
    if text.trim().is_empty() {
        return Ok(input.to_vec());
    }
    let mut document = DocumentMut::from_str(text).map_err(toml_error)?;
    if let Some(table) = document.get_mut(section).and_then(Item::as_table_like_mut) {
        table.remove(key);
    }
    Ok(document.to_string().into_bytes())
}

fn json_to_toml_item(value: &JsonValue) -> AdapterResult<Item> {
    match value {
        JsonValue::Object(values) => {
            let mut table = Table::new();
            for (key, value) in values {
                if !value.is_null() {
                    table.insert(key, json_to_toml_item(value)?);
                }
            }
            Ok(Item::Table(table))
        }
        JsonValue::Array(values) => {
            let mut array = Array::new();
            for value in values {
                array.push(json_to_toml_value(value)?);
            }
            Ok(Item::Value(TomlValue::Array(array)))
        }
        _ => Ok(Item::Value(json_to_toml_value(value)?)),
    }
}

fn json_to_toml_value(value: &JsonValue) -> AdapterResult<TomlValue> {
    match value {
        JsonValue::Bool(value) => Ok((*value).into()),
        JsonValue::String(value) => Ok(value.clone().into()),
        JsonValue::Number(value) if value.is_i64() => Ok(value.as_i64().unwrap_or_default().into()),
        JsonValue::Number(value) if value.is_f64() => Ok(value.as_f64().unwrap_or_default().into()),
        JsonValue::Array(values) => {
            let mut array = Array::new();
            for value in values {
                array.push(json_to_toml_value(value)?);
            }
            Ok(TomlValue::Array(array))
        }
        JsonValue::Null | JsonValue::Object(_) | JsonValue::Number(_) => {
            Err(structured_error("该 JSON 值无法无损表达为 TOML value"))
        }
    }
}

fn upsert_yaml(
    input: &[u8],
    section: &str,
    key: &str,
    payload: &JsonValue,
) -> AdapterResult<Vec<u8>> {
    let _ = utf8(input, "YAML")?;
    // 先用 serde_yaml 验证现有语义；真正写入仍按原字节缩进边界完成。
    if !input.iter().all(u8::is_ascii_whitespace) {
        serde_yaml::from_slice::<serde_yaml::Value>(input).map_err(yaml_error)?;
    }
    let rendered = render_yaml_entry(key, payload)?;
    if let Some(range) = find_yaml_entry(input, section, key)? {
        let output = replace_range(input, range.0, range.1, rendered.as_bytes());
        serde_yaml::from_slice::<serde_yaml::Value>(&output).map_err(yaml_error)?;
        return Ok(output);
    }

    let newline = preferred_newline(input);
    let section_range = find_yaml_section(input, section)?;
    let mut output = Vec::new();
    if let Some((_, end)) = section_range {
        output.extend_from_slice(&input[..end]);
        if !output.ends_with(b"\n") && !output.ends_with(b"\r") {
            output.extend_from_slice(newline);
        }
        output.extend_from_slice(rendered.as_bytes());
        output.extend_from_slice(&input[end..]);
    } else {
        output.extend_from_slice(input);
        if !output.is_empty() && !output.ends_with(b"\n") && !output.ends_with(b"\r") {
            output.extend_from_slice(newline);
        }
        output.extend_from_slice(format!("{section}:").as_bytes());
        output.extend_from_slice(newline);
        output.extend_from_slice(rendered.as_bytes());
    }
    serde_yaml::from_slice::<serde_yaml::Value>(&output).map_err(yaml_error)?;
    Ok(output)
}

fn remove_yaml(input: &[u8], section: &str, key: &str) -> AdapterResult<Vec<u8>> {
    let _ = utf8(input, "YAML")?;
    serde_yaml::from_slice::<serde_yaml::Value>(input).map_err(yaml_error)?;
    let Some((start, end)) = find_yaml_entry(input, section, key)? else {
        return Ok(input.to_vec());
    };
    let output = replace_range(input, start, end, b"");
    serde_yaml::from_slice::<serde_yaml::Value>(&output).map_err(yaml_error)?;
    Ok(output)
}

fn render_yaml_entry(key: &str, payload: &JsonValue) -> AdapterResult<String> {
    let yaml = serde_yaml::to_string(payload).map_err(yaml_error)?;
    let yaml = yaml.strip_prefix("---\n").unwrap_or(&yaml).trim_end();
    if matches!(payload, JsonValue::Object(_) | JsonValue::Array(_)) {
        let mut output = format!("  {key}:\n");
        for line in yaml.lines() {
            output.push_str("    ");
            output.push_str(line);
            output.push('\n');
        }
        Ok(output)
    } else {
        Ok(format!("  {key}: {yaml}\n"))
    }
}

fn find_yaml_entry(
    input: &[u8],
    section: &str,
    key: &str,
) -> AdapterResult<Option<(usize, usize)>> {
    let Some((section_start, section_end)) = find_yaml_section(input, section)? else {
        return Ok(None);
    };
    let target = format!("  {key}:");
    let lines = byte_lines(input);
    let mut found = Vec::new();
    for (index, (start, _, line)) in lines.iter().enumerate() {
        if *start <= section_start || *start >= section_end {
            continue;
        }
        let text = utf8(trim_line_ending(line), "YAML")?;
        if text == target || text.starts_with(&format!("{target} ")) {
            let mut end = lines[index].1;
            for (next_start, next_end, next_line) in lines.iter().skip(index + 1) {
                if *next_start >= section_end {
                    break;
                }
                let next = utf8(trim_line_ending(next_line), "YAML")?;
                if !next.trim().is_empty()
                    && !next.trim_start().starts_with('#')
                    && leading_spaces(next) <= 2
                {
                    break;
                }
                end = *next_end;
            }
            found.push((*start, end));
        }
    }
    match found.as_slice() {
        [] => Ok(None),
        [range] => Ok(Some(*range)),
        _ => Err(structured_error(format!("YAML key 重复：{section}.{key}"))),
    }
}

fn find_yaml_section(input: &[u8], section: &str) -> AdapterResult<Option<(usize, usize)>> {
    if input.is_empty() {
        return Ok(None);
    }
    let target = format!("{section}:");
    let lines = byte_lines(input);
    let mut matches = Vec::new();
    for (index, (start, _, line)) in lines.iter().enumerate() {
        let text = utf8(trim_line_ending(line), "YAML")?;
        if text == target || text.starts_with(&format!("{target} ")) {
            if leading_spaces(text) != 0 {
                continue;
            }
            let mut end = input.len();
            for (next_start, _, next_line) in lines.iter().skip(index + 1) {
                let next = utf8(trim_line_ending(next_line), "YAML")?;
                if !next.trim().is_empty()
                    && !next.trim_start().starts_with('#')
                    && leading_spaces(next) == 0
                {
                    end = *next_start;
                    break;
                }
            }
            matches.push((*start, end));
        }
    }
    match matches.as_slice() {
        [] => Ok(None),
        [range] => Ok(Some(*range)),
        _ => Err(structured_error(format!("YAML section 重复：{section}"))),
    }
}

fn validate_structured_key(value: &str) -> AdapterResult<()> {
    let valid = !value.trim().is_empty()
        && value.len() <= 256
        && !value.contains(['\0', '\r', '\n', '/', '\\']);
    if !valid {
        return Err(structured_error(format!("结构化 key 无效：{value}")));
    }
    Ok(())
}

fn replace_range(input: &[u8], start: usize, end: usize, replacement: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() - (end - start) + replacement.len());
    output.extend_from_slice(&input[..start]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&input[end..]);
    output
}

fn indent_multiline(value: &str, indent: &str) -> String {
    value.replace('\n', &format!("\n{indent}"))
}

fn indentation_start(input: &[u8], offset: usize) -> usize {
    input[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1)
}

fn line_end(input: &[u8], offset: usize) -> usize {
    input[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(input.len(), |index| offset + index + 1)
}

fn preferred_newline(input: &[u8]) -> &'static [u8] {
    if input.windows(2).any(|window| window == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    }
}

fn byte_lines(input: &[u8]) -> Vec<(usize, usize, &[u8])> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in input.iter().enumerate() {
        if *byte == b'\n' {
            lines.push((start, index + 1, &input[start..=index]));
            start = index + 1;
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

fn leading_spaces(value: &str) -> usize {
    value.bytes().take_while(|byte| *byte == b' ').count()
}

fn utf8<'a>(input: &'a [u8], format: &str) -> AdapterResult<&'a str> {
    std::str::from_utf8(input).map_err(|_| structured_error(format!("{format} 必须是 UTF-8")))
}

fn structured_error(message: impl Into<String>) -> AdapterError {
    AdapterError::new(AdapterErrorCode::ValidationFailed, message)
        .with_recovery("把目标配置标记为冲突并保留原文件；修复语法后重新生成计划")
}

fn toml_error(error: toml_edit::TomlError) -> AdapterError {
    structured_error(format!("TOML 解析失败：{error}"))
}

fn yaml_error(error: serde_yaml::Error) -> AdapterError {
    structured_error(format!("YAML 解析失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD: &[u8] = br#"{"command":"demo","args":["--safe"],"enabled":true}"#;

    #[test]
    fn jsonc_upsert_preserves_comments_unknown_keys_and_crlf() {
        let input = b"{\r\n  // user comment\r\n  \"theme\": \"dark\",\r\n  \"mcp\": {\r\n    \"other\": { \"enabled\": false }\r\n  }\r\n}\r\n";
        let output = upsert_structured_entry(input, "jsonc", "mcp", "demo", PAYLOAD).unwrap();
        let text = String::from_utf8(output.clone()).unwrap();
        assert!(text.contains("// user comment"));
        assert!(text.contains("\"theme\": \"dark\""));
        assert!(text.contains("\"other\""));
        assert!(text.contains("\"demo\""));
        assert!(output.windows(2).any(|window| window == b"\r\n"));

        let removed = remove_structured_entry(&output, "jsonc", "mcp", "demo").unwrap();
        let removed = String::from_utf8(removed).unwrap();
        assert!(removed.contains("// user comment"));
        assert!(removed.contains("\"other\""));
        assert!(!removed.contains("\"demo\""));
    }

    #[test]
    fn toml_upsert_preserves_comments_and_unknown_tables() {
        let input = b"# user comment\ntheme = \"dark\"\n\n[other]\nvalue = 1\n";
        let output =
            upsert_structured_entry(input, "toml", "mcp_servers", "demo", PAYLOAD).unwrap();
        let text = String::from_utf8(output.clone()).unwrap();
        assert!(text.contains("# user comment"));
        assert!(text.contains("[other]"));
        assert!(text.contains("[mcp_servers.demo]"));
        assert!(structured_entry_exists(&output, "toml", "mcp_servers", "demo").unwrap());

        let removed = remove_structured_entry(&output, "toml", "mcp_servers", "demo").unwrap();
        let removed = String::from_utf8(removed).unwrap();
        assert!(removed.contains("# user comment"));
        assert!(removed.contains("[other]"));
    }

    #[test]
    fn yaml_upsert_preserves_comments_and_unmanaged_entries() {
        let input = b"# user comment\ntheme: dark\nmcp_servers:\n  other:\n    command: keep\n";
        let output =
            upsert_structured_entry(input, "yaml", "mcp_servers", "demo", PAYLOAD).unwrap();
        let text = String::from_utf8(output.clone()).unwrap();
        assert!(text.contains("# user comment"));
        assert!(text.contains("command: keep"));
        assert!(text.contains("  demo:"));
        assert!(structured_entry_exists(&output, "yaml", "mcp_servers", "demo").unwrap());

        let removed = remove_structured_entry(&output, "yaml", "mcp_servers", "demo").unwrap();
        let removed = String::from_utf8(removed).unwrap();
        assert!(removed.contains("command: keep"));
        assert!(!removed.contains("  demo:"));
    }

    #[test]
    fn malformed_or_duplicate_structures_fail_closed() {
        assert!(upsert_structured_entry(b"{", "jsonc", "mcp", "demo", PAYLOAD).is_err());
        assert!(upsert_structured_entry(
            b"{\"mcp\":{},\"mcp\":{}}",
            "jsonc",
            "mcp",
            "demo",
            PAYLOAD
        )
        .is_err());
        assert!(upsert_structured_entry(
            b"mcp_servers: []\n",
            "yaml",
            "mcp_servers",
            "demo",
            PAYLOAD
        )
        .is_err());
    }
}
