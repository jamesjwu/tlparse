# Sub-Issue #1: Core Module Infrastructure

## Summary
Implement the core infrastructure for the module system. A module is simply a mapping from subscribed intermediate JSONL files to output files.

## Tasks

### 1.1 Create `src/modules/mod.rs`
- Define `Module` trait with `name()`, `id()`, `subscriptions()`, `render()`
- Define `ModuleContext` struct
- Define `ModuleOutput` struct

### 1.2 Create `src/modules/registry.rs`
- Implement `ModuleRegistry` for managing available modules
- Implement `default()` and `export_mode()` factory methods

### 1.3 Create `src/modules/context.rs`
- Implement JSONL reader for intermediate files
- Provide helper methods for common queries (e.g., `get_by_compile_id`)

### 1.4 Update `lib.rs`
- Add module system integration point
- Implement `render_with_modules()` function

### 1.5 Update `cli.rs`
- Add `--modules` flag to specify which modules to run
- Add `--skip-modules` flag to exclude modules

## API Design

```rust
// src/modules/mod.rs
pub mod registry;
pub mod context;

/// A module transforms intermediate JSONL files into output files
pub trait Module: Send + Sync {
    /// Human-readable name
    fn name(&self) -> &'static str;

    /// Short identifier used in file names
    fn id(&self) -> &'static str;

    /// Which intermediate files this module reads from
    fn subscriptions(&self) -> &[IntermediateFileType];

    /// Generate outputs from intermediate data
    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput>;
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
        // ... JSONL parser
    }

    /// Get entries filtered by compile_id
    pub fn get_entries_for_compile(
        &self,
        file_type: IntermediateFileType,
        compile_id: &str,
    ) -> anyhow::Result<Vec<IntermediateEntry>> {
        // ... filtered access
    }
}

pub struct ModuleConfig {
    pub plain_text: bool,
    pub custom_header_html: String,
    pub export_mode: bool,
}

// src/modules/output.rs
pub struct ModuleOutput {
    /// Files to write (relative path -> content)
    pub files: Vec<(PathBuf, String)>,
    /// Entries to add to compile directory (compile_id -> entries)
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,
    /// Content to add to index.html
    pub index_html: Option<IndexContribution>,
}

pub struct DirectoryEntry {
    pub name: String,
    pub url: String,
    pub suffix: String,  // For status indicators like ✅/❌/❓
}

pub struct IndexContribution {
    /// Section name (e.g., "Stack Trie", "Diagnostics")
    pub section: String,
    /// HTML content to insert
    pub html: String,
}
```

## Module Registry

```rust
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    pub fn default() -> Self {
        Self {
            modules: vec![
                Box::new(StackTrieModule::new()),
                Box::new(CompileDirectoryModule::new()),
                Box::new(CompileArtifactsModule::new()),
                Box::new(CacheModule::new()),
                Box::new(CompilationMetricsModule::new()),
                Box::new(GuardsModule::new()),
                Box::new(SymbolicShapesModule::new()),
                Box::new(ChromiumTraceModule::new()),
            ],
        }
    }

    pub fn export_mode() -> Self {
        Self {
            modules: vec![
                Box::new(ExportModule::new()),
                Box::new(SymbolicShapesModule::new()),
            ],
        }
    }

    pub fn render_all(&self, ctx: &ModuleContext) -> anyhow::Result<CombinedOutput> {
        let mut combined = CombinedOutput::default();
        for module in &self.modules {
            let output = module.render(ctx)?;
            combined.merge(output);
        }
        Ok(combined)
    }
}
```

## Utility Functions

Shared utilities in `src/modules/utils.rs`:

```rust
pub fn highlight_python(code: &str) -> anyhow::Result<String>;
pub fn format_json_pretty(json_str: &str) -> anyhow::Result<String>;
pub fn anchor_source(text: &str) -> String;
pub fn format_stack(stack: &StackSummary, caption: &str, open: bool) -> String;
```

## Acceptance Criteria
- [ ] Module trait is implemented
- [ ] Registry can run all modules
- [ ] Context provides JSONL access
- [ ] At least one module works end-to-end

## Notes on Future Lazy Loading
Lazy loading can be added later as an optional trait or wrapper:
```rust
// Future extension - not part of initial implementation
pub trait LazyModule: Module {
    fn client_script(&self) -> Option<&str>;
    fn supports_lazy_load(&self) -> bool;
}
```

## Dependencies
- Requires intermediate file generation (already implemented)

## Estimated Complexity
Low-Medium - Simplified API makes this straightforward.
