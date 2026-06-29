use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uchar, c_void};
use std::path::Path;
use std::ptr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDocument {
    pub physical_path: String,
    pub file_name: String,
    pub extension: String,
    pub modified_at: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSearchQuery {
    pub needle: String,
    pub extension: Option<String>,
    pub limit: usize,
    pub snippet_radius: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentSearchHit {
    pub physical_path: String,
    pub file_name: String,
    pub extension: String,
    pub modified_at: u64,
    pub snippet: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentStoreError {
    message: String,
}

impl ContentStoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ContentStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ContentStoreError {}

pub struct ContentStore {
    connection: *mut sqlite3,
}

impl ContentStore {
    pub fn open(path: &Path) -> Result<Self, ContentStoreError> {
        let path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| ContentStoreError::new("database path contains nul byte"))?;
        let mut connection = ptr::null_mut();
        let status = unsafe { sqlite3_open(path.as_ptr(), &mut connection) };
        if status != SQLITE_OK {
            let message = sqlite_error_message(connection);
            if !connection.is_null() {
                unsafe {
                    sqlite3_close(connection);
                }
            }
            return Err(ContentStoreError::new(message));
        }

        let store = Self { connection };
        store.create_schema()?;
        Ok(store)
    }

    pub fn upsert(&self, document: ContentDocument) -> Result<(), ContentStoreError> {
        self.delete_path(&document.physical_path)?;
        self.execute_with_bindings(
            "INSERT INTO content_documents(physical_path, file_name, extension, normalized_extension, modified_at, content) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                Binding::Text(&document.physical_path),
                Binding::Text(&document.file_name),
                Binding::Text(&document.extension),
                Binding::Text(&normalize_extension(&document.extension)),
                Binding::Integer(document.modified_at as i64),
                Binding::Text(&document.content),
            ],
        )
    }

    pub fn delete_path(&self, physical_path: &str) -> Result<(), ContentStoreError> {
        self.execute_with_bindings(
            "DELETE FROM content_documents WHERE physical_path = ?1",
            &[Binding::Text(physical_path)],
        )
    }

    pub fn search(
        &self,
        query: &ContentSearchQuery,
    ) -> Result<Vec<ContentSearchHit>, ContentStoreError> {
        if query.needle.trim().is_empty() || query.limit == 0 {
            return Ok(Vec::new());
        }

        let normalized_extension = query
            .extension
            .as_ref()
            .map(|value| normalize_extension(value));
        let limit = query.limit.min(i32::MAX as usize) as i64;
        let snippet_radius = query.snippet_radius.min(i32::MAX as usize) as i64;

        let (sql, bindings) = if let Some(extension) = normalized_extension.as_ref() {
            (
                "SELECT physical_path, file_name, extension, modified_at, content, bm25(content_documents) \
                 FROM content_documents WHERE content_documents MATCH ?1 AND normalized_extension = ?2 ORDER BY rank LIMIT ?4",
                vec![
                    Binding::Text(&query.needle),
                    Binding::Text(extension),
                    Binding::Integer(snippet_radius),
                    Binding::Integer(limit),
                ],
            )
        } else {
            (
                "SELECT physical_path, file_name, extension, modified_at, content, bm25(content_documents) \
                 FROM content_documents WHERE content_documents MATCH ?1 ORDER BY rank LIMIT ?3",
                vec![
                    Binding::Text(&query.needle),
                    Binding::Integer(snippet_radius),
                    Binding::Integer(limit),
                ],
            )
        };

        let statement = Statement::prepare(self.connection, sql)?;
        statement.bind_all(&bindings)?;

        let mut hits = Vec::new();
        loop {
            match statement.step()? {
                StepResult::Row => hits.push(ContentSearchHit {
                    physical_path: statement.column_text(0)?,
                    file_name: statement.column_text(1)?,
                    extension: statement.column_text(2)?,
                    modified_at: statement.column_i64(3).max(0) as u64,
                    snippet: build_snippet(
                        &statement.column_text(4)?,
                        &query.needle,
                        query.snippet_radius,
                    ),
                    score: statement.column_f64(5),
                }),
                StepResult::Done => break,
            }
        }
        Ok(hits)
    }

    fn create_schema(&self) -> Result<(), ContentStoreError> {
        self.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS content_documents USING fts5(
                physical_path UNINDEXED,
                file_name UNINDEXED,
                extension UNINDEXED,
                normalized_extension UNINDEXED,
                modified_at UNINDEXED,
                content
            );",
        )
    }

    fn execute_batch(&self, sql: &str) -> Result<(), ContentStoreError> {
        let sql = CString::new(sql).map_err(|_| ContentStoreError::new("sql contains nul byte"))?;
        let mut error = ptr::null_mut();
        let status = unsafe {
            sqlite3_exec(
                self.connection,
                sql.as_ptr(),
                None,
                ptr::null_mut(),
                &mut error,
            )
        };
        if status == SQLITE_OK {
            return Ok(());
        }

        let message = if error.is_null() {
            sqlite_error_message(self.connection)
        } else {
            let message = unsafe { CStr::from_ptr(error).to_string_lossy().to_string() };
            unsafe {
                sqlite3_free(error.cast());
            }
            message
        };
        Err(ContentStoreError::new(message))
    }

    fn execute_with_bindings(
        &self,
        sql: &str,
        bindings: &[Binding<'_>],
    ) -> Result<(), ContentStoreError> {
        let statement = Statement::prepare(self.connection, sql)?;
        statement.bind_all(bindings)?;
        match statement.step()? {
            StepResult::Done => Ok(()),
            StepResult::Row => Err(ContentStoreError::new(
                "statement unexpectedly returned row",
            )),
        }
    }
}

impl Drop for ContentStore {
    fn drop(&mut self) {
        if !self.connection.is_null() {
            unsafe {
                sqlite3_close(self.connection);
            }
            self.connection = ptr::null_mut();
        }
    }
}

enum Binding<'a> {
    Text(&'a str),
    Integer(i64),
}

enum StepResult {
    Row,
    Done,
}

struct Statement {
    connection: *mut sqlite3,
    statement: *mut sqlite3_stmt,
}

impl Statement {
    fn prepare(connection: *mut sqlite3, sql: &str) -> Result<Self, ContentStoreError> {
        let sql = CString::new(sql).map_err(|_| ContentStoreError::new("sql contains nul byte"))?;
        let mut statement = ptr::null_mut();
        let status = unsafe {
            sqlite3_prepare_v2(
                connection,
                sql.as_ptr(),
                -1,
                &mut statement,
                ptr::null_mut(),
            )
        };
        if status != SQLITE_OK {
            return Err(ContentStoreError::new(sqlite_error_message(connection)));
        }
        Ok(Self {
            connection,
            statement,
        })
    }

    fn bind_all(&self, bindings: &[Binding<'_>]) -> Result<(), ContentStoreError> {
        for (index, binding) in bindings.iter().enumerate() {
            let position = (index + 1).min(i32::MAX as usize) as c_int;
            let status = match binding {
                Binding::Text(value) => {
                    let value = CString::new(value.as_bytes())
                        .map_err(|_| ContentStoreError::new("binding contains nul byte"))?;
                    unsafe {
                        sqlite3_bind_text(
                            self.statement,
                            position,
                            value.as_ptr(),
                            -1,
                            SQLITE_TRANSIENT(),
                        )
                    }
                }
                Binding::Integer(value) => unsafe {
                    sqlite3_bind_int64(self.statement, position, *value)
                },
            };

            if status != SQLITE_OK {
                return Err(ContentStoreError::new(sqlite_error_message(
                    self.connection,
                )));
            }
        }
        Ok(())
    }

    fn step(&self) -> Result<StepResult, ContentStoreError> {
        match unsafe { sqlite3_step(self.statement) } {
            SQLITE_ROW => Ok(StepResult::Row),
            SQLITE_DONE => Ok(StepResult::Done),
            _ => Err(ContentStoreError::new(sqlite_error_message(
                self.connection,
            ))),
        }
    }

    fn column_text(&self, index: c_int) -> Result<String, ContentStoreError> {
        let value = unsafe { sqlite3_column_text(self.statement, index) };
        if value.is_null() {
            return Ok(String::new());
        }

        let text = unsafe { CStr::from_ptr(value.cast::<c_char>()) }
            .to_str()
            .map_err(|error| ContentStoreError::new(error.to_string()))?;
        Ok(text.to_string())
    }

    fn column_i64(&self, index: c_int) -> i64 {
        unsafe { sqlite3_column_int64(self.statement, index) }
    }

    fn column_f64(&self, index: c_int) -> f64 {
        unsafe { sqlite3_column_double(self.statement, index) }
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        if !self.statement.is_null() {
            unsafe {
                sqlite3_finalize(self.statement);
            }
            self.statement = ptr::null_mut();
        }
    }
}

fn normalize_extension(value: &str) -> String {
    value.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn build_snippet(content: &str, needle: &str, radius: usize) -> String {
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

fn sqlite_error_message(connection: *mut sqlite3) -> String {
    if connection.is_null() {
        return "sqlite connection is unavailable".to_string();
    }

    let message = unsafe { sqlite3_errmsg(connection) };
    if message.is_null() {
        return "sqlite error".to_string();
    }

    unsafe { CStr::from_ptr(message).to_string_lossy().to_string() }
}

#[allow(non_camel_case_types)]
enum sqlite3 {}

#[allow(non_camel_case_types)]
enum sqlite3_stmt {}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;

#[allow(non_snake_case)]
fn SQLITE_TRANSIENT() -> Option<unsafe extern "C" fn(*mut c_void)> {
    Some(unsafe { std::mem::transmute::<isize, unsafe extern "C" fn(*mut c_void)>(-1) })
}

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open(filename: *const c_char, pp_db: *mut *mut sqlite3) -> c_int;
    fn sqlite3_close(db: *mut sqlite3) -> c_int;
    fn sqlite3_errmsg(db: *mut sqlite3) -> *const c_char;
    fn sqlite3_exec(
        db: *mut sqlite3,
        sql: *const c_char,
        callback: Option<
            unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int,
        >,
        first_arg: *mut c_void,
        error_message: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_free(value: *mut c_void);
    fn sqlite3_prepare_v2(
        db: *mut sqlite3,
        sql: *const c_char,
        n_byte: c_int,
        statement: *mut *mut sqlite3_stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_finalize(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_step(statement: *mut sqlite3_stmt) -> c_int;
    fn sqlite3_bind_text(
        statement: *mut sqlite3_stmt,
        index: c_int,
        value: *const c_char,
        bytes: c_int,
        destructor: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_bind_int64(statement: *mut sqlite3_stmt, index: c_int, value: i64) -> c_int;
    fn sqlite3_column_text(statement: *mut sqlite3_stmt, index: c_int) -> *const c_uchar;
    fn sqlite3_column_int64(statement: *mut sqlite3_stmt, index: c_int) -> i64;
    fn sqlite3_column_double(statement: *mut sqlite3_stmt, index: c_int) -> f64;
}

#[cfg(test)]
mod tests {
    use super::*;

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

        store
            .delete_path("/work/notes/roadmap.md")
            .expect("delete succeeds");
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

    fn unique_temp_db(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{}-{}.sqlite",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        path
    }
}
