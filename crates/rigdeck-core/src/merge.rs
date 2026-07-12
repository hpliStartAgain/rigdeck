//! 保守的三方合并规则。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 三方合并结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MergeResult<T> {
    /// 可安全自动合并。
    Clean {
        /// 合并值。
        value: T,
    },
    /// 同一语义位置发生双边修改，需要用户审查。
    Conflict {
        /// 冲突位置；纯文本使用 `$text`。
        paths: Vec<String>,
        /// 基线。
        base: T,
        /// RigDeck 侧。
        ours: T,
        /// Agent 侧。
        theirs: T,
    },
}

/// 合并纯文本。
///
/// 相同内容与单边修改自动解决；双边文本修改保守地返回冲突。结构化 JSON/TOML/YAML
/// 应先由保留格式的 codec 转成语义树，再使用 [`merge_json`] 合并键。
pub fn merge_text(base: &str, ours: &str, theirs: &str) -> MergeResult<String> {
    if ours == theirs {
        MergeResult::Clean {
            value: ours.to_owned(),
        }
    } else if ours == base {
        MergeResult::Clean {
            value: theirs.to_owned(),
        }
    } else if theirs == base {
        MergeResult::Clean {
            value: ours.to_owned(),
        }
    } else {
        MergeResult::Conflict {
            paths: vec!["$text".to_owned()],
            base: base.to_owned(),
            ours: ours.to_owned(),
            theirs: theirs.to_owned(),
        }
    }
}

/// 递归合并 JSON 值，自动接受不重叠键变化。
pub fn merge_json(base: &Value, ours: &Value, theirs: &Value) -> MergeResult<Value> {
    let mut conflicts = Vec::new();
    let merged = merge_json_at("$", base, ours, theirs, &mut conflicts);
    if conflicts.is_empty() {
        MergeResult::Clean { value: merged }
    } else {
        MergeResult::Conflict {
            paths: conflicts,
            base: base.clone(),
            ours: ours.clone(),
            theirs: theirs.clone(),
        }
    }
}

fn merge_json_at(
    path: &str,
    base: &Value,
    ours: &Value,
    theirs: &Value,
    conflicts: &mut Vec<String>,
) -> Value {
    if ours == theirs {
        return ours.clone();
    }
    if ours == base {
        return theirs.clone();
    }
    if theirs == base {
        return ours.clone();
    }

    match (base, ours, theirs) {
        (Value::Object(base), Value::Object(ours), Value::Object(theirs)) => {
            Value::Object(merge_objects(path, base, ours, theirs, conflicts))
        }
        _ => {
            conflicts.push(path.to_owned());
            ours.clone()
        }
    }
}

fn merge_objects(
    path: &str,
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
    conflicts: &mut Vec<String>,
) -> Map<String, Value> {
    let keys: BTreeSet<_> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    let missing = Value::Null;
    keys.into_iter()
        .filter_map(|key| {
            let child_path = format!("{path}.{key}");
            let merged = merge_json_at(
                &child_path,
                base.get(key).unwrap_or(&missing),
                ours.get(key).unwrap_or(&missing),
                theirs.get(key).unwrap_or(&missing),
                conflicts,
            );
            // `null` 在这里代表三方都同意删除该键；真实 null 值仍能在至少一侧存在时保留。
            let all_missing = !ours.contains_key(key) && !theirs.contains_key(key);
            (!all_missing).then(|| (key.clone(), merged))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merges_non_overlapping_object_keys() {
        let base = json!({"a": 1, "b": 1});
        let ours = json!({"a": 2, "b": 1});
        let theirs = json!({"a": 1, "b": 2});
        assert_eq!(
            merge_json(&base, &ours, &theirs),
            MergeResult::Clean {
                value: json!({"a": 2, "b": 2})
            }
        );
    }

    #[test]
    fn reports_overlapping_change_path() {
        let base = json!({"a": 1});
        let ours = json!({"a": 2});
        let theirs = json!({"a": 3});
        let MergeResult::Conflict { paths, .. } = merge_json(&base, &ours, &theirs) else {
            panic!("应产生冲突");
        };
        assert_eq!(paths, vec!["$.a"]);
    }
}
