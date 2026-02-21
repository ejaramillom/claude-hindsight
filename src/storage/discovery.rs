//! Session file discovery
//!
//! Scans Claude Code project directories for JSONL transcript files.

use crate::error::{HindsightError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a discovered session file
#[derive(Debug, Clone)]
pub struct SessionFile {
    /// Full path to the JSONL file
    pub path: PathBuf,

    /// Session ID (extracted from filename)
    pub session_id: String,

    /// Project name (decoded from directory name)
    pub project_name: String,

    /// File size in bytes
    pub file_size: u64,

    /// Session creation timestamp (seconds since epoch, from first node's timestamp)
    pub created_at: i64,

    /// Last modified timestamp (seconds since epoch)
    pub modified_at: i64,

    /// Whether this session has subagents (folder with subagents/ directory)
    pub has_subagents: bool,

    /// Model used (short name, e.g. "sonnet-4-5")
    pub model: Option<String>,

    /// Number of errors (tool result errors + error nodes)
    pub error_count: usize,

    /// First user message preview (up to 80 chars)
    pub first_message: Option<String>,

    /// Source directory path (from config, e.g. "~/.claude/projects")
    pub source_dir: String,

    /// Comma-separated unique models used by subagents (e.g. "claude-haiku-4-5")
    pub subagent_models: Option<String>,
}

/// Discover all Claude Code sessions in configured directories
///
/// Scans `~/.claude/projects/` for session JSONL files.
///
/// # Returns
///
/// Returns a vector of `SessionFile` structs representing all discovered sessions.
///
/// # Errors
///
/// Returns `HindsightError::NoSessionsFound` if no sessions are discovered.
pub fn discover_sessions() -> Result<Vec<SessionFile>> {
    let config = crate::config::Config::load().unwrap_or_default();
    let claude_dirs = resolve_claude_dirs(&config);

    let mut sessions = Vec::new();
    for (expanded_path, source_dir) in claude_dirs {
        let mut dir_sessions = scan_claude_dir(&expanded_path, &source_dir)?;
        sessions.append(&mut dir_sessions);
    }

    if sessions.is_empty() {
        return Err(HindsightError::NoSessionsFound);
    }

    // Sort by modification time (newest first)
    sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));

    Ok(sessions)
}

/// Resolve configured directories: expand ~ and filter to those that exist.
fn resolve_claude_dirs(config: &crate::config::Config) -> Vec<(PathBuf, String)> {
    let home = dirs::home_dir().unwrap_or_default();
    config
        .paths
        .claude_dirs
        .iter()
        .map(|d| {
            let expanded = if let Some(stripped) = d.path.strip_prefix("~/") {
                home.join(stripped)
            } else {
                PathBuf::from(&d.path)
            };
            (expanded, d.path.clone())
        })
        .filter(|(p, _)| p.exists())
        .collect()
}

/// Scan a top-level Claude projects directory for all session files.
fn scan_claude_dir(claude_dir: &Path, source_dir: &str) -> Result<Vec<SessionFile>> {
    let mut sessions = Vec::new();
    for project_entry in fs::read_dir(claude_dir)? {
        let project_path = project_entry?.path();
        if !project_path.is_dir() {
            continue;
        }

        let project_name = decode_project_name(&project_path);
        let mut project_sessions = scan_project_dir(&project_path, &project_name, source_dir)?;
        sessions.append(&mut project_sessions);
    }
    Ok(sessions)
}

/// Scan a specific project directory for .jsonl session files.
fn scan_project_dir(project_path: &Path, project_name: &str, source_dir: &str) -> Result<Vec<SessionFile>> {
    let mut sessions = Vec::new();
    for file_entry in fs::read_dir(project_path)? {
        let file_path = file_entry?.path();

        if is_session_file(&file_path) {
            if let Ok(session) = build_session_file(&file_path, project_name, source_dir) {
                sessions.push(session);
            }
        }
    }
    Ok(sessions)
}

/// Check if a path points to a Claude session JSONL file.
fn is_session_file(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jsonl")
}

/// Build a SessionFile struct from a path and metadata.
fn build_session_file(path: &Path, project_name: &str, source_dir: &str) -> Result<SessionFile> {
    let metadata = fs::metadata(path)?;
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let modified_at = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Check if there's a matching directory with subagents
    let subagents_dir = path.parent().unwrap().join(&session_id).join("subagents");
    let has_subagents = subagents_dir.exists() && subagents_dir.is_dir();

    Ok(SessionFile {
        path: path.to_path_buf(),
        session_id,
        project_name: project_name.to_string(),
        file_size: metadata.len(),
        created_at: modified_at,
        modified_at,
        has_subagents,
        model: None,
        error_count: 0,
        first_message: None,
        source_dir: source_dir.to_string(),
        subagent_models: None,
    })
}

/// Decode project name from directory path
///
/// Converts `-Users-ediazestrada-Documents-Projects-experiment` to `experiment`
fn decode_project_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(|s| {
            // Try to extract the last meaningful part
            if s.starts_with('-') {
                s.split('-').next_back().unwrap_or(s).to_string()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_project_name() {
        let path = Path::new("-Users-ediazestrada-Documents-Projects-experiment");
        assert_eq!(decode_project_name(path), "experiment");

        let path = Path::new("my-project");
        assert_eq!(decode_project_name(path), "my-project");
    }
}
