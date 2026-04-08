//! Output formatting, exit codes, and color handling.

use std::io::Write;
use std::process::ExitCode as ProcessExitCode;

/// Stable exit codes used by every command and consumed by the Claude skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Generic = 1,
    NotFound = 2,
    Ambiguous = 3,
    PreconditionFailed = 4,
}

impl From<ExitCode> for ProcessExitCode {
    fn from(code: ExitCode) -> Self {
        ProcessExitCode::from(code as u8)
    }
}

/// Print a `serde_json::Value` (or any Serialize) to stdout as compact JSON.
/// Used by every command's `--json` mode. Returns a write error if stdout closes.
pub fn print_json<T: serde::Serialize>(value: &T) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writeln!(stdout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_values() {
        assert_eq!(ExitCode::Success as u8, 0);
        assert_eq!(ExitCode::Generic as u8, 1);
        assert_eq!(ExitCode::NotFound as u8, 2);
        assert_eq!(ExitCode::Ambiguous as u8, 3);
        assert_eq!(ExitCode::PreconditionFailed as u8, 4);
    }

    #[test]
    fn json_serialization_is_stable() {
        let v = serde_json::json!({ "ok": true, "n": 42 });
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(s, r#"{"n":42,"ok":true}"#);
    }
}
