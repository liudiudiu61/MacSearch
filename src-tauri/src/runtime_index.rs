use crate::content_store::{ContentDocument, ContentStore};
use crate::indexer::{
    enqueue_fs_event_with_content_gate, first_layer_scan, ContentAction, FileIndexRecord, FsEvent,
    IndexerPolicy, ParseQueue, StdFileSystemScanner,
};
use crate::parsers::ParserRegistry;
use crate::search::{
    search_file_name_index, FileNameSearchIndexRecord, SearchHit, SearchQuery, HIT_SOURCE_CONTENT,
};
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
    pub content_queue_depth: usize,
}

#[derive(Debug, Default)]
pub struct LocalSearchIndex {
    records: Vec<LocalSearchIndexRecord>,
    snippet_radius: usize,
}

impl LocalSearchIndex {
    pub fn rebuild_from_policy(policy: &IndexerPolicy) -> (Self, IndexBuildSummary) {
        Self::rebuild_from_policy_with_content_store(policy, None, policy.content_index.enabled)
    }

    pub fn rebuild_from_policy_with_content_store(
        policy: &IndexerPolicy,
        content_store: Option<&ContentStore>,
        allow_content_indexing: bool,
    ) -> (Self, IndexBuildSummary) {
        let entries = StdFileSystemScanner::scan(policy);
        let files = first_layer_scan(policy, entries);
        Self::from_records_with_content_store(policy, files, content_store, allow_content_indexing)
    }

    pub fn from_records(
        policy: &IndexerPolicy,
        files: Vec<FileIndexRecord>,
    ) -> (Self, IndexBuildSummary) {
        Self::from_records_with_content_store(policy, files, None, policy.content_index.enabled)
    }

    pub fn from_records_with_content_store(
        policy: &IndexerPolicy,
        files: Vec<FileIndexRecord>,
        content_store: Option<&ContentStore>,
        allow_content_indexing: bool,
    ) -> (Self, IndexBuildSummary) {
        let mut parse_errors = 0;
        let records: Vec<LocalSearchIndexRecord> = files
            .into_iter()
            .map(|file| {
                let content = if allow_content_indexing {
                    read_indexable_content(policy, &file, &mut parse_errors)
                } else {
                    None
                };
                if let (Some(store), Some(content)) = (content_store, content.as_ref()) {
                    if store
                        .upsert(ContentDocument {
                            physical_path: file.physical_path.clone(),
                            file_name: file.file_name.clone(),
                            extension: file.extension.clone(),
                            modified_at: file.modified_at,
                            content: content.clone(),
                        })
                        .is_err()
                    {
                        parse_errors += 1;
                    }
                }
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
            content_queue_depth: 0,
        };
        (
            Self {
                records,
                snippet_radius: policy.content_index.snippet_radius as usize,
            },
            summary,
        )
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
            content_queue_depth: 0,
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

        if query.text.as_ref().is_none_or(|value| value.is_empty()) {
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
                size_bytes: record.file.size_bytes,
                hit_source: HIT_SOURCE_CONTENT.to_string(),
                score: 0.0,
                snippet: record.content.as_ref().map(|content| {
                    build_content_snippet(content, &needle, self.snippet_radius.min(content.len()))
                }),
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

    pub fn apply_fs_event(
        &self,
        policy: &IndexerPolicy,
        event: FsEvent,
        content_store: Option<&ContentStore>,
        allow_content_indexing: bool,
    ) -> IndexBuildSummary {
        if let FsEvent::Removed { path } = event {
            let mut parse_errors = 0;
            if let Some(store) = content_store {
                if store.delete_path(&path).is_err() {
                    parse_errors += 1;
                }
            }
            if let Ok(mut index) = self.index.lock() {
                index
                    .records
                    .retain(|record| record.file.physical_path != path);
                return IndexBuildSummary {
                    parse_errors,
                    content_queue_depth: 0,
                    ..index.summary()
                };
            }
            return IndexBuildSummary {
                parse_errors,
                content_queue_depth: 0,
                ..IndexBuildSummary::default()
            };
        }

        let mut queue = ParseQueue::new();
        let Some(file) =
            enqueue_fs_event_with_content_gate(policy, &mut queue, event, allow_content_indexing)
        else {
            return self.summary();
        };

        let mut parse_errors = 0;
        let content = if allow_content_indexing && queue.pop().is_some() {
            read_indexable_content(policy, &file, &mut parse_errors)
        } else {
            None
        };
        let content_queue_depth = queue.len();
        if let Some(store) = content_store {
            if let Some(content) = content.as_ref() {
                if store
                    .upsert(ContentDocument {
                        physical_path: file.physical_path.clone(),
                        file_name: file.file_name.clone(),
                        extension: file.extension.clone(),
                        modified_at: file.modified_at,
                        content: content.clone(),
                    })
                    .is_err()
                {
                    parse_errors += 1;
                }
            } else if store.delete_path(&file.physical_path).is_err() {
                parse_errors += 1;
            }
        }

        if let Ok(mut index) = self.index.lock() {
            index.snippet_radius = policy.content_index.snippet_radius as usize;
            index
                .records
                .retain(|record| record.file.physical_path != file.physical_path);
            index.records.push(LocalSearchIndexRecord {
                normalized_content: content.as_ref().map(|value| value.to_ascii_lowercase()),
                file,
                content,
            });
            return IndexBuildSummary {
                parse_errors,
                content_queue_depth,
                ..index.summary()
            };
        }

        IndexBuildSummary {
            parse_errors,
            content_queue_depth,
            ..IndexBuildSummary::default()
        }
    }
}

fn read_indexable_content(
    policy: &IndexerPolicy,
    file: &FileIndexRecord,
    parse_errors: &mut usize,
) -> Option<String> {
    if file.content_action != ContentAction::QueueParse {
        return None;
    }

    let Ok(bytes) = std::fs::read(&file.physical_path) else {
        *parse_errors += 1;
        return None;
    };

    ParserRegistry::from_text_extensions(
        policy.text_extensions.clone(),
        policy.parse_error_code.clone(),
    )
    .parse_extension(&file.extension, &bytes)
    .map_err(|_| {
        *parse_errors += 1;
    })
    .ok()
}

fn merge_hits(
    mut primary: Vec<SearchHit>,
    secondary: Vec<SearchHit>,
    limit: usize,
) -> Vec<SearchHit> {
    for hit in secondary {
        if let Some(existing) = primary
            .iter()
            .position(|existing| existing.physical_path == hit.physical_path)
        {
            if primary[existing].snippet.is_none() && hit.snippet.is_some() {
                primary[existing].hit_source = hit.hit_source;
                primary[existing].score = hit.score;
                primary[existing].snippet = hit.snippet;
            }
            continue;
        }
        primary.push(hit);
        if primary.len() >= limit {
            break;
        }
    }
    primary
}

fn build_content_snippet(content: &str, needle: &str, radius: usize) -> String {
    let normalized_content = content.to_ascii_lowercase();
    let normalized_needle = needle.to_ascii_lowercase();
    let match_start = normalized_content
        .find(&normalized_needle)
        .or_else(|| {
            normalized_needle
                .split_whitespace()
                .find_map(|term| normalized_content.find(term))
        })
        .unwrap_or(0);
    let match_end = match_start.saturating_add(normalized_needle.len());
    let start = nearest_char_boundary_before(content, match_start.saturating_sub(radius));
    let end = nearest_char_boundary_after(content, (match_end + radius).min(content.len()));
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < content.len() { "..." } else { "" };
    format!("{prefix}{}{suffix}", &content[start..end])
}

fn nearest_char_boundary_before(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn nearest_char_boundary_after(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index < value.len() && !value.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_store::{ContentSearchQuery, ContentStore};
    use crate::indexer::{ContentAction, ContentIndexPolicy, FsEvent};

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
    fn rebuilds_persistent_content_store_from_indexable_text_files() {
        let root = unique_temp_dir("maisou-runtime-content-store");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/roadmap.md");
        std::fs::write(&target, "Persistent AnyTXT content lands in FTS5").expect("fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-content-store-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let db_path = db_root.join("content.sqlite");
        let store = ContentStore::open(&db_path).expect("content store opens");

        let (_index, summary) =
            LocalSearchIndex::rebuild_from_policy_with_content_store(&policy, Some(&store), true);
        let hits = store
            .search(&ContentSearchQuery {
                needle: "AnyTXT content".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("content store search succeeds");

        assert_eq!(summary.indexed_files, 1);
        assert_eq!(summary.content_files, 1);
        assert_eq!(summary.parse_errors, 0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file_name, "roadmap.md");
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
    }

    #[test]
    fn suspended_rebuild_keeps_filename_index_without_persistent_content() {
        let root = unique_temp_dir("maisou-runtime-suspended-content-store");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        std::fs::write(
            root.join("notes/roadmap.md"),
            "Suspended content should not index",
        )
        .expect("fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-suspended-content-store-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let store = ContentStore::open(&db_root.join("content.sqlite")).expect("store opens");

        let (index, summary) =
            LocalSearchIndex::rebuild_from_policy_with_content_store(&policy, Some(&store), false);
        let name_hits = index.search(
            &SearchQuery {
                name: Some("roadmap".to_string()),
                content: None,
                extension: Some("md".to_string()),
                text: None,
            },
            10,
        );
        let content_hits = store
            .search(&ContentSearchQuery {
                needle: "Suspended content".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("store search succeeds");

        assert_eq!(summary.indexed_files, 1);
        assert_eq!(summary.content_files, 0);
        assert_eq!(name_hits.len(), 1);
        assert!(content_hits.is_empty());
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
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
            snippet_radius: policy.content_index.snippet_radius as usize,
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
        assert_eq!(hits[0].hit_source, "content");
        assert_eq!(
            hits[0].snippet.as_deref(),
            Some("Everything style instant search")
        );
    }

    #[test]
    fn plain_text_query_merges_filename_hits_before_content_hits_and_deduplicates_paths() {
        let policy = test_policy_with_roots(vec!["/tmp".to_string()], vec!["/tmp".to_string()]);
        let index = LocalSearchIndex {
            records: vec![
                LocalSearchIndexRecord {
                    file: FileIndexRecord {
                        physical_path: "/tmp/notes/alpha.md".to_string(),
                        file_name: "alpha instant.md".to_string(),
                        extension: ".md".to_string(),
                        modified_at: 1,
                        size_bytes: 10,
                        content_action: ContentAction::QueueParse,
                    },
                    content: Some("instant content also matches".to_string()),
                    normalized_content: Some("instant content also matches".to_string()),
                },
                LocalSearchIndexRecord {
                    file: FileIndexRecord {
                        physical_path: "/tmp/notes/beta.md".to_string(),
                        file_name: "beta.md".to_string(),
                        extension: ".md".to_string(),
                        modified_at: 1,
                        size_bytes: 10,
                        content_action: ContentAction::QueueParse,
                    },
                    content: Some("instant only lives in content".to_string()),
                    normalized_content: Some("instant only lives in content".to_string()),
                },
            ],
            snippet_radius: policy.content_index.snippet_radius as usize,
        };

        let hits = index.search(
            &SearchQuery {
                name: None,
                content: None,
                extension: Some("md".to_string()),
                text: Some("instant".to_string()),
            },
            10,
        );

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].file_name, "alpha instant.md");
        assert_eq!(hits[0].hit_source, "content");
        assert!(hits[0].snippet.is_some());
        assert_eq!(hits[1].file_name, "beta.md");
        assert_eq!(hits[1].hit_source, "content");
        assert!(hits[1].snippet.is_some());
    }

    #[test]
    fn incremental_created_text_file_updates_filename_and_content_store() {
        let root = unique_temp_dir("maisou-runtime-incremental-created");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/created.md");
        let content = "Created AnyTXT content joins the full text index";
        std::fs::write(&target, content).expect("fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-incremental-created-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let store = ContentStore::open(&db_root.join("content.sqlite")).expect("store opens");
        let state = RuntimeIndexState::default();

        let summary = state.apply_fs_event(
            &policy,
            FsEvent::Created {
                path: target.to_string_lossy().to_string(),
                size_bytes: content.len() as u64,
                modified_at: 1,
            },
            Some(&store),
            true,
        );
        let name_hits = state.search(
            &SearchQuery {
                name: Some("created".to_string()),
                content: None,
                extension: Some("md".to_string()),
                text: None,
            },
            10,
        );
        let content_hits = store
            .search(&ContentSearchQuery {
                needle: "AnyTXT content".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("store search succeeds");

        assert_eq!(summary.indexed_files, 1);
        assert_eq!(summary.content_files, 1);
        assert_eq!(summary.parse_errors, 0);
        assert_eq!(name_hits.len(), 1);
        assert_eq!(content_hits.len(), 1);
        assert_eq!(content_hits[0].file_name, "created.md");
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
    }

    #[test]
    fn incremental_modified_text_file_replaces_content_store_document() {
        let root = unique_temp_dir("maisou-runtime-incremental-modified");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/updated.md");
        let old_content = "Old incremental content should disappear";
        let new_content = "New incremental content should be searchable";
        std::fs::write(&target, old_content).expect("old fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-incremental-modified-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let store = ContentStore::open(&db_root.join("content.sqlite")).expect("store opens");
        let state = RuntimeIndexState::default();

        state.apply_fs_event(
            &policy,
            FsEvent::Created {
                path: target.to_string_lossy().to_string(),
                size_bytes: old_content.len() as u64,
                modified_at: 1,
            },
            Some(&store),
            true,
        );
        std::fs::write(&target, new_content).expect("new fixture");
        let summary = state.apply_fs_event(
            &policy,
            FsEvent::Modified {
                path: target.to_string_lossy().to_string(),
                size_bytes: new_content.len() as u64,
                modified_at: 2,
            },
            Some(&store),
            true,
        );
        let old_hits = store
            .search(&ContentSearchQuery {
                needle: "Old incremental".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("old search succeeds");
        let new_hits = store
            .search(&ContentSearchQuery {
                needle: "New incremental".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("new search succeeds");

        assert_eq!(summary.indexed_files, 1);
        assert_eq!(summary.content_files, 1);
        assert!(old_hits.is_empty());
        assert_eq!(new_hits.len(), 1);
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
    }

    #[test]
    fn incremental_removed_file_deletes_runtime_and_content_store_rows() {
        let root = unique_temp_dir("maisou-runtime-incremental-removed");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/removed.md");
        let content = "Removed incremental content leaves no index row";
        std::fs::write(&target, content).expect("fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-incremental-removed-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let store = ContentStore::open(&db_root.join("content.sqlite")).expect("store opens");
        let state = RuntimeIndexState::default();
        let physical_path = target.to_string_lossy().to_string();

        state.apply_fs_event(
            &policy,
            FsEvent::Created {
                path: physical_path.clone(),
                size_bytes: content.len() as u64,
                modified_at: 1,
            },
            Some(&store),
            true,
        );
        let summary = state.apply_fs_event(
            &policy,
            FsEvent::Removed {
                path: physical_path.clone(),
            },
            Some(&store),
            true,
        );
        let name_hits = state.search(
            &SearchQuery {
                name: Some("removed".to_string()),
                content: None,
                extension: Some("md".to_string()),
                text: None,
            },
            10,
        );
        let content_hits = store
            .search(&ContentSearchQuery {
                needle: "Removed incremental".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("store search succeeds");

        assert_eq!(summary.indexed_files, 0);
        assert_eq!(summary.content_files, 0);
        assert!(name_hits.is_empty());
        assert!(content_hits.is_empty());
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
    }

    #[test]
    fn incremental_suspended_update_keeps_filename_without_content_parse() {
        let root = unique_temp_dir("maisou-runtime-incremental-suspended");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        let target = root.join("notes/suspended.md");
        let content = "Suspended incremental content must not be parsed";
        std::fs::write(&target, content).expect("fixture");

        let policy = test_policy_with_roots(
            vec![root.to_string_lossy().to_string()],
            vec![root.to_string_lossy().to_string()],
        );
        let db_root = unique_temp_dir("maisou-runtime-incremental-suspended-db");
        std::fs::create_dir_all(&db_root).expect("db dir");
        let store = ContentStore::open(&db_root.join("content.sqlite")).expect("store opens");
        let state = RuntimeIndexState::default();

        let summary = state.apply_fs_event(
            &policy,
            FsEvent::Created {
                path: target.to_string_lossy().to_string(),
                size_bytes: content.len() as u64,
                modified_at: 1,
            },
            Some(&store),
            false,
        );
        let name_hits = state.search(
            &SearchQuery {
                name: Some("suspended".to_string()),
                content: None,
                extension: Some("md".to_string()),
                text: None,
            },
            10,
        );
        let content_hits = store
            .search(&ContentSearchQuery {
                needle: "Suspended incremental".to_string(),
                extension: Some("md".to_string()),
                limit: 10,
                snippet_radius: 64,
            })
            .expect("store search succeeds");

        assert_eq!(summary.indexed_files, 1);
        assert_eq!(summary.content_files, 0);
        assert_eq!(name_hits.len(), 1);
        assert!(content_hits.is_empty());
        assert_eq!(state.summary().content_files, 0);
        std::fs::remove_dir_all(root).expect("cleanup");
        std::fs::remove_dir_all(db_root).expect("db cleanup");
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
            content_index: ContentIndexPolicy {
                enabled: true,
                batch_size: 64,
                snippet_radius: 500,
                default_limit: 50,
            },
        }
    }
}
