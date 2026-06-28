use crate::indexer::FileIndexRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub name: Option<String>,
    pub content: Option<String>,
    pub extension: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub physical_path: String,
    pub file_name: String,
    pub extension: String,
    pub modified_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNameSearchIndexRecord {
    pub physical_path: String,
    pub file_name: String,
    pub extension: String,
    pub modified_at: u64,
    normalized_path: String,
    normalized_file_name: String,
    normalized_extension: String,
}

pub fn build_file_name_search_index(records: &[FileIndexRecord]) -> Vec<FileNameSearchIndexRecord> {
    records
        .iter()
        .map(|record| FileNameSearchIndexRecord {
            physical_path: record.physical_path.clone(),
            file_name: record.file_name.clone(),
            extension: record.extension.clone(),
            modified_at: record.modified_at,
            normalized_path: record.physical_path.to_ascii_lowercase(),
            normalized_file_name: record.file_name.to_ascii_lowercase(),
            normalized_extension: normalize_extension(&record.extension),
        })
        .collect()
}

pub fn search_file_names(
    records: &[FileIndexRecord],
    query: &SearchQuery,
    limit: usize,
) -> Vec<SearchHit> {
    let index = build_file_name_search_index(records);
    search_file_name_index(&index, query, limit)
}

pub fn search_file_name_index(
    records: &[FileNameSearchIndexRecord],
    query: &SearchQuery,
    limit: usize,
) -> Vec<SearchHit> {
    let normalized_query = NormalizedSearchQuery::from(query);

    records
        .iter()
        .filter(|record| matches_query(record, &normalized_query))
        .take(limit)
        .map(|record| SearchHit {
            physical_path: record.physical_path.clone(),
            file_name: record.file_name.clone(),
            extension: record.extension.clone(),
            modified_at: record.modified_at,
        })
        .collect()
}

struct NormalizedSearchQuery {
    name: Option<String>,
    extension: Option<String>,
    text: Option<String>,
}

impl From<&SearchQuery> for NormalizedSearchQuery {
    fn from(query: &SearchQuery) -> Self {
        Self {
            name: query.name.as_ref().map(|value| value.to_ascii_lowercase()),
            extension: query
                .extension
                .as_ref()
                .map(|value| normalize_extension(value)),
            text: query.text.as_ref().map(|value| value.to_ascii_lowercase()),
        }
    }
}

impl FileNameSearchIndexRecord {
    pub fn from_file_record(record: &FileIndexRecord) -> Self {
        Self {
            physical_path: record.physical_path.clone(),
            file_name: record.file_name.clone(),
            extension: record.extension.clone(),
            modified_at: record.modified_at,
            normalized_path: record.physical_path.to_ascii_lowercase(),
            normalized_file_name: record.file_name.to_ascii_lowercase(),
            normalized_extension: normalize_extension(&record.extension),
        }
    }
}

fn matches_query(record: &FileNameSearchIndexRecord, query: &NormalizedSearchQuery) -> bool {
    if let Some(extension) = &query.extension {
        if record.normalized_extension != *extension {
            return false;
        }
    }

    if let Some(name) = &query.name {
        if !record.normalized_file_name.contains(name) {
            return false;
        }
    }

    if let Some(text) = &query.text {
        if !record.normalized_file_name.contains(text) && !record.normalized_path.contains(text) {
            return false;
        }
    }

    true
}

fn normalize_extension(value: &str) -> String {
    value.trim().trim_start_matches('.').to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::{ContentAction, FileIndexRecord};
    use std::time::Instant;

    #[test]
    fn filters_filename_index_by_field_directives() {
        let records = vec![
            record("/work/roadmap.md", "roadmap.md", ".md"),
            record("/work/prompt.json", "prompt.json", ".json"),
        ];
        let query = SearchQuery {
            name: Some("road".to_string()),
            content: None,
            extension: Some("md".to_string()),
            text: None,
        };

        let hits = search_file_names(&records, &query, 20);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_name, "roadmap.md");
    }

    #[test]
    fn filename_search_stays_under_twenty_milliseconds_for_local_window() {
        let records: Vec<FileIndexRecord> = (0..10_000)
            .map(|index| {
                let extension = if index % 2 == 0 { ".md" } else { ".json" };
                record(
                    &format!("/work/project/file-{index}{extension}"),
                    &format!("file-{index}{extension}"),
                    extension,
                )
            })
            .collect();
        let query = SearchQuery {
            name: Some("file-99".to_string()),
            content: None,
            extension: Some("md".to_string()),
            text: None,
        };

        let index = build_file_name_search_index(&records);
        let started_at = Instant::now();
        let hits = search_file_name_index(&index, &query, 50);
        let elapsed = started_at.elapsed();

        assert!(!hits.is_empty());
        assert!(
            elapsed.as_millis() <= 20,
            "filename search took {}ms",
            elapsed.as_millis()
        );
    }

    fn record(path: &str, name: &str, extension: &str) -> FileIndexRecord {
        FileIndexRecord {
            physical_path: path.to_string(),
            file_name: name.to_string(),
            extension: extension.to_string(),
            modified_at: 1,
            size_bytes: 10,
            content_action: ContentAction::FilenameOnly,
        }
    }
}
