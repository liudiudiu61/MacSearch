# AnyTXT Full Text Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade MaiSou from filename-first local search plus preview-time content reading into a persistent, incremental, AnyTXT-style full-text content search pipeline.

**Architecture:** Keep the existing two-track search model: filename search remains the sub-20ms fast path, while content search is built from opt-in, resource-gated parsing into a local SQLite FTS5 store. Rust owns local scanning, parsing, FSEvents updates, SQLite writes, and Tauri commands; Vue consumes generated command types when the OpenAPI/Tauri type pipeline is available and keeps complex logic in composables.

**Tech Stack:** Tauri v2, Rust, SQLite FTS5, Vue 3 `<script setup>`, TypeScript, Pinia, Tailwind, Vitest, Cargo tests.

---

## Current State

The current tool is not only a mock UI, but its user-visible search surface is still mainly Everything-style:

- Implemented: fast filename/path search through `search_file_names_command`.
- Implemented: query parsing for `name:`, `content:`, `ext:`, and plain text.
- Implemented: preview-time real file reading through `read_preview_content_command`.
- Implemented: a lightweight in-memory content-search path in `src-tauri/src/runtime_index.rs`.
- Missing for true AnyTXT parity: persistent full-text index, durable FTS5 schema, incremental content update/delete, ranked snippets, PDF/Office parser boundaries, and explicit UI distinction between filename hits and content hits.

## Assumptions And Options

**Assumption 1:** The next practical phase should be "AnyTXT MVP", not OCR, cloud sync, or broad parser plugins.

**Assumption 2:** We should first support text-like formats already configured in `config/indexer_policy.json`, then add PDF/Office as parser interface extensions.

**Assumption 3:** Content indexing must stay opt-in and resource-gated. If `Suspended`, content parsing stops but filename indexing continues.

**Option A: Keep content search in memory.** Fast to implement, but loses index on restart and cannot scale.

**Option B: Persist content in SQLite FTS5.** Slightly more work, but matches Phase 1 architecture and AnyTXT expectations.

**Decision:** Use Option B. It is the correct foundation for local-first full-text search.

## Files And Responsibilities

- Modify: `config/indexer_policy.json`
  - Add content-index knobs such as snippet radius, result limits, batch size, parser type mapping, and FTS ranking options.
- Modify: `src-tauri/src/indexer.rs`
  - Keep scan gating and parse queue decisions here. Add parser routing metadata without hardcoded business rules.
- Create: `src-tauri/src/content_store.rs`
  - Own SQLite connection setup, FTS5 table creation, upsert/delete/query contracts, and snippet extraction.
- Modify: `src-tauri/src/runtime_index.rs`
  - Stop treating content search as only an in-memory fallback. Delegate persistent content search to `content_store`.
- Modify: `src-tauri/src/search.rs`
  - Extend `SearchHit` with hit source, score, optional snippet, and match range.
- Modify: `src-tauri/src/commands.rs`
  - Expose rebuild/search/status commands that return content-hit metadata without changing file action commands.
- Modify: `src/types/search.ts`
  - Add `hitSource`, `score`, and content snippet fields.
- Modify: `src/composables/useLocalSearch.ts`
  - Map Tauri search responses into UI models. Do not manually invent API contracts beyond the command response shape already exposed.
- Modify: `src/stores/searchStore.ts`
  - Preserve selected-result preview behavior and cache snippets by result id.
- Modify: `src/components/SearchShell.vue`
  - Show whether a hit came from filename or content and render content snippets without turning the UI into a heavy document viewer.
- Tests:
  - Add Rust tests near `content_store.rs`, `runtime_index.rs`, and `commands.rs`.
  - Add Vitest coverage for request mapping, result display metadata, and content-hit preview behavior.

---

### Task 1: Persistent FTS5 Store Contract

**Files:**
- Create: `src-tauri/src/content_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/content_store.rs`

- [x] **Step 1: Write failing Rust tests**

Add tests proving the local store can create the schema, upsert content, query content, return snippets, and delete removed paths.

```rust
#[test]
fn fts_store_upserts_queries_and_deletes_content() {
    let db_path = unique_temp_db("maisou-content-store");
    let store = ContentStore::open(&db_path).expect("store opens");

    store
        .upsert(ContentDocument {
            physical_path: "/work/notes/roadmap.md".to_string(),
            file_name: "roadmap.md".to_string(),
            extension: ".md".to_string(),
            modified_at: 1,
            content: "AnyTXT style search finds text inside files".to_string(),
        })
        .expect("document upserts");

    let hits = store
        .search(&ContentSearchQuery {
            needle: "text inside".to_string(),
            extension: Some("md".to_string()),
            limit: 10,
            snippet_radius: 32,
        })
        .expect("content search succeeds");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].file_name, "roadmap.md");
    assert!(hits[0].snippet.contains("text inside"));

    store.delete_path("/work/notes/roadmap.md").expect("delete succeeds");
    let hits_after_delete = store
        .search(&ContentSearchQuery {
            needle: "text inside".to_string(),
            extension: Some("md".to_string()),
            limit: 10,
            snippet_radius: 32,
        })
        .expect("content search succeeds");
    assert!(hits_after_delete.is_empty());
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test content_store -- --nocapture`

Expected: FAIL because `content_store` does not exist.

- [x] **Step 3: Implement minimal store**

Create `ContentStore`, `ContentDocument`, `ContentSearchQuery`, and `ContentSearchHit`. Use SQLite FTS5, with schema creation inside `ContentStore::open`.

Keep all business rules out of this file. This file should only know persistence and query mechanics.

- [x] **Step 4: Run focused tests**

Run: `cargo test content_store -- --nocapture`

Expected: PASS.

### Task 2: Config-Driven Content Index Policy

**Files:**
- Modify: `config/indexer_policy.json`
- Modify: `src-tauri/src/indexer.rs`
- Test: `src-tauri/src/indexer.rs`

- [x] **Step 1: Write failing policy parse test**

Add a test proving the new policy fields load from JSON and no snippet/ranking/batch value is hardcoded.

```rust
#[test]
fn parses_content_index_policy_from_json() {
    let policy = IndexerPolicy::from_json(
        r#"{
          "physical_id": "local.indexer.policy",
          "professional_description": "test policy",
          "scan_roots": ["/work"],
          "opt_in_watch_roots": ["/work"],
          "exclude_path_fragments": ["/.git/"],
          "max_parse_size_bytes": 52428800,
          "text_extensions": [".md", ".txt"],
          "encrypted_extensions": [".enc"],
          "parse_error_code": "Parse_Error",
          "content_index": {
            "enabled": true,
            "batch_size": 64,
            "snippet_radius": 500,
            "default_limit": 50
          }
        }"#,
    )
    .expect("policy parses");

    assert!(policy.content_index.enabled);
    assert_eq!(policy.content_index.batch_size, 64);
    assert_eq!(policy.content_index.snippet_radius, 500);
    assert_eq!(policy.content_index.default_limit, 50);
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test parses_content_index_policy_from_json -- --nocapture`

Expected: FAIL because `content_index` is not modeled yet.

- [x] **Step 3: Implement policy fields**

Add `ContentIndexPolicy` to `src-tauri/src/indexer.rs` and parse it from `config/indexer_policy.json`.

- [x] **Step 4: Update JSON config**

Add:

```json
"content_index": {
  "enabled": true,
  "batch_size": 64,
  "snippet_radius": 500,
  "default_limit": 50
}
```

- [x] **Step 5: Run Rust tests**

Run: `cargo test`

Expected: PASS.

### Task 3: Build Content Index During Rebuild

**Files:**
- Modify: `src-tauri/src/runtime_index.rs`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/runtime_index.rs`

- [x] **Step 1: Write failing rebuild test**

Add a test proving `LocalSearchIndex::rebuild_from_policy` indexes eligible text content into the persistent store and reports indexed content count.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test rebuilds_real_file_index_and_finds_text_content -- --nocapture`

Expected: FAIL until persistent store wiring exists.

- [x] **Step 3: Implement minimal rebuild wiring**

When a file has `ContentAction::QueueParse`, read and parse it through existing `parse_text_content`; if valid, upsert it into `ContentStore`. If invalid, increment `parse_errors` and keep filename indexing intact.

- [x] **Step 4: Preserve resource gate behavior**

Do not parse content if the control center says content indexing is disallowed. Filename records still enter the runtime index.

- [x] **Step 5: Run focused tests**

Run: `cargo test runtime_index -- --nocapture`

Expected: PASS.

### Task 4: Search Result Contract With Hit Source And Snippet

**Files:**
- Modify: `src-tauri/src/search.rs`
- Modify: `src-tauri/src/runtime_index.rs`
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/search.rs`
- Test: `src-tauri/src/commands.rs`

- [x] **Step 1: Write failing command contract test**

Add a command-layer test proving `content:` returns a content hit with `hit_source = "content"` and a snippet, while `name:` returns `hit_source = "filename"`.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test search_file_names_command -- --nocapture`

Expected: FAIL because the response has no hit source or snippet.

- [x] **Step 3: Extend Rust response types**

Add fields to `SearchHit` and `SearchFileNameHit`:

```rust
pub hit_source: String,
pub score: f64,
pub snippet: Option<String>,
```

Use configured limits and snippets. Do not embed source labels in UI code only.

- [x] **Step 4: Merge filename and content results**

For plain text query:
- filename hits appear first when file name matches;
- content hits append asynchronously or in the same command response for the MVP;
- duplicate physical paths collapse into one hit, preserving the strongest source metadata.

- [x] **Step 5: Run Rust tests**

Run: `cargo test`

Expected: PASS.

### Task 5: Frontend Content Hit Rendering

**Files:**
- Modify: `src/types/search.ts`
- Modify: `src/composables/useLocalSearch.ts`
- Modify: `src/stores/searchStore.ts`
- Modify: `src/components/SearchShell.vue`
- Test: `tests/localSearch.test.ts`
- Test: `tests/previewContent.test.ts`

- [x] **Step 1: Write failing Vitest coverage**

Add tests proving Tauri hits with `hit_source = "content"` map to `SearchResult.hitSource = "content"` and display the snippet as the result excerpt.

- [x] **Step 2: Run test to verify it fails**

Run: `npm test -- localSearch`

Expected: FAIL because the frontend model has no hit source.

- [x] **Step 3: Extend frontend model**

Add to `SearchResult`:

```ts
hitSource: 'filename' | 'content';
score: number;
snippet: string | null;
```

- [x] **Step 4: Map command response**

In `useLocalSearch.ts`, map `hit_source`, `score`, and `snippet` from Tauri response. Use snippet as excerpt for content hits and path as excerpt for filename hits.

- [x] **Step 5: Render concise content metadata**

In `SearchShell.vue`, show a small source indicator and snippet text. Keep the existing preview panel as the detailed content surface.

- [x] **Step 6: Run frontend tests and build**

Run: `npm test`

Run: `npm run build`

Expected: PASS.

### Task 6: Incremental Updates From FSEvents

**Files:**
- Modify: `src-tauri/src/indexer.rs`
- Modify: `src-tauri/src/runtime_index.rs`
- Modify: `src-tauri/src/content_store.rs`
- Test: `src-tauri/src/runtime_index.rs`

- [x] **Step 1: Write failing incremental update tests**

Cover:
- created text file gets indexed;
- modified text file updates content;
- removed file deletes content index row;
- suspended content indexing does not parse content but keeps filename record.

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test incremental -- --nocapture`

Expected: FAIL until update handling is wired.

- [x] **Step 3: Implement update application**

Add a focused method such as `RuntimeIndexState::apply_fs_event(policy, event, allow_content_indexing)`. Reuse `enqueue_fs_event_with_content_gate`.

- [x] **Step 4: Run focused and full Rust tests**

Run: `cargo test incremental -- --nocapture`

Run: `cargo test`

Expected: PASS.

### Task 7: Parser Interface Boundary

**Files:**
- Modify: `src-tauri/src/indexer.rs`
- Create: `src-tauri/src/parsers.rs`
- Test: `src-tauri/src/parsers.rs`

- [x] **Step 1: Write failing parser tests**

Add tests for text-like parser success, malformed UTF-8 parse error, and unsupported extension falling back to filename-only.

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test parsers -- --nocapture`

Expected: FAIL because parser interface does not exist.

- [x] **Step 3: Implement parser trait**

Create:

```rust
pub trait ContentParser {
    fn parse(&self, bytes: &[u8]) -> Result<String, ParseFailure>;
}
```

Start with `Utf8TextParser`. Leave PDF/Office for a later task, but make the extension-to-parser routing config-driven.

- [x] **Step 4: Run parser and runtime tests**

Run: `cargo test parsers -- --nocapture`

Run: `cargo test runtime_index -- --nocapture`

Expected: PASS.

### Task 8: Status And Progress For Content Indexing

**Files:**
- Modify: `src-tauri/src/runtime_index.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/composables/useIndexerStatus.ts`
- Modify: `src/components/SearchShell.vue`
- Test: `tests/indexerStatus.test.ts`

- [x] **Step 1: Write failing status tests**

Prove status includes `indexedFiles`, `contentFiles`, `parseErrors`, and a new `contentQueueDepth`.

- [x] **Step 2: Run tests to verify they fail**

Run: `npm test -- indexerStatus`

Expected: FAIL because the UI status model lacks content queue depth.

- [x] **Step 3: Extend status response**

Add `content_queue_depth` to Rust response and map to `contentQueueDepth` in TS.

- [x] **Step 4: Render compact progress**

Show content index progress only in the status area, not as a modal. Keep search usable while indexing.

- [x] **Step 5: Run frontend and Rust tests**

Run: `npm test`

Run: `cargo test`

Expected: PASS.

---

## Validation Standard

Before marking the AnyTXT phase complete, run:

```bash
cargo test
npm test
npm run build
cargo fmt --check
cargo build
```

Expected:
- filename search still meets the existing 20ms benchmark;
- content search returns first-screen results within the configured local limit;
- oversized, encrypted, malformed, and unsupported files do not crash indexing;
- suspended mode blocks content parsing but preserves filename updates;
- all status codes and parse behavior remain loaded from config.

## Suggested Phase Boundary

Call this next phase:

**Phase 5: AnyTXT 全文内容索引与混合检索**

Recommended task order:

1. Persistent FTS5 store.
2. Config-driven content index policy.
3. Rebuild-time content indexing.
4. Search response with hit source and snippet.
5. Frontend content-hit rendering.
6. Incremental FSEvents updates.
7. Parser interface boundary.
8. Content indexing status and progress.

This sequence gives a usable AnyTXT MVP by Task 5, then makes it durable and production-ready through Tasks 6-8.
