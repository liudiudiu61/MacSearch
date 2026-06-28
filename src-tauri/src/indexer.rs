use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexerPolicy {
    pub physical_id: String,
    pub professional_description: String,
    pub scan_roots: Vec<String>,
    pub opt_in_watch_roots: Vec<String>,
    pub exclude_path_fragments: Vec<String>,
    pub max_parse_size_bytes: u64,
    pub text_extensions: Vec<String>,
    pub encrypted_extensions: Vec<String>,
    pub parse_error_code: String,
}

impl IndexerPolicy {
    pub fn from_json(payload: &str) -> Result<Self, IndexerPolicyParseError> {
        Ok(Self {
            physical_id: extract_json_string(payload, "physical_id")?,
            professional_description: extract_json_string(payload, "professional_description")?,
            scan_roots: extract_json_string_array(payload, "scan_roots")?,
            opt_in_watch_roots: extract_json_string_array(payload, "opt_in_watch_roots")?,
            exclude_path_fragments: extract_json_string_array(payload, "exclude_path_fragments")?,
            max_parse_size_bytes: extract_json_u64(payload, "max_parse_size_bytes")?,
            text_extensions: extract_json_string_array(payload, "text_extensions")?
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect(),
            encrypted_extensions: extract_json_string_array(payload, "encrypted_extensions")?
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect(),
            parse_error_code: extract_json_string(payload, "parse_error_code")?,
        })
    }

    #[cfg(test)]
    fn default_for_tests() -> Self {
        Self {
            physical_id: "local.indexer.policy".to_string(),
            professional_description: "Indexer policy for tests".to_string(),
            scan_roots: vec!["/opt/in".to_string()],
            opt_in_watch_roots: vec!["/opt/in/notes".to_string()],
            exclude_path_fragments: vec!["/.git/".to_string(), "/node_modules/".to_string()],
            max_parse_size_bytes: 52_428_800,
            text_extensions: vec![".txt".to_string(), ".md".to_string(), ".json".to_string()],
            encrypted_extensions: vec![".gpg".to_string(), ".enc".to_string()],
            parse_error_code: "Parse_Error".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexerPolicyParseError {
    pub field: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSystemEntry {
    pub physical_path: String,
    pub is_directory: bool,
    pub size_bytes: u64,
    pub modified_at: u64,
}

impl FileSystemEntry {
    pub fn file(path: &str, size_bytes: u64, modified_at: u64) -> Self {
        Self {
            physical_path: path.to_string(),
            is_directory: false,
            size_bytes,
            modified_at,
        }
    }

    pub fn directory(path: &str) -> Self {
        Self {
            physical_path: path.to_string(),
            is_directory: true,
            size_bytes: 0,
            modified_at: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentAction {
    QueueParse,
    FilenameOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Parsed,
    ParseError { code: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndexRecord {
    pub physical_path: String,
    pub file_name: String,
    pub extension: String,
    pub modified_at: u64,
    pub size_bytes: u64,
    pub content_action: ContentAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Created {
        path: String,
        size_bytes: u64,
        modified_at: u64,
    },
    Modified {
        path: String,
        size_bytes: u64,
        modified_at: u64,
    },
    Removed {
        path: String,
    },
}

#[derive(Debug, Default)]
pub struct ParseQueue {
    records: VecDeque<FileIndexRecord>,
}

impl ParseQueue {
    pub fn new() -> Self {
        Self {
            records: VecDeque::new(),
        }
    }

    pub fn push(&mut self, record: FileIndexRecord) {
        self.records.push_back(record);
    }

    pub fn pop(&mut self) -> Option<FileIndexRecord> {
        self.records.pop_front()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }
}

pub fn first_layer_scan(
    policy: &IndexerPolicy,
    entries: Vec<FileSystemEntry>,
) -> Vec<FileIndexRecord> {
    entries
        .into_iter()
        .filter(|entry| !entry.is_directory)
        .filter(|entry| is_under_any_root(&entry.physical_path, &policy.scan_roots))
        .filter(|entry| !is_excluded(&entry.physical_path, &policy.exclude_path_fragments))
        .map(|entry| record_from_entry(policy, entry))
        .collect()
}

pub struct StdFileSystemScanner;

impl StdFileSystemScanner {
    pub fn scan(policy: &IndexerPolicy) -> Vec<FileSystemEntry> {
        let mut entries = Vec::new();
        for root in &policy.scan_roots {
            let expanded_root = expand_home(root);
            collect_entries(&expanded_root, &mut entries);
        }
        entries
    }
}

pub fn enqueue_fs_event(policy: &IndexerPolicy, queue: &mut ParseQueue, event: FsEvent) {
    match event {
        FsEvent::Created {
            path,
            size_bytes,
            modified_at,
        }
        | FsEvent::Modified {
            path,
            size_bytes,
            modified_at,
        } => {
            if !is_under_any_root(&path, &policy.opt_in_watch_roots)
                || is_excluded(&path, &policy.exclude_path_fragments)
            {
                return;
            }

            let entry = FileSystemEntry::file(&path, size_bytes, modified_at);
            let record = record_from_entry(policy, entry);
            if record.content_action == ContentAction::QueueParse {
                queue.push(record);
            }
        }
        FsEvent::Removed { .. } => {}
    }
}

pub fn enqueue_fs_event_with_content_gate(
    policy: &IndexerPolicy,
    queue: &mut ParseQueue,
    event: FsEvent,
    allow_content_indexing: bool,
) -> Option<FileIndexRecord> {
    match event {
        FsEvent::Created {
            path,
            size_bytes,
            modified_at,
        }
        | FsEvent::Modified {
            path,
            size_bytes,
            modified_at,
        } => {
            if !is_under_any_root(&path, &policy.opt_in_watch_roots)
                || is_excluded(&path, &policy.exclude_path_fragments)
            {
                return None;
            }

            let entry = FileSystemEntry::file(&path, size_bytes, modified_at);
            let record = record_from_entry(policy, entry);
            if allow_content_indexing && record.content_action == ContentAction::QueueParse {
                queue.push(record.clone());
            }
            Some(record)
        }
        FsEvent::Removed { .. } => None,
    }
}

pub fn parse_text_content(policy: &IndexerPolicy, bytes: &[u8]) -> ParseStatus {
    match std::str::from_utf8(bytes) {
        Ok(_) => ParseStatus::Parsed,
        Err(_) => ParseStatus::ParseError {
            code: policy.parse_error_code.clone(),
        },
    }
}

pub trait FileEventWatcher {
    fn drain_events(&mut self) -> Vec<FsEvent>;
}

pub struct MacOsFseventsWatcher {
    roots: Vec<String>,
    pending_events: Arc<Mutex<Vec<FsEvent>>>,
    #[cfg(target_os = "macos")]
    _stream: Option<macos_fsevents::FseventStreamHandle>,
}

impl MacOsFseventsWatcher {
    pub fn new(policy: &IndexerPolicy) -> Self {
        let pending_events = Arc::new(Mutex::new(Vec::new()));
        Self {
            roots: policy.opt_in_watch_roots.clone(),
            pending_events,
            #[cfg(target_os = "macos")]
            _stream: None,
        }
    }

    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    pub fn start(&mut self) -> bool {
        self.start_platform_stream()
    }

    #[cfg(target_os = "macos")]
    fn start_platform_stream(&mut self) -> bool {
        if self._stream.is_some() {
            return true;
        }
        self._stream = macos_fsevents::start_stream(&self.roots, Arc::clone(&self.pending_events));
        self._stream.is_some()
    }

    #[cfg(not(target_os = "macos"))]
    fn start_platform_stream(&mut self) -> bool {
        false
    }
}

impl FileEventWatcher for MacOsFseventsWatcher {
    fn drain_events(&mut self) -> Vec<FsEvent> {
        let Ok(mut pending_events) = self.pending_events.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut *pending_events)
    }
}

pub fn run_watch_once<W: FileEventWatcher>(
    policy: &IndexerPolicy,
    watcher: &mut W,
    queue: &mut ParseQueue,
) {
    for event in watcher.drain_events() {
        enqueue_fs_event(policy, queue, event);
    }
}

fn record_from_entry(policy: &IndexerPolicy, entry: FileSystemEntry) -> FileIndexRecord {
    let extension = extension_of(&entry.physical_path);
    let content_action = if entry.size_bytes > policy.max_parse_size_bytes
        || policy.encrypted_extensions.contains(&extension)
    {
        ContentAction::FilenameOnly
    } else {
        ContentAction::QueueParse
    };

    FileIndexRecord {
        file_name: file_name_of(&entry.physical_path),
        extension,
        physical_path: entry.physical_path,
        modified_at: entry.modified_at,
        size_bytes: entry.size_bytes,
        content_action,
    }
}

fn is_under_any_root(path: &str, roots: &[String]) -> bool {
    roots
        .iter()
        .map(|root| expand_home(root))
        .any(|root| path.starts_with(&root))
}

fn is_excluded(path: &str, fragments: &[String]) -> bool {
    fragments.iter().any(|fragment| path.contains(fragment))
}

fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn extension_of(path: &str) -> String {
    let file_name = file_name_of(path);
    match file_name.rsplit_once('.') {
        Some((_, extension)) => format!(".{}", extension.to_ascii_lowercase()),
        None => String::new(),
    }
}

fn collect_entries(root: &str, entries: &mut Vec<FileSystemEntry>) {
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };

    for entry_result in read_dir {
        let Ok(entry) = entry_result else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let path = entry.path().to_string_lossy().to_string();
        if metadata.is_dir() {
            entries.push(FileSystemEntry::directory(&path));
            collect_entries(&path, entries);
        } else if metadata.is_file() {
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            entries.push(FileSystemEntry::file(&path, metadata.len(), modified_at));
        }
    }
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }

    path.to_string()
}

fn extract_json_string(
    payload: &str,
    field: &'static str,
) -> Result<String, IndexerPolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload
        .find(&marker)
        .ok_or(IndexerPolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let colon_index = after_field
        .find(':')
        .ok_or(IndexerPolicyParseError { field })?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value_start = after_colon
        .strip_prefix('"')
        .ok_or(IndexerPolicyParseError { field })?;
    let value_end = value_start
        .find('"')
        .ok_or(IndexerPolicyParseError { field })?;
    Ok(value_start[..value_end].to_string())
}

fn extract_json_u64(payload: &str, field: &'static str) -> Result<u64, IndexerPolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload
        .find(&marker)
        .ok_or(IndexerPolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let colon_index = after_field
        .find(':')
        .ok_or(IndexerPolicyParseError { field })?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value_text: String = after_colon
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    value_text
        .parse::<u64>()
        .map_err(|_| IndexerPolicyParseError { field })
}

fn extract_json_string_array(
    payload: &str,
    field: &'static str,
) -> Result<Vec<String>, IndexerPolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload
        .find(&marker)
        .ok_or(IndexerPolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let array_start = after_field
        .find('[')
        .ok_or(IndexerPolicyParseError { field })?;
    let after_array_start = &after_field[array_start + 1..];
    let array_end = after_array_start
        .find(']')
        .ok_or(IndexerPolicyParseError { field })?;
    let array_body = &after_array_start[..array_end];
    let mut values = Vec::new();
    for raw_value in array_body.split(',') {
        let trimmed = raw_value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed
            .strip_prefix('"')
            .and_then(|text| text.strip_suffix('"'))
            .ok_or(IndexerPolicyParseError { field })?;
        values.push(value.to_string());
    }
    Ok(values)
}

#[cfg(target_os = "macos")]
mod macos_fsevents {
    use super::FsEvent;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::ptr;
    use std::sync::{Arc, Mutex};

    type CFAllocatorRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFIndex = isize;
    type CFRunLoopRef = *const c_void;
    type CFStringRef = *const c_void;
    type FSEventStreamEventFlags = u32;
    type FSEventStreamEventId = u64;
    type FSEventStreamRef = *mut c_void;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_FSEVENT_STREAM_EVENT_ID_SINCE_NOW: FSEventStreamEventId = u64::MAX;
    const K_FSEVENT_STREAM_CREATE_FLAG_NO_DEFER: u32 = 0x0000_0002;
    const K_FSEVENT_STREAM_CREATE_FLAG_FILE_EVENTS: u32 = 0x0000_0010;

    #[repr(C)]
    struct FSEventStreamContext {
        version: CFIndex,
        info: *mut c_void,
        retain: Option<unsafe extern "C" fn(*const c_void) -> *const c_void>,
        release: Option<unsafe extern "C" fn(*const c_void)>,
        copy_description: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    }

    type FSEventStreamCallback = unsafe extern "C" fn(
        stream_ref: FSEventStreamRef,
        client_call_back_info: *mut c_void,
        num_events: usize,
        event_paths: *mut c_void,
        event_flags: *const FSEventStreamEventFlags,
        event_ids: *const FSEventStreamEventId,
    );

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayCreate(
            allocator: CFAllocatorRef,
            values: *const *const c_void,
            num_values: CFIndex,
            callbacks: *const c_void,
        ) -> CFArrayRef;
        fn CFRelease(cf: *const c_void);
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
    }

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn FSEventStreamCreate(
            allocator: CFAllocatorRef,
            callback: FSEventStreamCallback,
            context: *mut FSEventStreamContext,
            paths_to_watch: CFArrayRef,
            since_when: FSEventStreamEventId,
            latency: f64,
            flags: u32,
        ) -> FSEventStreamRef;
        fn FSEventStreamInvalidate(stream_ref: FSEventStreamRef);
        fn FSEventStreamRelease(stream_ref: FSEventStreamRef);
        fn FSEventStreamScheduleWithRunLoop(
            stream_ref: FSEventStreamRef,
            run_loop: CFRunLoopRef,
            run_loop_mode: CFStringRef,
        );
        fn FSEventStreamStart(stream_ref: FSEventStreamRef) -> bool;
        fn FSEventStreamStop(stream_ref: FSEventStreamRef);
    }

    pub struct FseventStreamHandle {
        stream: FSEventStreamRef,
        _context: Box<WatcherContext>,
    }

    impl Drop for FseventStreamHandle {
        fn drop(&mut self) {
            unsafe {
                FSEventStreamStop(self.stream);
                FSEventStreamInvalidate(self.stream);
                FSEventStreamRelease(self.stream);
            }
        }
    }

    struct WatcherContext {
        pending_events: Arc<Mutex<Vec<FsEvent>>>,
    }

    pub fn start_stream(
        roots: &[String],
        pending_events: Arc<Mutex<Vec<FsEvent>>>,
    ) -> Option<FseventStreamHandle> {
        if roots.is_empty() {
            return None;
        }

        let c_roots: Vec<CString> = roots
            .iter()
            .filter_map(|root| CString::new(root.as_str()).ok())
            .collect();
        if c_roots.is_empty() {
            return None;
        }

        let cf_roots: Vec<CFStringRef> = c_roots
            .iter()
            .filter_map(|root| unsafe {
                let value = CFStringCreateWithCString(
                    ptr::null(),
                    root.as_ptr(),
                    K_CF_STRING_ENCODING_UTF8,
                );
                (!value.is_null()).then_some(value)
            })
            .collect();
        if cf_roots.is_empty() {
            return None;
        }

        let values: Vec<*const c_void> = cf_roots.iter().map(|value| *value).collect();
        let paths = unsafe {
            CFArrayCreate(
                ptr::null(),
                values.as_ptr(),
                values.len() as CFIndex,
                ptr::null(),
            )
        };
        for root in &cf_roots {
            unsafe { CFRelease(*root) };
        }
        if paths.is_null() {
            return None;
        }

        let mut context = Box::new(WatcherContext { pending_events });
        let mut stream_context = FSEventStreamContext {
            version: 0,
            info: (&mut *context) as *mut WatcherContext as *mut c_void,
            retain: None,
            release: None,
            copy_description: None,
        };
        let stream = unsafe {
            FSEventStreamCreate(
                ptr::null(),
                fsevents_callback,
                &mut stream_context,
                paths,
                K_FSEVENT_STREAM_EVENT_ID_SINCE_NOW,
                0.25,
                K_FSEVENT_STREAM_CREATE_FLAG_NO_DEFER | K_FSEVENT_STREAM_CREATE_FLAG_FILE_EVENTS,
            )
        };
        unsafe { CFRelease(paths) };
        if stream.is_null() {
            return None;
        }

        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            let mode = CFStringCreateWithCString(
                ptr::null(),
                b"kCFRunLoopDefaultMode\0".as_ptr() as *const c_char,
                K_CF_STRING_ENCODING_UTF8,
            );
            FSEventStreamScheduleWithRunLoop(stream, run_loop, mode);
            CFRelease(mode);
            if !FSEventStreamStart(stream) {
                FSEventStreamInvalidate(stream);
                FSEventStreamRelease(stream);
                return None;
            }
        }

        Some(FseventStreamHandle {
            stream,
            _context: context,
        })
    }

    unsafe extern "C" fn fsevents_callback(
        _stream_ref: FSEventStreamRef,
        client_call_back_info: *mut c_void,
        num_events: usize,
        event_paths: *mut c_void,
        _event_flags: *const FSEventStreamEventFlags,
        _event_ids: *const FSEventStreamEventId,
    ) {
        if client_call_back_info.is_null() || event_paths.is_null() {
            return;
        }
        let context = &*(client_call_back_info as *const WatcherContext);
        let paths = event_paths as *const *const c_char;
        let Ok(mut pending_events) = context.pending_events.lock() else {
            return;
        };

        for index in 0..num_events {
            let path_ptr = *paths.add(index);
            if path_ptr.is_null() {
                continue;
            }
            let path = CStr::from_ptr(path_ptr).to_string_lossy().to_string();
            let metadata = std::fs::metadata(&path).ok();
            pending_events.push(FsEvent::Modified {
                path,
                size_bytes: metadata.as_ref().map(|value| value.len()).unwrap_or(0),
                modified_at: metadata
                    .and_then(|value| value.modified().ok())
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_indexer_policy_from_json_configuration() {
        let policy = IndexerPolicy::from_json(
            r#"{
              "physical_id": "local.indexer.policy",
              "professional_description": "Indexer traversal policy",
              "scan_roots": ["/Users/example/Documents"],
              "opt_in_watch_roots": ["/Users/example/Documents/Notes"],
              "exclude_path_fragments": ["/node_modules/", "/.git/"],
              "max_parse_size_bytes": 52428800,
              "text_extensions": [".txt", ".md", ".json"],
              "encrypted_extensions": [".gpg", ".enc"],
              "parse_error_code": "Parse_Error"
            }"#,
        )
        .expect("policy should parse");

        assert_eq!(policy.scan_roots, vec!["/Users/example/Documents"]);
        assert_eq!(policy.max_parse_size_bytes, 52_428_800);
        assert_eq!(policy.parse_error_code, "Parse_Error");
        assert!(policy
            .exclude_path_fragments
            .contains(&"/.git/".to_string()));
    }

    #[test]
    fn first_layer_scan_keeps_file_name_metadata_without_content_parse() {
        let policy = IndexerPolicy::default_for_tests();
        let entries = vec![
            FileSystemEntry::file("/opt/in/notes/todo.md", 1024, 100),
            FileSystemEntry::directory("/opt/in/notes/archive"),
        ];

        let records = first_layer_scan(&policy, entries);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].file_name, "todo.md");
        assert_eq!(records[0].extension, ".md");
        assert_eq!(records[0].content_action, ContentAction::QueueParse);
    }

    #[test]
    fn first_layer_scan_excludes_blacklisted_paths_and_large_or_encrypted_content() {
        let policy = IndexerPolicy::default_for_tests();
        let entries = vec![
            FileSystemEntry::file("/opt/in/project/.git/config", 12, 100),
            FileSystemEntry::file("/opt/in/huge.pdf", 99_000_000, 100),
            FileSystemEntry::file("/opt/in/secret.gpg", 128, 100),
        ];

        let records = first_layer_scan(&policy, entries);

        assert_eq!(records.len(), 2);
        assert!(records
            .iter()
            .all(|record| record.physical_path != "/opt/in/project/.git/config"));
        assert_eq!(records[0].content_action, ContentAction::FilenameOnly);
        assert_eq!(records[1].content_action, ContentAction::FilenameOnly);
    }

    #[test]
    fn suspended_resource_gate_keeps_filename_update_out_of_parse_queue_within_budget() {
        let policy = IndexerPolicy::default_for_tests();
        let mut queue = ParseQueue::new();
        let started_at = std::time::Instant::now();

        let record = enqueue_fs_event_with_content_gate(
            &policy,
            &mut queue,
            FsEvent::Modified {
                path: "/opt/in/notes/large-batch.md".to_string(),
                size_bytes: 2048,
                modified_at: 200,
            },
            false,
        )
        .expect("filename update should still be represented");

        assert_eq!(record.file_name, "large-batch.md");
        assert_eq!(record.content_action, ContentAction::QueueParse);
        assert_eq!(queue.len(), 0);
        assert!(
            started_at.elapsed() <= std::time::Duration::from_secs(1),
            "resource gate took {:?}",
            started_at.elapsed()
        );
    }

    #[test]
    fn malformed_text_content_is_marked_with_configured_parse_error_code() {
        let policy = IndexerPolicy::default_for_tests();

        let status = parse_text_content(&policy, &[0xff, 0xfe, 0xfd]);

        assert_eq!(
            status,
            ParseStatus::ParseError {
                code: "Parse_Error".to_string()
            }
        );
    }

    #[test]
    fn fsevents_adapter_pushes_opt_in_changes_to_parse_queue() {
        let policy = IndexerPolicy::default_for_tests();
        let mut queue = ParseQueue::new();

        enqueue_fs_event(
            &policy,
            &mut queue,
            FsEvent::Modified {
                path: "/opt/in/notes/todo.md".to_string(),
                size_bytes: 2048,
                modified_at: 200,
            },
        );
        enqueue_fs_event(
            &policy,
            &mut queue,
            FsEvent::Modified {
                path: "/outside/todo.md".to_string(),
                size_bytes: 2048,
                modified_at: 200,
            },
        );

        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop().unwrap().physical_path, "/opt/in/notes/todo.md");
    }

    #[test]
    fn std_file_system_scanner_feeds_first_layer_funnel() {
        let root = unique_temp_dir("maisou-indexer-scan");
        std::fs::create_dir_all(root.join("notes")).expect("notes dir");
        std::fs::create_dir_all(root.join("project/.git")).expect("git dir");
        std::fs::write(root.join("notes/todo.md"), "hello").expect("small file");
        std::fs::write(root.join("project/.git/config"), "ignore").expect("ignored file");

        let policy = IndexerPolicy {
            scan_roots: vec![root.to_string_lossy().to_string()],
            opt_in_watch_roots: vec![root.join("notes").to_string_lossy().to_string()],
            ..IndexerPolicy::default_for_tests()
        };

        let entries = StdFileSystemScanner::scan(&policy);
        let records = first_layer_scan(&policy, entries);

        assert_eq!(records.len(), 1);
        assert!(records[0].physical_path.ends_with("notes/todo.md"));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn std_file_system_scanner_expands_configured_home_roots() {
        let home = std::env::var("HOME").expect("HOME should be set for scanner tests");
        let policy = IndexerPolicy {
            physical_id: "local.indexer.policy".to_string(),
            professional_description: "Indexer policy for home expansion tests".to_string(),
            scan_roots: vec!["~/definitely-not-a-maisou-test-directory".to_string()],
            opt_in_watch_roots: Vec::new(),
            exclude_path_fragments: Vec::new(),
            max_parse_size_bytes: 10,
            text_extensions: vec![".txt".to_string()],
            encrypted_extensions: Vec::new(),
            parse_error_code: "Parse_Error".to_string(),
        };

        assert_eq!(
            expand_home(&policy.scan_roots[0]),
            format!("{home}/definitely-not-a-maisou-test-directory")
        );
    }

    #[test]
    fn first_layer_scan_matches_entries_against_expanded_home_roots() {
        let home = std::env::var("HOME").expect("HOME should be set for scanner tests");
        let policy = IndexerPolicy {
            scan_roots: vec!["~/Documents".to_string()],
            opt_in_watch_roots: Vec::new(),
            ..IndexerPolicy::default_for_tests()
        };
        let entries = vec![FileSystemEntry::file(
            &format!("{home}/Documents/maisou-home-root-test.md"),
            16,
            100,
        )];

        let records = first_layer_scan(&policy, entries);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].file_name, "maisou-home-root-test.md");
    }

    #[test]
    fn watcher_boundary_drains_events_into_parse_queue() {
        let policy = IndexerPolicy::default_for_tests();
        let mut watcher = FakeWatcher {
            events: vec![FsEvent::Modified {
                path: "/opt/in/notes/todo.md".to_string(),
                size_bytes: 1024,
                modified_at: 300,
            }],
        };
        let mut queue = ParseQueue::new();

        run_watch_once(&policy, &mut watcher, &mut queue);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue.pop().unwrap().modified_at, 300);
    }

    #[test]
    fn macos_fsevents_watcher_uses_opt_in_roots_from_policy() {
        let policy = IndexerPolicy::default_for_tests();

        let watcher = MacOsFseventsWatcher::new(&policy);

        assert_eq!(watcher.roots(), &["/opt/in/notes".to_string()]);
    }

    struct FakeWatcher {
        events: Vec<FsEvent>,
    }

    impl FileEventWatcher for FakeWatcher {
        fn drain_events(&mut self) -> Vec<FsEvent> {
            std::mem::take(&mut self.events)
        }
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
}
