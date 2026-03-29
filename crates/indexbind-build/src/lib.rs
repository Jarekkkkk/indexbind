use anyhow::{anyhow, Result};
use indexbind_core::{
    build_artifact, build_canonical_artifact, export_artifact_from_build_cache,
    export_canonical_from_build_cache, update_build_cache, BuildArtifactOptions, BuildCacheUpdate,
    BuildStats, CanonicalBuildStats, IncrementalBuildStats, NormalizedDocument, SourceRoot,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub enum DirectoryUpdateMode {
    #[default]
    FullScan,
    GitDiff {
        base_revision: Option<String>,
    },
}

pub fn build_from_directory(
    input: &Path,
    output: &Path,
    mut options: BuildArtifactOptions,
) -> Result<BuildStats> {
    let source_root = input.canonicalize()?;
    options.source_root = SourceRoot {
        id: "root".to_string(),
        original_path: source_root.display().to_string(),
    };
    let documents = read_documents(&source_root)?;
    build_artifact(output, &documents, &options).map_err(Into::into)
}

pub fn update_cache_from_directory(
    input: &Path,
    cache_path: &Path,
    options: BuildArtifactOptions,
) -> Result<IncrementalBuildStats> {
    update_cache_from_directory_with_mode(input, cache_path, options, DirectoryUpdateMode::FullScan)
}

pub fn update_cache_from_directory_with_mode(
    input: &Path,
    cache_path: &Path,
    mut options: BuildArtifactOptions,
    mode: DirectoryUpdateMode,
) -> Result<IncrementalBuildStats> {
    let source_root = input.canonicalize()?;
    options.source_root = SourceRoot {
        id: "root".to_string(),
        original_path: source_root.display().to_string(),
    };
    match read_directory_update(&source_root, mode) {
        Ok(update) => update_build_cache(cache_path, update, &options).map_err(Into::into),
        Err(_) => {
            let documents = read_documents(&source_root)?;
            update_build_cache(
                cache_path,
                BuildCacheUpdate {
                    documents,
                    removed_relative_paths: Vec::new(),
                    replace_all: true,
                },
                &options,
            )
            .map_err(Into::into)
        }
    }
}

pub fn export_artifact_from_cache(cache_path: &Path, output: &Path) -> Result<BuildStats> {
    export_artifact_from_build_cache(cache_path, output).map_err(Into::into)
}

pub fn build_canonical_from_directory(
    input: &Path,
    output_dir: &Path,
    mut options: BuildArtifactOptions,
) -> Result<CanonicalBuildStats> {
    let source_root = input.canonicalize()?;
    options.source_root = SourceRoot {
        id: "root".to_string(),
        original_path: source_root.display().to_string(),
    };
    let documents = read_documents(&source_root)?;
    build_canonical_artifact(output_dir, &documents, &options).map_err(Into::into)
}

pub fn export_canonical_from_cache(
    cache_path: &Path,
    output_dir: &Path,
) -> Result<CanonicalBuildStats> {
    export_canonical_from_build_cache(cache_path, output_dir).map_err(Into::into)
}

fn read_documents(root: &Path) -> Result<Vec<NormalizedDocument>> {
    read_documents_for_relative_paths(root, None)
}

fn read_documents_for_relative_paths(
    root: &Path,
    relative_paths: Option<&BTreeSet<String>>,
) -> Result<Vec<NormalizedDocument>> {
    let mut documents = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() || !supported_extension(entry.path()) {
            continue;
        }
        let path = entry.path();
        let relative_path = relative_path(root, path)?;
        if relative_paths.is_some_and(|allowed| !allowed.contains(&relative_path)) {
            continue;
        }
        let source = fs::read_to_string(path)?;
        let file_name_title = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string);
        let parsed = parse_document_source(&source, file_name_title);
        documents.push(NormalizedDocument {
            doc_id: None,
            source_path: Some(path.canonicalize()?.display().to_string()),
            relative_path,
            canonical_url: parsed.canonical_url,
            title: parsed.title,
            summary: parsed.summary,
            content: parsed.content,
            metadata: parsed.metadata,
        });
    }
    Ok(documents)
}

fn read_directory_update(root: &Path, mode: DirectoryUpdateMode) -> Result<BuildCacheUpdate> {
    match mode {
        DirectoryUpdateMode::FullScan => Ok(BuildCacheUpdate {
            documents: read_documents(root)?,
            removed_relative_paths: Vec::new(),
            replace_all: true,
        }),
        DirectoryUpdateMode::GitDiff { base_revision } => read_git_diff_update(root, base_revision),
    }
}

fn read_git_diff_update(root: &Path, base_revision: Option<String>) -> Result<BuildCacheUpdate> {
    ensure_git_repository(root)?;
    let mut changed = BTreeSet::new();
    let mut removed = BTreeSet::new();

    if let Some(base_revision) = base_revision {
        collect_git_name_status(
            root,
            &[
                "diff",
                "--name-status",
                "-z",
                &format!("{base_revision}...HEAD"),
                "--",
                ".",
            ],
            &mut changed,
            &mut removed,
        )?;
    } else {
        collect_git_name_status(
            root,
            &["diff", "--name-status", "-z", "HEAD", "--", "."],
            &mut changed,
            &mut removed,
        )?;
    }

    collect_git_name_status(
        root,
        &["diff", "--name-status", "-z", "--", "."],
        &mut changed,
        &mut removed,
    )?;
    collect_untracked_files(root, &mut changed)?;

    let documents = read_documents_for_relative_paths(root, Some(&changed))?;
    Ok(BuildCacheUpdate {
        documents,
        removed_relative_paths: removed.into_iter().collect(),
        replace_all: false,
    })
}

#[derive(Debug, PartialEq)]
struct ParsedDocumentSource {
    canonical_url: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    content: String,
    metadata: BTreeMap<String, Value>,
}

fn parse_document_source(source: &str, file_name_title: Option<String>) -> ParsedDocumentSource {
    let (frontmatter, content) = split_frontmatter(source)
        .and_then(|(frontmatter, body)| parse_frontmatter(frontmatter).map(|parsed| (parsed, body)))
        .map(|(frontmatter, body)| (frontmatter, body.to_string()))
        .unwrap_or_else(|| (ParsedFrontmatter::default(), source.to_string()));
    let title = frontmatter
        .title
        .clone()
        .or_else(|| extract_title(&content))
        .or(file_name_title);

    ParsedDocumentSource {
        canonical_url: frontmatter.canonical_url,
        title,
        summary: frontmatter.summary,
        content,
        metadata: frontmatter.metadata,
    }
}

#[derive(Debug, Default)]
struct ParsedFrontmatter {
    canonical_url: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    metadata: BTreeMap<String, Value>,
}

fn split_frontmatter(source: &str) -> Option<(&str, &str)> {
    let (rest, delimiter) = if let Some(rest) = source.strip_prefix("---\n") {
        (rest, "\n---\n")
    } else if let Some(rest) = source.strip_prefix("---\r\n") {
        (rest, "\r\n---\r\n")
    } else {
        return None;
    };
    let (frontmatter, body) = rest.split_once(delimiter)?;
    Some((frontmatter, body))
}

fn parse_frontmatter(frontmatter: &str) -> Option<ParsedFrontmatter> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(frontmatter).ok()?;
    let Some(object) = yaml_mapping_to_json_map(value) else {
        return None;
    };

    let mut metadata = BTreeMap::new();
    let mut title = None;
    let mut summary = None;
    let mut canonical_url = None;
    for (key, value) in object {
        match key.as_str() {
            "title" => title = value.as_str().map(str::to_string),
            "summary" => summary = value.as_str().map(str::to_string),
            "canonical_url" | "canonicalUrl" => canonical_url = value.as_str().map(str::to_string),
            _ => {
                metadata.insert(key, value);
            }
        }
    }

    Some(ParsedFrontmatter {
        canonical_url,
        title,
        summary,
        metadata,
    })
}

fn extract_title(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            Some(trimmed.trim_start_matches('#').trim().to_string())
        } else {
            None
        }
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| anyhow!("path is outside of source root: {}", path.display()))?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn supported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "mdx" | "txt")
    )
}

fn supported_relative_path(path: &str) -> bool {
    supported_extension(Path::new(path))
}

fn ensure_git_repository(root: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("source root is not inside a git repository"))
    }
}

fn collect_git_name_status(
    root: &Path,
    args: &[&str],
    changed: &mut BTreeSet<String>,
    removed: &mut BTreeSet<String>,
) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse_git_name_status(&output.stdout, changed, removed);
    Ok(())
}

fn collect_untracked_files(root: &Path, changed: &mut BTreeSet<String>) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative_path = normalize_relative_path_bytes(path);
        if supported_relative_path(&relative_path) {
            changed.insert(relative_path);
        }
    }
    Ok(())
}

fn parse_git_name_status(
    bytes: &[u8],
    changed: &mut BTreeSet<String>,
    removed: &mut BTreeSet<String>,
) {
    let mut fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    while let Some(status_bytes) = fields.next() {
        let status = String::from_utf8_lossy(status_bytes);
        let code = status.chars().next().unwrap_or('M');
        match code {
            'R' | 'C' => {
                let old_path = fields.next().map(normalize_relative_path_bytes);
                let new_path = fields.next().map(normalize_relative_path_bytes);
                if let Some(old_path) = old_path.filter(|path| supported_relative_path(path)) {
                    removed.insert(old_path);
                }
                if let Some(new_path) = new_path.filter(|path| supported_relative_path(path)) {
                    changed.insert(new_path);
                }
            }
            'D' => {
                if let Some(path) = fields.next().map(normalize_relative_path_bytes) {
                    if supported_relative_path(&path) {
                        removed.insert(path);
                    }
                }
            }
            _ => {
                if let Some(path) = fields.next().map(normalize_relative_path_bytes) {
                    if supported_relative_path(&path) {
                        changed.insert(path);
                    }
                }
            }
        }
    }
}

fn normalize_relative_path_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\\', "/")
}

fn yaml_mapping_to_json_map(value: serde_yaml::Value) -> Option<Map<String, Value>> {
    serde_json::to_value(value).ok()?.as_object().cloned()
}

#[allow(dead_code)]
fn _debug_root(path: &PathBuf) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_document_source, read_documents, update_cache_from_directory_with_mode,
        DirectoryUpdateMode,
    };
    use anyhow::{anyhow, Result};
    use indexbind_core::BuildArtifactOptions;
    use serde_json::json;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    #[test]
    fn frontmatter_overrides_body_heading_and_populates_metadata() {
        let source = r#"---
title: Frontmatter Title
summary: Short summary
canonical_url: /docs/getting-started
lang: rust
weight: 2
published: true
---

# Body Heading

Hello world.
"#;

        let parsed = parse_document_source(source, Some("fallback".to_string()));
        assert_eq!(parsed.title.as_deref(), Some("Frontmatter Title"));
        assert_eq!(parsed.summary.as_deref(), Some("Short summary"));
        assert_eq!(
            parsed.canonical_url.as_deref(),
            Some("/docs/getting-started")
        );
        assert_eq!(
            parsed.content.trim_start(),
            "# Body Heading\n\nHello world.\n"
        );
        assert_eq!(parsed.metadata.get("lang"), Some(&json!("rust")));
        assert_eq!(parsed.metadata.get("weight"), Some(&json!(2)));
        assert_eq!(parsed.metadata.get("published"), Some(&json!(true)));
        assert!(!parsed.metadata.contains_key("title"));
        assert!(!parsed.metadata.contains_key("summary"));
        assert!(!parsed.metadata.contains_key("canonical_url"));
    }

    #[test]
    fn body_heading_and_filename_remain_fallbacks() {
        let with_heading = parse_document_source("# Heading\n\nBody", Some("fallback".to_string()));
        assert_eq!(with_heading.title.as_deref(), Some("Heading"));

        let with_filename = parse_document_source("Body only", Some("fallback".to_string()));
        assert_eq!(with_filename.title.as_deref(), Some("fallback"));
    }

    #[test]
    fn canonical_url_alias_is_supported() {
        let source = r#"---
canonicalUrl: /docs/alias
---

Body
"#;
        let parsed = parse_document_source(source, None);
        assert_eq!(parsed.canonical_url.as_deref(), Some("/docs/alias"));
    }

    #[test]
    fn invalid_frontmatter_falls_back_to_body_content() {
        let source = "---\ninvalid: [\n---\n# Heading\n";
        let parsed = parse_document_source(source, None);
        assert_eq!(parsed.title.as_deref(), Some("Heading"));
        assert_eq!(parsed.content, source);
        assert!(parsed.metadata.is_empty());
    }

    #[test]
    fn windows_style_frontmatter_is_supported() {
        let source = "---\r\ntitle: Guide\r\nsummary: Windows\r\n---\r\n\r\nBody\r\n";
        let parsed = parse_document_source(source, None);
        assert_eq!(parsed.title.as_deref(), Some("Guide"));
        assert_eq!(parsed.summary.as_deref(), Some("Windows"));
        assert_eq!(parsed.content, "\r\nBody\r\n");
    }

    #[test]
    fn read_documents_parses_frontmatter_from_directory_files() {
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("guide.md");
        fs::write(
            &path,
            r#"---
title: Guide
summary: Directory summary
canonical_url: /docs/guide
section: docs
---

# Ignored Heading

Body
"#,
        )
        .unwrap();

        let documents = read_documents(tempdir.path()).unwrap();
        assert_eq!(documents.len(), 1);
        let document = &documents[0];
        assert_eq!(document.title.as_deref(), Some("Guide"));
        assert_eq!(document.summary.as_deref(), Some("Directory summary"));
        assert_eq!(document.canonical_url.as_deref(), Some("/docs/guide"));
        assert_eq!(document.metadata.get("section"), Some(&json!("docs")));
        assert_eq!(document.content.trim_start(), "# Ignored Heading\n\nBody\n");
    }

    #[test]
    fn git_diff_mode_updates_only_changed_and_removed_files() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }

        let tempdir = tempfile::tempdir().unwrap();
        run_git(tempdir.path(), &["init"]).unwrap();
        run_git(
            tempdir.path(),
            &["config", "user.email", "codex@example.com"],
        )
        .unwrap();
        run_git(tempdir.path(), &["config", "user.name", "Codex"]).unwrap();

        fs::write(tempdir.path().join("a.md"), "# A\n\nAlpha\n").unwrap();
        fs::write(tempdir.path().join("b.md"), "# B\n\nBeta\n").unwrap();
        run_git(tempdir.path(), &["add", "."]).unwrap();
        run_git(tempdir.path(), &["commit", "-m", "init"]).unwrap();

        let cache = tempdir.path().join("build-cache.sqlite");
        let options = BuildArtifactOptions::default();
        let first = update_cache_from_directory_with_mode(
            tempdir.path(),
            &cache,
            options.clone(),
            DirectoryUpdateMode::FullScan,
        )
        .unwrap();
        assert_eq!(first.new_document_count, 2);

        fs::write(tempdir.path().join("a.md"), "# A\n\nAlpha updated\n").unwrap();
        fs::remove_file(tempdir.path().join("b.md")).unwrap();
        fs::write(tempdir.path().join("c.md"), "# C\n\nGamma\n").unwrap();

        let second = update_cache_from_directory_with_mode(
            tempdir.path(),
            &cache,
            options,
            DirectoryUpdateMode::GitDiff {
                base_revision: None,
            },
        )
        .unwrap();
        assert_eq!(second.changed_document_count, 1);
        assert_eq!(second.new_document_count, 1);
        assert_eq!(second.removed_document_count, 1);
        assert_eq!(second.active_document_count, 2);
    }

    fn run_git(root: &Path, args: &[&str]) -> Result<()> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow!(
                "git command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
}
