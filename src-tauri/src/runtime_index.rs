use crate::indexer::{
    first_layer_scan, parse_text_content, ContentAction, FileIndexRecord, IndexerPolicy,
    ParseStatus, StdFileSystemScanner,
};
use crate::search::{search_file_name_index, FileNameSearchIndexRecord, SearchHit, SearchQuery};
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSearchIndexRecord {
    pub file: FileIndexRecord,
    pub content: Option<String>,
    normalized_content: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexBuildSummary {
    pub indexed_files: usize,
    pub content_files: usize,
    pub parse_errors: usize,
}

#[derive(Debug, Default)]
pub struct LocalSearchIndex {
    records: Vec<LocalSearchIndexRecord>,
}

impl LocalSearchIndex {
    pub fn rebuild_from_policy(policy: &IndexerPolicy) -> (Self, IndexBuildSummary) {
        let entries = StdFileSystemScanner::scan(policy);
        let files = first_layer_scan(policy, entries);
        Self::from_records(policy, files)
    }

    pub fn from_records(
        policy: &IndexerPolicy,
        files: Vec<FileIndexRecord>,
    ) -> (Self, IndexBuildSummary) {
        let mut parse_errors = 0;
        let records: Vec<LocalSearchIndexRecord> = files
            .into_iter()
            .map(|file| {
                let content = read_indexable_content(policy, &file, &mut parse_errors);
                LocalSearchIndexRecord {
                    normalized_content: content.as_ref().map(|value| value.to_ascii_lowercase()),
                    file,
                    content,
                }
            })
            .collect();
        let content_files = records
            .iter()
            .filter(|record| record.content.is_some())
            .count();
        let summary = IndexBuildSummary {
            indexed_files: records.len(),
            content_files,
            parse_errors,
        };
        (Self { records }, summary)
    }

    pub fn summary(&self) -> IndexBuildSummary {
        IndexBuildSummary {
            indexed_files: self.records.len(),
            content_files: self
                .records
                .iter()
                .filter(|record| record.content.is_some())
                .count(),
            parse_errors: 0,
        }
    }

    pub fn search(&self, query: &SearchQuery, limit: usize) -> Vec<SearchHit> {
        let file_index: Vec<FileNameSearchIndexRecord> = self
            .records
            .iter()
            .map(|record| FileNameSearchIndexRecord::from_file_record(&record.file))
            .collect();
        let name_matches = search_file_name_index(&file_index, query, limit);

        if query
            .content
            .as_ref()
            .is_some_and(|value| !value.is_empty())
        {
            return self.search_content(query, limit);
        }

        if !name_matches.is_empty() || query.text.as_ref().is_none_or(|value| value.is_empty()) {
            return name_matches;
        }

        let content_matches = self.search_content(query, limit);
        merge_hits(name_matches, content_matches, limit)
    }

    fn search_content(&self, query: &SearchQuery, limit: usize) -> Vec<SearchHit> {
        let needle = query
            .content
            .as_ref()
            .or(query.text.as_ref())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let extension = query
            .extension
            .as_ref()
            .map(|value| value.trim().trim_start_matches('.').to_ascii_lowercase());

        if needle.is_empty() {
            return Vec::new();
        }

        self.records
            .iter()
            .filter(|record| {
                if let Some(extension) = &extension {
                    if record
                        .file
                        .extension
                        .trim_start_matches('.')
                        .to_ascii_lowercase()
                        != *extension
                    {
                        return false;
                    }
                }

                record
                    .normalized_content
                    .as_ref()
                    .is_some_and(|content| content.contains(&needle))
            })
            .take(limit)
            .map(|record| SearchHit {
                physical_path: record.file.physical_path.clone(),
                file_name: record.file.file_name.clone(),
                extension: record.file.extension.clone(),
                modified_at: record.file.modified_at,
            })
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct RuntimeIndexState {
    index: Mutex<LocalSearchIndex>,
}

impl RuntimeIndexState {
    pub fn rebuild(&self, policy: &IndexerPolicy) -> IndexBuildSummary {
        let (index, summary) = LocalSearchIndex::rebuild_from_policy(policy);
        if let Ok(mut guard) = self.index.lock() {
            *guard = index;
        }
        summary
    }

    pub fn summary(&self) -> IndexBuildSummary {
        self.index
            .lock()
            .map(|index| index.summary())
            .unwrap_or_default()
    }

    pub fn search(&self, query: &SearchQuery, limit: usize) -> Vec<SearchHit> {
        self.index
            .lock()
            .map(|index| index.search(query, limit))
            .unwrap_or_default()
    }
}

fn read_indexable_content(
    policy: &IndexerPolicy,
    file: &FileIndexRecord,
    parse_errors: &mut usize,
) -> Option<String> {
    if file.content_action != ContentAction::QueueParse
        || !policy.text_extensions.contains(&file.extension)
    {
        return None;
    }

    let Ok(bytes) = std::fs::read(&file.physical_path) else {
        *parse_errors += 1;
        return None;
    };

    if let ParseStatus::ParseError { .. } = parse_text_content(policy, &bytes) {
        *parse_errors += 1;
        return None;
    }

    String::from_utf8(bytes).ok()
}

fn merge_hits(
    mut primary: Vec<SearchHit>,
    secondary: Vec<SearchHit>,
    limit: usize,
) -> Vec<SearchHit> {
    for hit in secondary {
        if primary
            .iter()
            .any(|existing| existing.physical_path == hit.physical_path)
        {
            continue;
        }
        primary.push(hit);
        if primary.len() >= limit {
            break;
        }
    }
    primary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::ContentAction;

    #[test]
    fn rebuilds_real_file_index_and_finds_text_content() {
        let root = unique_temp_dir("maisou-runtime-index");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/roadmap.md");
        std::fs::write(&target, "Task 5 makes local content searchable").expect("fixture");
        std::fs::write(root.join("notes/binary.enc"), "secret").expect("encrypted fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );

        let (index, summary) = LocalSearchIndex::rebuild_from_policy(&policy);
        let hits = index.search(
            &SearchQuery {
                name: None,
                content: Some("local content".to_string()),
                extension: Some("md".to_string()),
                text: None,
            },
            10,
        );

        assert_eq!(summary.indexed_files, 2);
        assert_eq!(summary.content_files, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_name, "roadmap.md");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn plain_text_query_falls_back_to_content_when_filename_does_not_match() {
        let policy = test_policy_with_roots(vec!["/tmp".to_string()], vec!["/tmp".to_string()]);
        let records = vec![FileIndexRecord {
            physical_path: "/tmp/notes/architecture.md".to_string(),
            file_name: "architecture.md".to_string(),
            extension: ".md".to_string(),
            modified_at: 1,
            size_bytes: 10,
            content_action: ContentAction::QueueParse,
        }];
        let index = LocalSearchIndex {
            records: vec![LocalSearchIndexRecord {
                file: records[0].clone(),
                content: Some("Everything style instant search".to_string()),
                normalized_content: Some("everything style instant search".to_string()),
            }],
        };

        let hits = index.search(
            &SearchQuery {
                name: None,
                content: None,
                extension: None,
                text: Some("instant".to_string()),
            },
            10,
        );

        assert_eq!(policy.parse_error_code, "Parse_Error");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_name, "architecture.md");
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{}-{}",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        path
    }

    fn test_policy_with_roots(
        scan_roots: Vec<String>,
        opt_in_watch_roots: Vec<String>,
    ) -> IndexerPolicy {
        IndexerPolicy {
            physical_id: "local.indexer.policy".to_string(),
            professional_description: "Runtime index test policy".to_string(),
            scan_roots,
            opt_in_watch_roots,
            exclude_path_fragments: vec!["/.git/".to_string(), "/node_modules/".to_string()],
            max_parse_size_bytes: 52_428_800,
            text_extensions: vec![".txt".to_string(), ".md".to_string(), ".json".to_string()],
            encrypted_extensions: vec![".gpg".to_string(), ".enc".to_string()],
            parse_error_code: "Parse_Error".to_string(),
        }
    }
}
