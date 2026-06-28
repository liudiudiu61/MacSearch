use crate::indexer::{first_layer_scan, parse_text_content, IndexerPolicy, ParseStatus};
use crate::runtime_index::{IndexBuildSummary, RuntimeIndexState};
use crate::search::{search_file_names, SearchQuery};
use std::process::Command;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SearchFileNamesRequest {
    pub query: SearchCommandQuery,
    pub limit: usize,
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

    hits.into_iter()
        .map(|hit| SearchFileNameHit {
            id: hit.physical_path.clone(),
            name: hit.file_name,
            path: hit.physical_path,
            extension: hit.extension.trim_start_matches('.').to_string(),
            modified_at: hit.modified_at,
        })
        .collect()
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
pub fn read_preview_content_command(
    request: ReadPreviewContentRequest,
) -> Result<PreviewContent, String> {
    let policy = default_runtime_indexer_policy();
    let metadata = std::fs::metadata(&request.path).map_err(|error| error.to_string())?;

    if metadata.len() > policy.max_parse_size_bytes {
        return Err("preview file exceeds configured parse size".to_string());
    }

    let bytes = std::fs::read(&request.path).map_err(|error| error.to_string())?;
    if let ParseStatus::ParseError { code } = parse_text_content(&policy, &bytes) {
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

impl From<IndexBuildSummary> for SearchIndexStatus {
    fn from(summary: IndexBuildSummary) -> Self {
        Self {
            indexed_files: summary.indexed_files,
            content_files: summary.content_files,
            parse_errors: summary.parse_errors,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
