//! Import rules for the harness memory files (`~/.claude/projects/<slug>/memory/*.md`).
//! Pure: parsing and mapping only. Reading the directory is infrastructure's job,
//! and aplan never writes back into it.

use chrono::{DateTime, Utc};

use crate::errors::DomainError;
use crate::types::memory::MemoryKind;

/// The subset of a memory file's frontmatter that aplan imports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryFrontmatter {
    /// `name:` — a kebab-case slug, used to build a stable provenance reference.
    pub name: Option<String>,
    /// `description:` — the one-line summary, which becomes the memory title.
    pub description: Option<String>,
    /// `metadata.type` — drives the kind mapping.
    pub metadata_type: Option<String>,
    /// `metadata.modified`, when present and RFC 3339.
    pub modified: Option<DateTime<Utc>>,
}

/// A parsed memory file: its frontmatter and the markdown body below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMemoryFile {
    pub frontmatter: MemoryFrontmatter,
    pub body: String,
}

const FRONTMATTER_FENCE: &str = "---";

/// Parse a memory file into frontmatter + body.
///
/// Deliberately a small line-based reader rather than a YAML dependency: the
/// domain crate takes no new dependencies, and the shape of these files is
/// fixed (flat scalars plus one nested `metadata:` mapping).
///
/// Returns `ValidationError` when the file has no frontmatter fence at all —
/// `MEMORY.md`, the harness index, is exactly that case, and the caller skips it
/// rather than failing the whole import.
pub fn parse_memory_file(contents: &str) -> Result<ParsedMemoryFile, DomainError> {
    let trimmed = contents.trim_start_matches('\u{feff}').trim_start();
    let mut lines = trimmed.lines();

    if lines.next().map(str::trim) != Some(FRONTMATTER_FENCE) {
        return Err(DomainError::ValidationError(
            "memory file has no frontmatter fence".into(),
        ));
    }

    let mut front = MemoryFrontmatter::default();
    let mut in_metadata = false;
    let mut closed = false;
    let mut body_lines: Vec<&str> = Vec::new();

    for line in lines {
        if !closed {
            if line.trim() == FRONTMATTER_FENCE {
                closed = true;
                continue;
            }
            let indented = line.starts_with(' ') || line.starts_with('\t');
            let Some((key, value)) = split_key_value(line) else {
                continue;
            };
            // A nested block stays nested until a non-indented key appears.
            if !indented {
                in_metadata = key == "metadata";
            }
            match (in_metadata, indented, key.as_str()) {
                (false, false, "name") => front.name = unquote(value),
                (false, false, "description") => front.description = unquote(value),
                (true, true, "type") => front.metadata_type = unquote(value),
                (true, true, "modified") => {
                    front.modified = unquote(value).and_then(|raw| parse_rfc3339(&raw))
                }
                _ => {}
            }
            continue;
        }
        body_lines.push(line);
    }

    if !closed {
        return Err(DomainError::ValidationError(
            "memory file frontmatter is never closed".into(),
        ));
    }

    Ok(ParsedMemoryFile {
        frontmatter: front,
        body: body_lines.join("\n").trim().to_string(),
    })
}

/// Split `key: value` on the FIRST colon. Lines without one (list items, prose)
/// are ignored by the caller.
fn split_key_value(line: &str) -> Option<(String, &str)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.contains(' ') {
        return None;
    }
    Some((key.to_string(), value))
}

/// Trim a scalar and strip one layer of matching quotes. `None` when empty.
fn unquote(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(trimmed)
        .trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn parse_rfc3339(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Map the harness `metadata.type` onto a memory kind.
///
/// `feedback` and `user` describe how the user wants to be worked with, so they
/// are preferences. `project` and `reference` state something true about the
/// world, so they are facts. An unknown or absent type falls back to `fact`, the
/// lowest-weighted kind in recall scoring — an import must never silently
/// promote something to a decision.
pub fn kind_for_metadata_type(metadata_type: Option<&str>) -> MemoryKind {
    match metadata_type.map(str::trim).map(str::to_lowercase).as_deref() {
        Some("feedback") | Some("user") => MemoryKind::Preference,
        Some("project") | Some("reference") => MemoryKind::Fact,
        _ => MemoryKind::Fact,
    }
}

/// Stable provenance reference for an imported file, so a second import is a
/// no-op. Keyed on the frontmatter `name` when present, else the file name.
pub fn import_source_ref(name: Option<&str>, file_name: &str) -> String {
    let key = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(file_name);
    format!("{IMPORT_SOURCE_REF_PREFIX}{key}")
}

/// Prefix every imported memory's `source_ref` carries. Also the lookup key for
/// idempotency.
pub const IMPORT_SOURCE_REF_PREFIX: &str = "memory-file:";

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_FILE: &str = r#"---
name: project-mcp-crate-broken-rmcp
description: mcp doesn't compile at HEAD; use scoped cargo test
metadata:
  node_type: memory
  type: project
  originSessionId: abc-123
  modified: 2026-07-07T09:15:00.000Z
---

The `mcp` crate does not build at HEAD.

**Why:** the rmcp dependency moved.
"#;

    #[test]
    fn parses_a_real_harness_file() {
        let parsed = parse_memory_file(REAL_FILE).expect("parses");
        assert_eq!(
            parsed.frontmatter.name.as_deref(),
            Some("project-mcp-crate-broken-rmcp")
        );
        assert_eq!(
            parsed.frontmatter.description.as_deref(),
            Some("mcp doesn't compile at HEAD; use scoped cargo test")
        );
        assert_eq!(parsed.frontmatter.metadata_type.as_deref(), Some("project"));
        assert_eq!(
            parsed.frontmatter.modified,
            Some(
                DateTime::parse_from_rfc3339("2026-07-07T09:15:00.000Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert!(parsed.body.starts_with("The `mcp` crate"));
        assert!(parsed.body.ends_with("moved."), "body is trimmed");
    }

    #[test]
    fn strips_quotes_from_a_quoted_description() {
        let file = "---\nname: x\ndescription: \"quoted: with a colon\"\n---\nbody";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(
            parsed.frontmatter.description.as_deref(),
            Some("quoted: with a colon"),
            "only the first colon splits, and the quotes come off"
        );
    }

    #[test]
    fn a_body_containing_a_fence_is_kept_whole() {
        let file = "---\nname: x\n---\nbefore\n---\nafter";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(parsed.body, "before\n---\nafter");
    }

    #[test]
    fn a_file_without_frontmatter_is_rejected() {
        // MEMORY.md, the harness index, looks like this.
        let index = "- [a note](a.md) — hook\n- [b note](b.md) — hook\n";
        assert!(matches!(
            parse_memory_file(index).unwrap_err(),
            DomainError::ValidationError(_)
        ));
        assert!(matches!(
            parse_memory_file("").unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn an_unclosed_frontmatter_is_rejected() {
        let file = "---\nname: x\ndescription: y\n";
        assert!(matches!(
            parse_memory_file(file).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn a_nested_type_is_not_confused_with_a_top_level_one() {
        // `node_type` must not be read as `type`, and a top-level `type:` is not
        // the metadata one.
        let file = "---\nname: x\ntype: top-level-decoy\nmetadata:\n  node_type: memory\n  type: feedback\n---\nbody";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(parsed.frontmatter.metadata_type.as_deref(), Some("feedback"));
    }

    #[test]
    fn keys_after_the_metadata_block_ends_are_top_level_again() {
        let file = "---\nmetadata:\n  type: feedback\nname: after-block\n---\nbody";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(parsed.frontmatter.name.as_deref(), Some("after-block"));
        assert_eq!(parsed.frontmatter.metadata_type.as_deref(), Some("feedback"));
    }

    #[test]
    fn an_unparseable_modified_is_dropped_not_fatal() {
        let file = "---\nname: x\nmetadata:\n  modified: last tuesday\n---\nbody";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(parsed.frontmatter.modified, None);
    }

    #[test]
    fn an_empty_body_is_allowed() {
        let file = "---\nname: x\ndescription: y\n---\n";
        let parsed = parse_memory_file(file).expect("parses");
        assert_eq!(parsed.body, "");
    }

    #[test]
    fn feedback_and_user_types_become_preferences() {
        assert_eq!(kind_for_metadata_type(Some("feedback")), MemoryKind::Preference);
        assert_eq!(kind_for_metadata_type(Some("user")), MemoryKind::Preference);
        assert_eq!(kind_for_metadata_type(Some(" FEEDBACK ")), MemoryKind::Preference);
    }

    #[test]
    fn project_and_reference_types_become_facts() {
        assert_eq!(kind_for_metadata_type(Some("project")), MemoryKind::Fact);
        assert_eq!(kind_for_metadata_type(Some("reference")), MemoryKind::Fact);
    }

    #[test]
    fn an_unknown_or_missing_type_falls_back_to_fact() {
        assert_eq!(kind_for_metadata_type(None), MemoryKind::Fact);
        assert_eq!(kind_for_metadata_type(Some("invented")), MemoryKind::Fact);
        // Never a decision or a commitment: an import must not promote itself.
        for t in [None, Some("feedback"), Some("project"), Some("nonsense")] {
            let kind = kind_for_metadata_type(t);
            assert!(
                matches!(kind, MemoryKind::Fact | MemoryKind::Preference),
                "{t:?} mapped to {kind:?}"
            );
        }
    }

    #[test]
    fn the_source_ref_prefers_the_frontmatter_name() {
        assert_eq!(
            import_source_ref(Some("my-slug"), "file.md"),
            "memory-file:my-slug"
        );
    }

    #[test]
    fn the_source_ref_falls_back_to_the_file_name() {
        assert_eq!(import_source_ref(None, "file.md"), "memory-file:file.md");
        assert_eq!(import_source_ref(Some("  "), "file.md"), "memory-file:file.md");
    }

    #[test]
    fn the_source_ref_is_stable_across_calls() {
        // This is what makes a second import a no-op.
        assert_eq!(
            import_source_ref(Some("slug"), "a.md"),
            import_source_ref(Some("slug"), "b.md"),
            "the name wins, so renaming the file does not re-import it"
        );
    }
}
