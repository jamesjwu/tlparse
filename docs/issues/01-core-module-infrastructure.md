# Sub-Issue #1: Core Module Infrastructure

## Summary
Implement the core infrastructure for the module system including traits, context, and registry.

## Tasks

### 1.1 Create `src/modules/mod.rs`
- Define `Module` trait with `name()`, `id()`, `subscriptions()`, `loading_strategy()`, `render()`
- Define `LoadingStrategy` enum (Eager, Lazy, Hybrid)
- Define `ModuleContext` struct
- Define `ModuleOutput` struct

### 1.2 Create `src/modules/registry.rs`
- Implement `ModuleRegistry` for managing available modules
- Implement `default()` and `export_mode()` factory methods
- Add module discovery and filtering by loading strategy

### 1.3 Create `src/modules/output.rs`
- Define `DirectoryEntry`, `IndexEntry`, `IndexSection`, `IndexContent`
- Implement output aggregation from multiple modules
- Handle conflict resolution for overlapping outputs

### 1.4 Create `src/modules/context.rs`
- Implement JSONL streaming reader for intermediate files
- Add caching layer for frequently accessed data
- Provide helper methods for common queries (e.g., `get_by_compile_id`)

### 1.5 Update `lib.rs`
- Add module system integration point
- Implement `render_with_modules()` function
- Add module execution orchestration (eager first, then lazy-capable)

### 1.6 Update `cli.rs`
- Add `--modules` flag to specify which modules to run
- Add `--skip-modules` flag to exclude modules
- Add `--list-modules` flag to list available modules

## API Design

```rust
// src/modules/mod.rs
pub mod registry;
pub mod output;
pub mod context;

pub trait Module: Send + Sync {
    fn name(&self) -> &'static str;
    fn id(&self) -> &'static str;
    fn subscriptions(&self) -> &[IntermediateFileType];
    fn loading_strategy(&self) -> LoadingStrategy;
    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput>;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadingStrategy {
    Eager,
    Lazy,
    Hybrid,
}

// src/modules/context.rs
pub struct ModuleContext<'a> {
    pub intermediate_dir: &'a Path,
    pub output_dir: &'a Path,
    pub manifest: &'a IntermediateManifest,
    pub config: &'a ModuleConfig,
}

impl ModuleContext<'_> {
    /// Read and parse a JSONL intermediate file
    pub fn read_jsonl(&self, file_type: IntermediateFileType) -> anyhow::Result<Vec<IntermediateEntry>> {
        let path = self.intermediate_dir.join(file_type.filename());
        // ... streaming JSONL parser
    }

    /// Get entries from a file filtered by compile_id
    pub fn get_entries_for_compile(
        &self,
        file_type: IntermediateFileType,
        compile_id: &str,
    ) -> anyhow::Result<Vec<IntermediateEntry>> {
        // ... filtered access
    }
}

pub struct ModuleConfig {
    pub plain_text: bool,           // Output plain text instead of HTML
    pub custom_header_html: String, // Custom header for index page
    pub export_mode: bool,          // Export mode active
    pub inductor_provenance: bool,  // Provenance tracking enabled
}

// src/modules/output.rs
pub struct ModuleOutput {
    pub files: Vec<(PathBuf, String)>,
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,
    pub index_entries: Vec<IndexEntry>,
    pub lazy_scripts: Vec<PathBuf>,
}

pub struct DirectoryEntry {
    pub name: String,
    pub url: String,
    pub lazy_loader: Option<String>,
    pub suffix: String,                    // For status indicators (✅/❌/❓)
    pub cache_status: Option<CacheStatus>, // Cache hit/miss/bypass
}

pub struct IndexEntry {
    pub section: IndexSection,
    pub title: String,
    pub content: IndexContent,
}

pub enum IndexSection {
    StackTrie,
    Diagnostics,
    CompileDirectory,
    Downloads,
    Custom(String),
}

pub enum IndexContent {
    Html(String),
    Link(String),
    Lazy { container_id: String, loader: String },
    Hybrid { summary_html: String, detail_url: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
    Bypass,
}
```

## Utility Functions

Define shared utility functions in `src/modules/utils.rs`:

```rust
/// Highlight Python code using syntect
pub fn highlight_python(code: &str) -> anyhow::Result<String>;

/// Pretty-print JSON with indentation
pub fn format_json_pretty(json_str: &str) -> anyhow::Result<String>;

/// Wrap source code in HTML with line anchors
pub fn anchor_source(text: &str) -> String;

/// Extract eval_with_key ID from filename
pub fn extract_eval_with_key_id(name: &str) -> Option<&str>;

/// Format a stack trace as collapsible HTML
pub fn format_stack(stack: &StackSummary, caption: &str, open: bool) -> String;
```

## Acceptance Criteria
- [ ] Module trait is implemented and documented
- [ ] Registry can load and filter modules
- [ ] Context provides efficient JSONL access
- [ ] CLI flags are working
- [ ] At least one test module demonstrates the pattern

## Dependencies
- Requires intermediate file generation (already implemented)

## Estimated Complexity
Medium - This is foundational infrastructure that other modules depend on.
