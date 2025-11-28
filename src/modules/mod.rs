//! Module system for tlparse.
//!
//! A module is a simple transformation: it reads from intermediate JSONL files
//! (produced by the parsing stage) and generates output files plus directory entries.
//!
//! This enables a clean separation between parsing and rendering, and opens the door
//! for future lazy loading support.

pub mod chromium_trace;
pub mod compile_artifacts;
pub mod context;

pub use chromium_trace::ChromiumTraceModule;
pub use compile_artifacts::CompileArtifactsModule;

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;

/// A module transforms intermediate JSONL files into output files.
///
/// Modules are stateless transformations that:
/// 1. Subscribe to specific intermediate file types
/// 2. Read those files via ModuleContext
/// 3. Produce output files and directory entries
pub trait Module: Send + Sync {
    /// Human-readable name for display
    fn name(&self) -> &'static str;

    /// Short identifier used in file naming and CLI flags
    fn id(&self) -> &'static str;

    /// Which intermediate file types this module reads from
    fn subscriptions(&self) -> &[IntermediateFileType];

    /// Generate outputs from intermediate data
    fn render(&self, ctx: &context::ModuleContext) -> Result<ModuleOutput>;
}

/// Output produced by a module
#[derive(Debug, Default)]
pub struct ModuleOutput {
    /// Files to write (relative path -> content)
    pub files: Vec<(PathBuf, String)>,

    /// Entries to add to compile directory (compile_id string -> entries)
    /// Use "__global__" key for entries not tied to a specific compile_id
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,

    /// Optional content contribution to index.html
    pub index_contribution: Option<IndexContribution>,
}

/// An entry in the compile directory (displayed as a link in the UI)
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// Display name for the link
    pub name: String,
    /// URL to link to (relative path or external URL)
    pub url: String,
    /// Optional suffix (e.g., "✅" for cache hit, "❌" for cache miss)
    pub suffix: String,
}

impl DirectoryEntry {
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            suffix: String::new(),
        }
    }

    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }
}

/// Contribution to index.html from a module
#[derive(Debug, Clone)]
pub struct IndexContribution {
    /// Section name (e.g., "Stack Trie", "Diagnostics")
    pub section: String,
    /// HTML content to insert
    pub html: String,
}

/// Configuration passed to modules
#[derive(Debug, Clone)]
pub struct ModuleConfig {
    /// Whether to use plain text output (no syntax highlighting)
    pub plain_text: bool,
    /// Custom HTML to include in header
    pub custom_header_html: String,
    /// Whether running in export mode
    pub export_mode: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            plain_text: false,
            custom_header_html: String::new(),
            export_mode: false,
        }
    }
}

/// Registry of available modules
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Register a module
    pub fn register(&mut self, module: Box<dyn Module>) {
        self.modules.push(module);
    }

    /// Get all registered modules
    pub fn modules(&self) -> &[Box<dyn Module>] {
        &self.modules
    }

    /// Render all modules and combine their outputs
    pub fn render_all(&self, ctx: &context::ModuleContext) -> Result<CombinedOutput> {
        let mut combined = CombinedOutput::default();

        for module in &self.modules {
            match module.render(ctx) {
                Ok(output) => combined.merge(output),
                Err(e) => {
                    eprintln!("Module '{}' failed: {}", module.name(), e);
                    // Continue with other modules
                }
            }
        }

        Ok(combined)
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleRegistry {
    /// Create a registry with default modules for normal operation
    pub fn with_defaults(config: &ModuleConfig) -> Self {
        let mut registry = Self::new();

        // Add compile artifacts module (graphs, codegen, artifacts)
        registry.register(Box::new(CompileArtifactsModule::new(config.plain_text)));

        // Add chromium trace module
        registry.register(Box::new(ChromiumTraceModule::new()));

        // TODO: Add more modules as they are implemented:
        // - StackTrieModule
        // - CompilationMetricsModule
        // - GuardsModule
        // - CacheModule
        // - SymbolicShapesModule

        registry
    }

    /// Create a registry with modules for export mode
    pub fn for_export_mode(_config: &ModuleConfig) -> Self {
        let registry = Self::new();
        // TODO: Add export-specific modules
        registry
    }
}

/// Combined output from all modules
#[derive(Debug, Default)]
pub struct CombinedOutput {
    /// All files to write
    pub files: Vec<(PathBuf, String)>,

    /// All directory entries, keyed by compile_id string
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,

    /// All index contributions
    pub index_contributions: Vec<IndexContribution>,
}

impl CombinedOutput {
    /// Merge output from a module into this combined output
    pub fn merge(&mut self, output: ModuleOutput) {
        self.files.extend(output.files);

        for (compile_id, entries) in output.directory_entries {
            self.directory_entries
                .entry(compile_id)
                .or_default()
                .extend(entries);
        }

        if let Some(contribution) = output.index_contribution {
            self.index_contributions.push(contribution);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestModule;

    impl Module for TestModule {
        fn name(&self) -> &'static str {
            "Test Module"
        }

        fn id(&self) -> &'static str {
            "test"
        }

        fn subscriptions(&self) -> &[IntermediateFileType] {
            &[IntermediateFileType::Graphs]
        }

        fn render(&self, _ctx: &context::ModuleContext) -> Result<ModuleOutput> {
            Ok(ModuleOutput {
                files: vec![(PathBuf::from("test.txt"), "test content".to_string())],
                directory_entries: HashMap::new(),
                index_contribution: None,
            })
        }
    }

    #[test]
    fn test_module_registry() {
        let mut registry = ModuleRegistry::new();
        registry.register(Box::new(TestModule));

        assert_eq!(registry.modules().len(), 1);
        assert_eq!(registry.modules()[0].name(), "Test Module");
        assert_eq!(registry.modules()[0].id(), "test");
    }

    #[test]
    fn test_directory_entry() {
        let entry = DirectoryEntry::new("test.txt", "path/to/test.txt").with_suffix("✅");

        assert_eq!(entry.name, "test.txt");
        assert_eq!(entry.url, "path/to/test.txt");
        assert_eq!(entry.suffix, "✅");
    }
}
