use crate::indexer::{first_layer_scan, parse_content_for_extension, IndexerPolicy, ParseStatus};
use crate::runtime_index::{IndexBuildSummary, RuntimeIndexState};
use crate::search::{search_file_names, SearchHit, SearchQuery};
use std::process::Command;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchFileNamesRequest {
    pub query: SearchCommandQuery,
    pub limit: usize,
    pub mode: SearchMode,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Filename,
    Content,
    Mixed,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchCommandQuery {
    pub name: Option<String>,
    pub content: Option<String>,
    pub extension: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchFileNameHit {
    pub id: String,
    pub name: String,
    pub path: String,
    pub extension: String,
    pub modified_at: u64,
    pub size_bytes: u64,
    pub kind: String,
    pub hit_source: String,
    pub score: f64,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReadPreviewContentRequest {
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreviewContent {
    pub content: String,
    pub source: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FileActionRequest {
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchIndexStatus {
    pub indexed_files: usize,
    pub content_files: usize,
    pub parse_errors: usize,
    pub content_queue_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IndexerPolicySettings {
    pub scan_roots: Vec<String>,
    pub watch_roots: Vec<String>,
    pub exclude_path_fragments: Vec<String>,
    pub max_parse_size_bytes: u64,
    pub text_extensions: Vec<String>,
    pub content_index: ContentIndexSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContentIndexSettings {
    pub enabled: bool,
    pub batch_size: u64,
    pub snippet_radius: u64,
    pub default_limit: u64,
}

#[tauri::command]
pub fn search_file_names_command(
    state: tauri::State<'_, RuntimeIndexState>,
    request: SearchFileNamesRequest,
) -> Vec<SearchFileNameHit> {
    let policy = default_runtime_indexer_policy();
    let query = SearchQuery {
        name: request.query.name,
        content: request.query.content,
        extension: request.query.extension,
        text: request.query.text,
    };

    if state.summary().indexed_files == 0 {
        state.rebuild(&policy);
    }

    let runtime_hits = state.search(&query, request.limit);
    let hits = if runtime_hits.is_empty() {
        let entries = crate::indexer::StdFileSystemScanner::scan(&policy);
        let records = first_layer_scan(&policy, entries);
        search_file_names(&records, &query, request.limit)
    } else {
        runtime_hits
    };

    search_hits_to_command_hits(filter_hits_by_mode(hits, &request.mode))
}

#[tauri::command]
pub fn rebuild_search_index_command(
    state: tauri::State<'_, RuntimeIndexState>,
) -> SearchIndexStatus {
    let policy = default_runtime_indexer_policy();
    SearchIndexStatus::from(state.rebuild(&policy))
}

#[tauri::command]
pub fn get_search_index_status_command(
    state: tauri::State<'_, RuntimeIndexState>,
) -> SearchIndexStatus {
    SearchIndexStatus::from(state.summary())
}

#[tauri::command]
pub fn get_indexer_policy_settings_command() -> IndexerPolicySettings {
    let policy = default_runtime_indexer_policy();
    indexer_policy_to_settings(&policy)
}

#[tauri::command]
pub fn read_preview_content_command(
    request: ReadPreviewContentRequest,
) -> Result<PreviewContent, String> {
    let policy = default_runtime_indexer_policy();
    let metadata = std::fs::metadata(&request.path).map_err(|error| error.to_string())?;

    if metadata.len() > policy.max_parse_size_bytes {
        return Err("preview file exceeds configured parse size".to_string());
    }

    let bytes = std::fs::read(&request.path).map_err(|error| error.to_string())?;
    if let ParseStatus::ParseError { code } =
        parse_content_for_extension(&policy, &extension_of(&request.path), &bytes)
    {
        return Err(code);
    }

    let content = String::from_utf8(bytes).map_err(|error| error.to_string())?;
    Ok(PreviewContent {
        content,
        source: "file".to_string(),
    })
}

#[tauri::command]
pub fn open_file_command(request: FileActionRequest) -> Result<(), String> {
    run_file_action(build_open_file_command(&request.path))
}

#[tauri::command]
pub fn reveal_file_command(request: FileActionRequest) -> Result<(), String> {
    run_file_action(build_reveal_file_command(&request.path))
}

fn default_runtime_indexer_policy() -> IndexerPolicy {
    IndexerPolicy::from_json(include_str!("../../config/indexer_policy.json"))
        .expect("indexer policy configuration should parse")
}

fn indexer_policy_to_settings(policy: &IndexerPolicy) -> IndexerPolicySettings {
    IndexerPolicySettings {
        scan_roots: policy.scan_roots.clone(),
        watch_roots: policy.opt_in_watch_roots.clone(),
        exclude_path_fragments: policy.exclude_path_fragments.clone(),
        max_parse_size_bytes: policy.max_parse_size_bytes,
        text_extensions: policy.text_extensions.clone(),
        content_index: ContentIndexSettings {
            enabled: policy.content_index.enabled,
            batch_size: policy.content_index.batch_size,
            snippet_radius: policy.content_index.snippet_radius,
            default_limit: policy.content_index.default_limit,
        },
    }
}

fn extension_of(path: &str) -> String {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    match file_name.rsplit_once('.') {
        Some((_, extension)) => format!(".{}", extension.to_ascii_lowercase()),
        None => String::new(),
    }
}

impl From<IndexBuildSummary> for SearchIndexStatus {
    fn from(summary: IndexBuildSummary) -> Self {
        Self {
            indexed_files: summary.indexed_files,
            content_files: summary.content_files,
            parse_errors: summary.parse_errors,
            content_queue_depth: summary.content_queue_depth,
        }
    }
}

fn build_open_file_command(path: &str) -> Command {
    let mut command = Command::new("open");
    command.arg(path);
    command
}

fn build_reveal_file_command(path: &str) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg(path);
    command
}

fn run_file_action(mut command: Command) -> Result<(), String> {
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("file action failed with status {status}"))
    }
}

fn search_hits_to_command_hits(hits: Vec<SearchHit>) -> Vec<SearchFileNameHit> {
    hits.into_iter()
        .map(|hit| SearchFileNameHit {
            id: hit.physical_path.clone(),
            name: hit.file_name,
            path: hit.physical_path,
            extension: hit.extension.trim_start_matches('.').to_string(),
            modified_at: hit.modified_at,
            size_bytes: hit.size_bytes,
            kind: kind_for_extension(&hit.extension),
            hit_source: hit.hit_source,
            score: hit.score,
            snippet: hit.snippet,
        })
        .collect()
}

fn kind_for_extension(extension: &str) -> String {
    let normalized = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let config = include_str!("../../config/search_syntax.json");
    let Some(groups) = extract_json_array(config, "extensionKindGroups") else {
        return "文件".to_string();
    };

    for group in split_json_objects(groups) {
        let Some(kind) = extract_json_string_value(group, "kind") else {
            continue;
        };
        let Some(extensions) = extract_json_array(group, "extensions") else {
            continue;
        };
        if extract_json_string_values(extensions)
            .iter()
            .any(|candidate| {
                candidate
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(&normalized)
            })
        {
            return kind;
        }
    }

    "文件".to_string()
}

fn extract_json_array<'a>(payload: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("\"{key}\"");
    let marker_start = payload.find(&marker)?;
    let after_marker = &payload[marker_start + marker.len()..];
    let array_start = after_marker.find('[')?;
    let mut depth = 0usize;
    let mut start_index = None;

    for (index, character) in after_marker[array_start..].char_indices() {
        if character == '[' {
            if depth == 0 {
                start_index = Some(array_start + index + 1);
            }
            depth += 1;
        } else if character == ']' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                let start = start_index?;
                return Some(&after_marker[start..array_start + index]);
            }
        }
    }

    None
}

fn split_json_objects(payload: &str) -> Vec<&str> {
    let mut objects = Vec::new();
    let mut depth = 0usize;
    let mut start_index = None;

    for (index, character) in payload.char_indices() {
        if character == '{' {
            if depth == 0 {
                start_index = Some(index);
            }
            depth += 1;
        } else if character == '}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                if let Some(start) = start_index.take() {
                    objects.push(&payload[start..=index]);
                }
            }
        }
    }

    objects
}

fn extract_json_string_value(payload: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let marker_start = payload.find(&marker)?;
    let after_marker = &payload[marker_start + marker.len()..];
    let colon = after_marker.find(':')?;
    let after_colon = after_marker[colon + 1..].trim_start();
    let value_start = after_colon.strip_prefix('"')?;
    let value_end = value_start.find('"')?;

    Some(value_start[..value_end].to_string())
}

fn extract_json_string_values(payload: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut remainder = payload;

    while let Some(start) = remainder.find('"') {
        let after_start = &remainder[start + 1..];
        let Some(end) = after_start.find('"') else {
            break;
        };
        values.push(after_start[..end].to_string());
        remainder = &after_start[end + 1..];
    }

    values
}

fn filter_hits_by_mode(hits: Vec<SearchHit>, mode: &SearchMode) -> Vec<SearchHit> {
    hits.into_iter()
        .filter(|hit| match mode {
            SearchMode::Filename => hit.hit_source == crate::search::HIT_SOURCE_FILENAME,
            SearchMode::Content => hit.hit_source == crate::search::HIT_SOURCE_CONTENT,
            SearchMode::Mixed => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn search_file_names_command_preserves_filename_and_content_result_metadata() {
        let hits = search_hits_to_command_hits(vec![
            SearchHit {
                physical_path: "/work/roadmap.md".to_string(),
                file_name: "roadmap.md".to_string(),
                extension: ".md".to_string(),
                modified_at: 1,
                size_bytes: 4096,
                hit_source: "filename".to_string(),
                score: 0.0,
                snippet: None,
            },
            SearchHit {
                physical_path: "/work/notes.md".to_string(),
                file_name: "notes.md".to_string(),
                extension: ".md".to_string(),
                modified_at: 2,
                size_bytes: 512,
                hit_source: "content".to_string(),
                score: -1.25,
                snippet: Some("...AnyTXT content...".to_string()),
            },
        ]);

        assert_eq!(hits[0].modified_at, 1);
        assert_eq!(hits[0].size_bytes, 4096);
        assert_eq!(hits[0].kind, "文档");
        assert_eq!(hits[0].hit_source, "filename");
        assert_eq!(hits[0].score, 0.0);
        assert_eq!(hits[0].snippet, None);
        assert_eq!(hits[1].modified_at, 2);
        assert_eq!(hits[1].size_bytes, 512);
        assert_eq!(hits[1].kind, "文档");
        assert_eq!(hits[1].hit_source, "content");
        assert_eq!(hits[1].score, -1.25);
        assert_eq!(hits[1].snippet.as_deref(), Some("...AnyTXT content..."));
    }

    #[test]
    fn filters_command_hits_by_requested_search_mode() {
        let hits = vec![
            SearchHit {
                physical_path: "/work/roadmap.md".to_string(),
                file_name: "roadmap.md".to_string(),
                extension: ".md".to_string(),
                modified_at: 1,
                size_bytes: 4096,
                hit_source: "filename".to_string(),
                score: 0.0,
                snippet: None,
            },
            SearchHit {
                physical_path: "/work/notes.md".to_string(),
                file_name: "notes.md".to_string(),
                extension: ".md".to_string(),
                modified_at: 2,
                size_bytes: 512,
                hit_source: "content".to_string(),
                score: -1.25,
                snippet: Some("...AnyTXT content...".to_string()),
            },
        ];

        let filename_hits = filter_hits_by_mode(hits.clone(), &SearchMode::Filename);
        let content_hits = filter_hits_by_mode(hits.clone(), &SearchMode::Content);
        let mixed_hits = filter_hits_by_mode(hits, &SearchMode::Mixed);

        assert_eq!(filename_hits.len(), 1);
        assert_eq!(filename_hits[0].hit_source, "filename");
        assert_eq!(content_hits.len(), 1);
        assert_eq!(content_hits[0].hit_source, "content");
        assert_eq!(mixed_hits.len(), 2);
    }

    #[test]
    fn search_index_status_includes_content_queue_depth() {
        let status = SearchIndexStatus::from(IndexBuildSummary {
            indexed_files: 12,
            content_files: 7,
            parse_errors: 1,
            content_queue_depth: 3,
        });

        assert_eq!(status.indexed_files, 12);
        assert_eq!(status.content_files, 7);
        assert_eq!(status.parse_errors, 1);
        assert_eq!(status.content_queue_depth, 3);
    }

    #[test]
    fn indexer_policy_settings_exposes_configured_read_only_fields() {
        let policy = IndexerPolicy {
            physical_id: "local.indexer.policy".to_string(),
            professional_description: "Local index policy".to_string(),
            scan_roots: vec!["~/Documents".to_string()],
            opt_in_watch_roots: vec!["~/Documents/Notes".to_string()],
            exclude_path_fragments: vec!["/.git/".to_string()],
            max_parse_size_bytes: 4096,
            text_extensions: vec![".md".to_string(), ".txt".to_string()],
            encrypted_extensions: vec![".gpg".to_string()],
            parse_error_code: "Parse_Error".to_string(),
            content_index: crate::indexer::ContentIndexPolicy {
                enabled: true,
                batch_size: 32,
                snippet_radius: 120,
                default_limit: 20,
            },
        };

        let settings = indexer_policy_to_settings(&policy);

        assert_eq!(settings.scan_roots, vec!["~/Documents"]);
        assert_eq!(settings.watch_roots, vec!["~/Documents/Notes"]);
        assert_eq!(settings.exclude_path_fragments, vec!["/.git/"]);
        assert_eq!(settings.max_parse_size_bytes, 4096);
        assert_eq!(settings.text_extensions, vec![".md", ".txt"]);
        assert_eq!(settings.content_index.enabled, true);
        assert_eq!(settings.content_index.batch_size, 32);
        assert_eq!(settings.content_index.snippet_radius, 120);
        assert_eq!(settings.content_index.default_limit, 20);
    }

    #[test]
    fn read_preview_content_returns_text_for_existing_small_file() {
        let path = std::env::temp_dir().join("maisou-preview-small.txt");
        fs::write(&path, "before\nPhase 3 target\nafter").expect("fixture should write");

        let preview = read_preview_content_command(ReadPreviewContentRequest {
            path: path.to_string_lossy().to_string(),
        })
        .expect("preview content should read");

        assert_eq!(preview.content, "before\nPhase 3 target\nafter");
        assert_eq!(preview.source, "file");
    }

    #[test]
    fn read_preview_content_rejects_files_above_configured_limit() {
        let path = std::env::temp_dir().join("maisou-preview-large.txt");
        let oversized = vec![b'x'; 52_428_801];
        fs::write(&path, oversized).expect("fixture should write");

        let error = read_preview_content_command(ReadPreviewContentRequest {
            path: path.to_string_lossy().to_string(),
        })
        .expect_err("oversized preview should be rejected");

        assert_eq!(error, "preview file exceeds configured parse size");
    }

    #[test]
    fn read_preview_content_marks_malformed_text_with_configured_parse_error() {
        let path = std::env::temp_dir().join("maisou-preview-malformed.txt");
        fs::write(&path, [0xff, 0xfe, 0xfd]).expect("fixture should write");

        let error = read_preview_content_command(ReadPreviewContentRequest {
            path: path.to_string_lossy().to_string(),
        })
        .expect_err("malformed text should be rejected");

        assert_eq!(error, "Parse_Error");
    }

    #[test]
    fn builds_macos_open_file_command_without_shell_interpolation() {
        let command = build_open_file_command("/tmp/麦搜 test.md");

        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command
                .get_args()
                .map(|value| value.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["/tmp/麦搜 test.md"]
        );
    }

    #[test]
    fn builds_macos_reveal_file_command_with_finder_selection_flag() {
        let command = build_reveal_file_command("/tmp/麦搜 test.md");

        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command
                .get_args()
                .map(|value| value.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["-R", "/tmp/麦搜 test.md"]
        );
    }
}
