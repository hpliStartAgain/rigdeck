//! 输出脱敏。

use crate::SecretValue;

/// 脱敏占位符。
pub const REDACTED: &str = "[REDACTED]";

/// 替换已知 secret 与常见凭据行。
///
/// 该函数作为日志、诊断、计划 diff、UI 复制和 crash 输出的共同入口。模式匹配是
/// 纵深防御，不能取代“类型中不保存明文 secret”的主边界。
pub fn redact_text(input: &str, known: &[SecretValue]) -> String {
    let mut output = input.to_owned();
    for secret in known {
        if secret.expose().len() < 4 {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(secret.expose()) {
            output = output.replace(text, REDACTED);
        }
    }

    output
        .lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 安装结构化脱敏的 panic hook，避免默认 crash 输出原样打印凭据行。
///
/// panic 路径不能安全访问应用数据库或系统钥匙串，因此这里执行无状态结构脱敏；
/// 正常诊断和导出仍应使用 [`redact_text`] 并传入已知 secret。
pub fn install_redacted_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|value| (*value).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非文本 panic payload".to_owned());
        let safe = redact_text(&payload, &[]);
        if let Some(location) = info.location() {
            eprintln!(
                "RigDeck 发生未处理错误：{safe}（{}:{}:{}）",
                location.file(),
                location.line(),
                location.column()
            );
        } else {
            eprintln!("RigDeck 发生未处理错误：{safe}");
        }
    }));
}

fn redact_sensitive_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let sensitive = [
        "authorization:",
        "password=",
        "password:",
        "api_key=",
        "api-key:",
        "token=",
        "client_secret",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if !sensitive {
        return line.to_owned();
    }
    let split = line.find([':', '=']).unwrap_or(line.len());
    format!("{}: {REDACTED}", &line[..split])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_and_structural_secrets() {
        let known = [SecretValue::new(b"abcd-1234".to_vec())];
        let input = "note=abcd-1234\nAuthorization: Bearer hidden\nname=visible";
        let output = redact_text(input, &known);
        assert!(!output.contains("abcd-1234"));
        assert!(!output.contains("Bearer hidden"));
        assert!(output.contains("name=visible"));
    }

    #[test]
    fn crash_fixture_is_structurally_redacted() {
        let output = redact_text(
            "fatal\nAuthorization: Bearer crash-secret\npassword=hidden",
            &[],
        );
        assert!(!output.contains("crash-secret"));
        assert!(!output.contains("hidden"));
        assert!(output.contains(REDACTED));
    }

    #[test]
    fn empty_input_redacts_to_empty() {
        assert_eq!(redact_text("", &[]), "");
    }

    #[test]
    fn unicode_input_preserves_non_secret_text() {
        let output = redact_text("你好 world api_key=hidden", &[]);
        assert!(output.contains("你好"));
        assert!(output.contains("world"));
        assert!(!output.contains("hidden"));
    }

    #[test]
    fn multiple_secrets_all_redacted() {
        let secrets = [
            SecretValue::new(b"secret-one".to_vec()),
            SecretValue::new(b"secret-two".to_vec()),
        ];
        let input = "first=secret-one second=secret-two third=visible";
        let output = redact_text(input, &secrets);
        assert!(!output.contains("secret-one"));
        assert!(!output.contains("secret-two"));
        assert!(output.contains("third=visible"));
    }

    #[test]
    fn very_long_input_is_handled() {
        let secret = SecretValue::new(b"leak-me".to_vec());
        let input = "leak-me ".repeat(10000);
        let output = redact_text(&input, &[secret]);
        assert!(!output.contains("leak-me"));
    }
}
