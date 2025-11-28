# Modular Architecture: Module System Implementation

## Overview

This issue tracks the implementation of a modular tlparse architecture where independent "modules" subscribe to intermediate JSONL files and produce outputs. The goal is to achieve feature parity with the current `lib.rs` implementation while enabling:

1. **Modularity**: Each feature is self-contained in its own module file
2. **Extensibility**: New modules can be added without modifying core code
3. **Future lazy loading**: Module outputs can later be made lazy-loadable (not in initial implementation)

## Module API

A module is simply a transformation from intermediate JSONL files to output files:

```rust
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

/// Context passed to modules during rendering
pub struct ModuleContext<'a> {
    pub intermediate_dir: &'a Path,
    pub output_dir: &'a Path,
    pub manifest: &'a IntermediateManifest,
    pub config: &'a ModuleConfig,
}

pub struct ModuleConfig {
    pub plain_text: bool,
    pub custom_header_html: String,
    pub export_mode: bool,
}

/// Output from a module
pub struct ModuleOutput {
    /// Files to write (relative path -> content)
    pub files: Vec<(PathBuf, String)>,
    /// Entries to add to compile directory (compile_id -> entries)
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,
    /// Content to add to index.html
    pub index_html: Option<IndexContribution>,
}

/// Entry in the compile directory
pub struct DirectoryEntry {
    pub name: String,
    pub url: String,
    pub suffix: String,  // For status indicators (✅/❌/❓)
}

/// Contribution to the index page
pub struct IndexContribution {
    pub section: String,  // e.g., "Stack Trie", "Diagnostics"
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

## Intermediate File Changes

To better support lazy loading and cache tracking, we enhance the intermediate files:

### 1. Add `cache.jsonl` for Cache Artifacts
Separate cache-related artifacts (cache_hit_*, cache_miss_*, cache_bypass_*) from regular artifacts:

```jsonl
{"type":"artifact","compile_id":"0_0_0","cache_status":"hit","name":"cache_hit_abc123","encoding":"json","payload":"..."}
{"type":"artifact","compile_id":"0_0_1","cache_status":"miss","name":"cache_miss_def456","encoding":"json","payload":"..."}
```

### 2. Enhanced `manifest.json`
```json
{
  "version": "2.1",
  "generated_at": "2024-11-28T12:00:00Z",
  "source_file": "/path/to/log",
  "compile_ids": [
    {
      "id": "0_0_0",
      "display_name": "0/0",
      "has_metrics": true,
      "has_graphs": true,
      "has_guards": true,
      "status": "success",
      "cache_status": "hit"
    }
  ],
  "stack_trie": { /* pre-computed stack trie structure */ },
  "failures_summary": { /* quick access to failures */ },
  "cache_summary": { "hits": 5, "misses": 2, "bypasses": 1 },
  "files": {
    "graphs": { "path": "graphs.jsonl", "count": 42 },
    "codegen": { "path": "codegen.jsonl", "count": 10 },
    "cache": { "path": "cache.jsonl", "count": 8 }
  }
}
```

## Module Implementation Plan

### Phase 1: Core Infrastructure
- Define `Module` trait and `ModuleContext`
- Create `ModuleRegistry`
- Implement `ModuleOutput` aggregation
- Add CLI support for `--modules` flag
- Add `cache.jsonl` intermediate file

### Phase 2: Implement Modules
- **StackTrieModule** - Stack trie from `compilation_metrics.jsonl` (dynamo_start entries)
- **CompileDirectoryModule** - Generate compile_directory.json
- **CompileArtifactsModule** - All graphs, codegen, and generic artifacts
- **CacheModule** - Cache status tracking with ✅/❌/❓ indicators
- **CompilationMetricsModule** - Metrics HTML + failures_and_restarts.html
- **GuardsModule** - dynamo_guards.html + dynamo_cpp_guards_str.txt
- **SymbolicShapesModule** - symbolic_guard_information.html
- **ChromiumTraceModule** - Pass-through chromium_events.json

### Phase 3: Special Modes
- **ExportModule** - Export mode index and failures
- **MultiRankModule** - Cross-rank analysis

## Sub-Issues

| # | Issue | File | Description |
|---|-------|------|-------------|
| 1 | Core Module Infrastructure | `01-core-module-infrastructure.md` | Module trait, registry, context |
| 2 | StackTrieModule | `02-stack-trie-module.md` | Stack trie from dynamo_start |
| 3 | CompileDirectoryModule | `03-compile-directory-module.md` | compile_directory.json generation |
| 4 | CompileArtifactsModule | `04-compile-artifacts-module.md` | Graphs, codegen, artifacts |
| 5 | CacheModule | `05-cache-module.md` | Cache hit/miss/bypass tracking |
| 6 | CompilationMetricsModule | `06-compilation-metrics-module.md` | Metrics + failures |
| 7 | GuardsModule | `07-guards-module.md` | dynamo_guards + cpp_guards |
| 8 | SymbolicShapesModule | `08-symbolic-shapes-module.md` | Symbolic guard information |
| 9 | ChromiumTraceModule | `09-chromium-trace-module.md` | Perfetto trace pass-through |
| 10 | ExportModule | `10-export-module.md` | Export mode support |
| 11 | MultiRankModule | `11-multi-rank-module.md` | Cross-rank analysis |

All sub-issue files are located in `docs/issues/`.

## Future: Lazy Loading

Lazy loading can be added later as an extension. A module that supports lazy loading might implement an additional trait:

```rust
pub trait LazyModule: Module {
    fn client_script(&self) -> Option<&str>;
}
```

This is not part of the initial implementation.

## Success Criteria

1. **Feature Parity**: All current tlparse outputs are reproducible
2. **Modularity**: Each module is self-contained in its own file
3. **Extensibility**: Adding a new module requires no changes to core code
4. **Backward Compatibility**: `tlparse <log> -o <dir>` produces identical output

## Migration Strategy

1. Implement modules alongside existing code
2. Add `--use-modules` flag to opt into new system
3. Validate output equivalence with existing system
4. Make modules the default, keep `--legacy` escape hatch
5. Remove legacy code after stabilization
