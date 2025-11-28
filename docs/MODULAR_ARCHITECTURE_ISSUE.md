# Modular Architecture: Module System Implementation

## Overview

This issue tracks the implementation of a modular tlparse architecture where independent "modules" subscribe to intermediate JSONL files and produce outputs. The goal is to achieve feature parity with the current `lib.rs` implementation while enabling:

1. **Lazy loading**: Most outputs can be generated on-demand in the browser instead of at parse time
2. **Modularity**: Each feature is self-contained in its own module file
3. **Extensibility**: New modules can be added without modifying core code

## Proposed Module API

```rust
/// A module that consumes intermediate JSONL files and produces output
pub trait Module: Send + Sync {
    /// Human-readable name for this module
    fn name(&self) -> &'static str;

    /// Short identifier used in URLs and file names
    fn id(&self) -> &'static str;

    /// Which intermediate files this module subscribes to
    fn subscriptions(&self) -> &[IntermediateFileType];

    /// Whether this module must run eagerly (at parse time) vs lazily (on-demand)
    fn loading_strategy(&self) -> LoadingStrategy;

    /// Generate outputs from intermediate data
    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoadingStrategy {
    /// Must be pre-rendered at parse time (e.g., index, compile directory)
    Eager,
    /// Can be loaded on-demand via client-side JavaScript
    Lazy,
    /// Hybrid: produces minimal eager output + lazy-loadable details
    Hybrid,
}

/// Context passed to modules during rendering
pub struct ModuleContext<'a> {
    /// Directory containing intermediate JSONL files
    pub intermediate_dir: &'a Path,
    /// Output directory for generated files
    pub output_dir: &'a Path,
    /// Parsed manifest
    pub manifest: &'a IntermediateManifest,
    /// Configuration options
    pub config: &'a ModuleConfig,
    /// Compile IDs to process (None = all)
    pub compile_ids: Option<&'a [String]>,
}

/// Output from a module
pub struct ModuleOutput {
    /// Files to write (relative path -> content)
    pub files: Vec<(PathBuf, String)>,
    /// Entries to add to the compile directory index
    pub directory_entries: HashMap<String, Vec<DirectoryEntry>>,
    /// Global index entries (shown on main page)
    pub index_entries: Vec<IndexEntry>,
    /// JavaScript files to include for lazy loading
    pub lazy_scripts: Vec<PathBuf>,
}

/// Entry in the compile directory
pub struct DirectoryEntry {
    pub name: String,
    pub url: String,
    /// For lazy modules: JS function to call to generate content
    pub lazy_loader: Option<String>,
    /// Suffix for status indicators (✅/❌/❓)
    pub suffix: String,
    /// Cache status for cache artifacts
    pub cache_status: Option<CacheStatus>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
    Bypass,
}

/// Entry on the main index page
pub struct IndexEntry {
    pub section: IndexSection,
    pub title: String,
    pub content: IndexContent,
}

pub enum IndexContent {
    /// Pre-rendered HTML
    Html(String),
    /// URL to load
    Link(String),
    /// Lazy-loaded content (container ID + loader function)
    Lazy { container_id: String, loader: String },
    /// Hybrid: summary shown eagerly, details loaded on demand
    Hybrid { summary_html: String, detail_url: String },
}

pub enum IndexSection {
    StackTrie,
    Diagnostics,
    CompileDirectory,
    Downloads,
    Custom(String),
}
```

## Module Registry

```rust
/// Registry of all available modules
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    pub fn default() -> Self {
        Self {
            modules: vec![
                // Eager modules (must run at parse time)
                Box::new(StackTrieModule::new()),
                Box::new(CompileDirectoryModule::new()),
                Box::new(IndexModule::new()),

                // Hybrid modules (minimal eager + lazy details)
                Box::new(CompilationMetricsModule::new()), // Includes failures

                // Lazy modules (all content generated on-demand)
                Box::new(CompileArtifactsModule::new()), // Graphs, codegen, artifacts
                Box::new(CacheModule::new()),
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

### 2. Add `index.jsonl` for quick navigation
Contains pre-computed navigation data:
```jsonl
{"type":"compile_summary","compile_id":"0_0_0","status":"success","duration_ms":1234,"stack_hash":"abc123"}
{"type":"stack_trie_node","path":["model.py:forward"],"terminal":["0_0_0"]}
```

### 3. Payload Chunking (for large files)
For lazy loading, we may want to split large payloads:
```jsonl
{"type":"inductor_output_code","compile_id":"0_0_0","chunk":0,"total_chunks":3,"payload":"..."}
```

## Lazy Loading Architecture

### Client-Side JavaScript

```javascript
// modules.js - Module loader framework
class ModuleLoader {
  constructor(manifestUrl) {
    this.manifest = null;
    this.cache = new Map();
    this.manifestUrl = manifestUrl;
  }

  async init() {
    const resp = await fetch(this.manifestUrl);
    this.manifest = await resp.json();
  }

  // Load a specific compile ID's data from a JSONL file
  async loadCompileData(fileType, compileId) {
    const cacheKey = `${fileType}:${compileId}`;
    if (this.cache.has(cacheKey)) {
      return this.cache.get(cacheKey);
    }

    const fileInfo = this.manifest.files[fileType];
    const resp = await fetch(fileInfo.path);
    const text = await resp.text();

    // Parse JSONL and filter by compile_id
    const entries = text.split('\n')
      .filter(line => line.trim())
      .map(line => JSON.parse(line))
      .filter(entry => entry.compile_id === compileId);

    this.cache.set(cacheKey, entries);
    return entries;
  }

  // Render a graph viewer for a compile ID
  async renderGraphViewer(containerId, compileId) {
    const container = document.getElementById(containerId);
    container.innerHTML = '<div class="loading">Loading graphs...</div>';

    const graphs = await this.loadCompileData('graphs', compileId);
    container.innerHTML = renderGraphsHtml(graphs);
  }

  // Render inductor output code
  async renderInductorCode(containerId, compileId) {
    const container = document.getElementById(containerId);
    container.innerHTML = '<div class="loading">Loading code...</div>';

    const codegen = await this.loadCompileData('codegen', compileId);
    const outputCode = codegen.find(e => e.type === 'inductor_output_code');
    if (outputCode) {
      container.innerHTML = highlightPython(outputCode.payload);
    }
  }
}

// Initialize on page load
const loader = new ModuleLoader('manifest.json');
loader.init();
```

### Index Page Structure

```html
<!DOCTYPE html>
<html>
<head>
  <title>tlparse</title>
  <link rel="stylesheet" href="styles.css">
</head>
<body>
  <!-- Eager content: always rendered at parse time -->
  <header>
    <h1>Compilation Report</h1>
    <nav id="quick-stats">...</nav>
  </header>

  <!-- Stack Trie: pre-rendered (eager) -->
  <section id="stack-trie">
    <!-- Pre-rendered stack trie HTML -->
  </section>

  <!-- Compile Directory: minimal eager shell + lazy details -->
  <section id="compile-directory">
    <ul id="compile-list">
      <!-- Each item is clickable to expand lazy content -->
      <li data-compile-id="0_0_0">
        <span class="compile-header" onclick="toggleCompile('0_0_0')">
          0/0 - forward (success)
        </span>
        <div id="compile-0_0_0-details" class="lazy-container">
          <!-- Loaded on click -->
        </div>
      </li>
    </ul>
  </section>

  <!-- Downloads section -->
  <section id="downloads">
    <a href="graphs.jsonl" download>Download Graphs</a>
    <a href="chromium_events.json" download>Chromium Trace</a>
  </section>

  <script src="modules.js"></script>
  <script>
    async function toggleCompile(compileId) {
      const container = document.getElementById(`compile-${compileId}-details`);
      if (container.dataset.loaded) {
        container.style.display = container.style.display === 'none' ? 'block' : 'none';
        return;
      }

      // Lazy load compile details
      container.innerHTML = '<div class="loading">Loading...</div>';
      container.style.display = 'block';

      // Load and render all artifacts for this compile
      await Promise.all([
        loader.renderGraphViewer(`graphs-${compileId}`, compileId),
        loader.renderInductorCode(`code-${compileId}`, compileId),
        loader.renderMetrics(`metrics-${compileId}`, compileId),
      ]);

      container.dataset.loaded = 'true';
    }
  </script>
</body>
</html>
```

## Module Implementation Plan

### Phase 1: Core Infrastructure
- Define `Module` trait and `ModuleContext`
- Create `ModuleRegistry` with loading strategy support
- Implement `ModuleOutput` aggregation
- Add CLI support for `--modules` flag
- Add `cache.jsonl` intermediate file

### Phase 2: Eager Modules (Must be pre-rendered)
- **StackTrieModule** - Build and render stack trie from `compilation_metrics.jsonl` (dynamo_start entries)
- **CompileDirectoryModule** - Generate compile_directory.json
- **IndexModule** - Main index.html shell with lazy loading setup

### Phase 3: Hybrid Modules
- **CompilationMetricsModule** - Metrics HTML + failures_and_restarts.html (summary eager, details lazy)
- **CacheModule** - Cache status tracking with ✅/❌/❓ indicators

### Phase 4: Lazy Modules
- **CompileArtifactsModule** - All graphs, codegen, and generic artifacts
- **GuardsModule** - dynamo_guards.html + dynamo_cpp_guards_str.txt
- **SymbolicShapesModule** - symbolic_guard_information.html
- **ChromiumTraceModule** - Pass-through chromium_events.json

### Phase 5: Special Modes
- **ExportModule** - Export mode index and failures
- **MultiRankModule** - Cross-rank analysis (remains eager due to complexity)

### Phase 6: Client-Side Framework
- Core ModuleLoader JavaScript class
- Per-module lazy loading scripts
- Syntax highlighting integration

## Sub-Issues

Detailed implementation plans for each component:

| # | Issue | File | Description |
|---|-------|------|-------------|
| 1 | Core Module Infrastructure | `01-core-module-infrastructure.md` | Module trait, registry, context |
| 2 | StackTrieModule | `02-stack-trie-module.md` | Stack trie from dynamo_start |
| 3 | CompileDirectoryModule | `03-compile-directory-module.md` | compile_directory.json generation |
| 4 | CompileArtifactsModule | `04-compile-artifacts-module.md` | Graphs, codegen, artifacts (consolidated) |
| 5 | CacheModule | `05-cache-module.md` | Cache hit/miss/bypass tracking |
| 6 | CompilationMetricsModule | `06-compilation-metrics-module.md` | Metrics + failures (combined) |
| 7 | GuardsModule | `07-guards-module.md` | dynamo_guards + cpp_guards |
| 8 | SymbolicShapesModule | `08-symbolic-shapes-module.md` | Symbolic guard information |
| 9 | ChromiumTraceModule | `09-chromium-trace-module.md` | Perfetto trace pass-through |
| 10 | ExportModule | `10-export-module.md` | Export mode support |
| 11 | Client-Side Framework | `11-client-side-framework.md` | JavaScript lazy loading |
| 12 | MultiRankModule | `12-multi-rank-module.md` | Cross-rank analysis |
| 13 | IndexModule | `13-index-module.md` | Main shell page generation |

All sub-issue files are located in `docs/issues/`.

## Success Criteria

1. **Feature Parity**: All current tlparse outputs are reproducible
2. **Modularity**: Each module is self-contained in its own file
3. **Lazy Loading**: At least 50% of content can be lazy-loaded
4. **Performance**: Parse time reduced by deferring rendering
5. **Extensibility**: Adding a new module requires no changes to core code
6. **Backward Compatibility**: `tlparse <log> -o <dir>` produces identical output

## Migration Strategy

1. Implement modules alongside existing code
2. Add `--use-modules` flag to opt into new system
3. Validate output equivalence with existing system
4. Make modules the default, keep `--legacy` escape hatch
5. Remove legacy code after stabilization
