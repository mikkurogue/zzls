use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ZigFormatter {
    zig_path: PathBuf,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum FormatResult {
    Formatted,
    NoChanges,
    CheckFailed { diff: String },
    Error(String),
}

impl ZigFormatter {
    pub fn new(zig_path: PathBuf) -> Self {
        Self { zig_path }
    }

    pub async fn format_file(&self, file: &Path, check_only: bool) -> Result<FormatResult, FormatterError> {
        let mut cmd = Command::new(&self.zig_path);
        cmd.arg("fmt");

        if check_only {
            cmd.arg("--check");
        }

        cmd.arg(file);

        let output = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| FormatterError::Process(e.to_string()))?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        if check_only {
            if output.status.success() {
                Ok(FormatResult::NoChanges)
            } else {
                Ok(FormatResult::CheckFailed { diff: stderr.to_string() })
            }
        } else if output.status.success() {
            Ok(FormatResult::Formatted)
        } else {
            Ok(FormatResult::Error(stderr.to_string()))
        }
    }

    pub async fn format_source(&self, source: &str) -> Result<String, FormatterError> {
        let mut child = Command::new(&self.zig_path)
            .arg("fmt")
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| FormatterError::Process(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes()).await
                .map_err(|e| FormatterError::Io(e.to_string()))?;
            drop(stdin);
        }

        let output = child.wait_with_output().await
            .map_err(|e| FormatterError::Process(e.to_string()))?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            tracing::warn!("zig fmt --stdin failed: {}", stderr);
            return Ok(source.to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        if stdout.is_empty() {
            return Ok(source.to_string());
        }

        Ok(stdout.to_string())
    }

    #[allow(dead_code)]
    pub async fn needs_formatting(&self, source: &str) -> Result<bool, FormatterError> {
        let mut child = Command::new(&self.zig_path)
            .arg("fmt")
            .arg("--check")
            .arg("--stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| FormatterError::Process(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes()).await
                .map_err(|e| FormatterError::Io(e.to_string()))?;
            drop(stdin);
        }

        let output = child.wait_with_output().await
            .map_err(|e| FormatterError::Process(e.to_string()))?;

        Ok(!output.status.success())
    }
}

#[derive(Debug, Clone)]
pub enum FormatterError {
    Io(String),
    Process(String),
}

impl std::fmt::Display for FormatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatterError::Io(e) => write!(f, "IO error: {}", e),
            FormatterError::Process(e) => write!(f, "Process error: {}", e),
        }
    }
}

impl std::error::Error for FormatterError {}
