//! Obsidian vault import functionality
//!
//! This module provides native Rust implementation for importing notes
//! from an Obsidian vault, eliminating the need for external Node.js scripts.

use chrono::{DateTime, Utc};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use yaml_rust2::YamlLoader;

/// Parsed note from an Obsidian vault
#[derive(Debug, Clone)]
pub struct ObsidianNote {
    pub title: String,
    pub content: String,
    pub source_url: String,
    /// Flat frontmatter tags (no hierarchy)
    pub frontmatter_tags: Vec<String>,
    /// Hierarchical folder tags with parent relationships
    pub folder_tags: Vec<HierarchicalTag>,
    pub created_at: String,
    pub updated_at: String,
    pub relative_path: String,
}

/// Default patterns to exclude when discovering notes
pub const DEFAULT_EXCLUDES: &[&str] = &[".obsidian", ".trash", ".git", "node_modules"];

/// Parse YAML frontmatter from markdown content
///
/// Returns a tuple of (frontmatter YAML, body content without frontmatter)
pub fn parse_frontmatter(content: &str) -> (Option<yaml_rust2::Yaml>, String) {
    // Regex to match YAML frontmatter block: starts with ---, ends with ---
    let re = match Regex::new(r"^---\s*\n([\s\S]*?)\n---\s*\n?") {
        Ok(r) => r,
        Err(_) => return (None, content.to_string()),
    };

    if let Some(captures) = re.captures(content) {
        let yaml_str = &captures[1];
        let body = &content[captures[0].len()..];

        match YamlLoader::load_from_str(yaml_str) {
            Ok(docs) if !docs.is_empty() => (Some(docs[0].clone()), body.to_string()),
            _ => (None, content.to_string()),
        }
    } else {
        (None, content.to_string())
    }
}

/// Extract tags from YAML frontmatter
///
/// Supports various formats:
/// - Array: `tags: [tag1, tag2]`
/// - Comma-separated string: `tags: "tag1, tag2"`
/// - YAML list:
///   ```yaml
///   tags:
///     - tag1
///     - tag2
///   ```
pub fn extract_frontmatter_tags(yaml: &yaml_rust2::Yaml) -> Vec<String> {
    match &yaml["tags"] {
        yaml_rust2::Yaml::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        yaml_rust2::Yaml::String(s) => s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// A tag with its parent path for hierarchical creation
#[derive(Debug, Clone)]
pub struct HierarchicalTag {
    pub name: String,
    /// Parent tag names in order from root to immediate parent
    /// e.g., for "Projects/Work/Tasks", Tasks would have parent_path: ["Projects", "Work"]
    pub parent_path: Vec<String>,
}

/// Extract tags from folder path as a hierarchy
///
/// Each folder in the path becomes a tag with proper parent relationships.
/// e.g., "Projects/Work/note.md" -> [
///   HierarchicalTag { name: "Projects", parent_path: [] },
///   HierarchicalTag { name: "Work", parent_path: ["Projects"] },
/// ]
pub fn extract_folder_tags_hierarchical(relative_path: &Path) -> Vec<HierarchicalTag> {
    let mut result = Vec::new();

    if let Some(parent) = relative_path.parent() {
        let components: Vec<String> = parent
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .filter(|s| !s.is_empty())
            .collect();

        for (i, name) in components.iter().enumerate() {
            result.push(HierarchicalTag {
                name: name.clone(),
                parent_path: components[..i].to_vec(),
            });
        }
    }

    result
}

/// Extract tags from folder path (flat, for backward compatibility)
///
/// Each folder in the path becomes a tag.
/// e.g., "Projects/Work/note.md" -> ["Projects", "Work"]
pub fn extract_folder_tags(relative_path: &Path) -> Vec<String> {
    if let Some(parent) = relative_path.parent() {
        parent
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(String::from))
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        Vec::new()
    }
}

/// Generate Obsidian-style source URL for deduplication and reference
///
/// Format: `obsidian://VaultName/path/to/note`
pub fn generate_source_url(vault_name: &str, relative_path: &Path) -> String {
    let note_path = relative_path
        .with_extension("")
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");

    let encoded_vault = urlencoding::encode(vault_name);
    let path_parts: Vec<String> = note_path
        .split('/')
        .map(|p| urlencoding::encode(p).to_string())
        .collect();

    format!("obsidian://{}/{}", encoded_vault, path_parts.join("/"))
}

/// Parse a single Obsidian note file
pub fn parse_obsidian_note(
    file_path: &Path,
    relative_path: &Path,
    vault_name: &str,
) -> Result<ObsidianNote, String> {
    let content =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let metadata =
        fs::metadata(file_path).map_err(|e| format!("Failed to get metadata: {}", e))?;

    let (frontmatter, body) = parse_frontmatter(&content);

    // Extract title from: frontmatter > first h1 > filename
    let title = frontmatter
        .as_ref()
        .and_then(|fm| fm["title"].as_str().map(String::from))
        .or_else(|| {
            // Look for first h1 heading
            for line in body.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("# ") {
                    return Some(trimmed[2..].trim().to_string());
                }
            }
            None
        })
        .unwrap_or_else(|| {
            relative_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    // Build final content with title as h1 if not present
    let final_content = if body.trim().starts_with("# ") {
        body.trim().to_string()
    } else {
        format!("# {}\n\n{}", title, body.trim())
    };

    // Extract hierarchical folder tags
    let folder_tags = extract_folder_tags_hierarchical(relative_path);

    // Extract flat frontmatter tags
    let frontmatter_tags = if let Some(ref fm) = frontmatter {
        extract_frontmatter_tags(fm)
    } else {
        Vec::new()
    };

    // Get timestamps from frontmatter or file metadata
    let created_at = frontmatter
        .as_ref()
        .and_then(|fm| fm["created"].as_str().map(String::from))
        .or_else(|| {
            metadata.created().ok().and_then(|t| {
                t.duration_since(UNIX_EPOCH).ok().and_then(|d| {
                    DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
                })
            })
        })
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let updated_at = frontmatter
        .as_ref()
        .and_then(|fm| {
            fm["modified"]
                .as_str()
                .or_else(|| fm["updated"].as_str())
                .map(String::from)
        })
        .or_else(|| {
            metadata.modified().ok().and_then(|t| {
                t.duration_since(UNIX_EPOCH).ok().and_then(|d| {
                    DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
                })
            })
        })
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    Ok(ObsidianNote {
        title,
        content: final_content,
        source_url: generate_source_url(vault_name, relative_path),
        frontmatter_tags,
        folder_tags,
        created_at,
        updated_at,
        relative_path: relative_path.to_string_lossy().to_string(),
    })
}

/// Check if a path should be excluded based on exclude patterns
fn should_exclude(relative_path: &Path, exclude_patterns: &[&str]) -> bool {
    let path_str = relative_path.to_string_lossy();

    for pattern in exclude_patterns {
        // Check if any component of the path matches the pattern
        for component in relative_path.components() {
            if let Some(s) = component.as_os_str().to_str() {
                if s == *pattern {
                    return true;
                }
            }
        }
        // Also check if the path starts with the pattern
        if path_str.starts_with(pattern) {
            return true;
        }
    }

    false
}

/// Discover all markdown files in an Obsidian vault
///
/// Recursively finds all `.md` files, excluding common directories
/// like `.obsidian`, `.trash`, `.git`, and `node_modules`.
pub fn discover_notes(
    vault_path: &Path,
    exclude_patterns: &[&str],
) -> Result<Vec<PathBuf>, String> {
    let pattern = vault_path.join("**/*.md");
    let pattern_str = pattern.to_string_lossy();

    let entries: Vec<PathBuf> = glob::glob(&pattern_str)
        .map_err(|e| format!("Invalid glob pattern: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|path| {
            let relative = path.strip_prefix(vault_path).unwrap_or(path);
            !should_exclude(relative, exclude_patterns)
        })
        .collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_with_yaml() {
        let content = "---\ntitle: Test Note\ntags: [tag1, tag2]\n---\n\nBody content here.";
        let (fm, body) = parse_frontmatter(content);

        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm["title"].as_str(), Some("Test Note"));
        assert_eq!(body.trim(), "Body content here.");
    }

    #[test]
    fn test_parse_frontmatter_no_yaml() {
        let content = "Just regular markdown content.";
        let (fm, body) = parse_frontmatter(content);

        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_extract_frontmatter_tags_array() {
        let yaml = YamlLoader::load_from_str("tags: [tag1, tag2, tag3]")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let tags = extract_frontmatter_tags(&yaml);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn test_extract_frontmatter_tags_string() {
        let yaml = YamlLoader::load_from_str("tags: \"tag1, tag2, tag3\"")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let tags = extract_frontmatter_tags(&yaml);
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn test_extract_folder_tags() {
        let path = Path::new("Projects/Work/meeting-notes.md");
        let tags = extract_folder_tags(path);
        assert_eq!(tags, vec!["Projects", "Work"]);
    }

    #[test]
    fn test_extract_folder_tags_root() {
        let path = Path::new("note.md");
        let tags = extract_folder_tags(path);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_generate_source_url() {
        let url = generate_source_url("My Vault", Path::new("Projects/note.md"));
        assert_eq!(url, "obsidian://My%20Vault/Projects/note");
    }

    #[test]
    fn test_should_exclude() {
        assert!(should_exclude(
            Path::new(".obsidian/config.json"),
            DEFAULT_EXCLUDES
        ));
        assert!(should_exclude(Path::new(".git/HEAD"), DEFAULT_EXCLUDES));
        assert!(!should_exclude(
            Path::new("Projects/note.md"),
            DEFAULT_EXCLUDES
        ));
    }

    #[test]
    fn test_extract_folder_tags_hierarchical() {
        let path = Path::new("Projects/Work/Tasks/meeting-notes.md");
        let tags = extract_folder_tags_hierarchical(path);

        assert_eq!(tags.len(), 3);

        // First tag: Projects (no parent)
        assert_eq!(tags[0].name, "Projects");
        assert!(tags[0].parent_path.is_empty());

        // Second tag: Work (parent: Projects)
        assert_eq!(tags[1].name, "Work");
        assert_eq!(tags[1].parent_path, vec!["Projects"]);

        // Third tag: Tasks (parent path: Projects/Work)
        assert_eq!(tags[2].name, "Tasks");
        assert_eq!(tags[2].parent_path, vec!["Projects", "Work"]);
    }

    #[test]
    fn test_extract_folder_tags_hierarchical_root() {
        let path = Path::new("note.md");
        let tags = extract_folder_tags_hierarchical(path);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_extract_folder_tags_hierarchical_single_level() {
        let path = Path::new("Projects/note.md");
        let tags = extract_folder_tags_hierarchical(path);

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Projects");
        assert!(tags[0].parent_path.is_empty());
    }
}
