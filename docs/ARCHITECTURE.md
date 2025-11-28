# tlparse Architecture Documentation

## Overview

`tlparse` is a Rust-based command-line tool for parsing and visualizing PyTorch 2.0 (PT2) structured trace logs. It transforms raw log output from `TORCH_TRACE` into an interactive HTML report that helps developers understand compilation behavior, diagnose issues, and analyze multi-rank distributed training scenarios.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CLI (cli.rs)                                   │
│  - Argument parsing (clap)                                                  │
│  - File discovery (single file, directory, multi-rank)                      │
│  - Output directory management                                              │
│  - Browser launching                                                        │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Core Parser (lib.rs)                              │
│  - Log line parsing (JSONL format)                                          │
│  - String table interning                                                   │
│  - Envelope deserialization                                                 │
│  - Parser orchestration                                                     │
│  - Multi-rank coordination                                                  │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                    ┌─────────────────┼─────────────────┐
                    ▼                 ▼                 ▼
┌──────────────────────┐  ┌──────────────────┐  ┌──────────────────────────┐
│  Structured Parsers  │  │   Index/Report   │  │   Multi-Rank Analysis    │
│    (parsers.rs)      │  │    Generation    │  │                          │
│                      │  │                  │  │  - Divergence detection  │
│  - SentinelFileParser│  │  - Stack trie    │  │  - Runtime estimation    │
│  - GraphDumpParser   │  │  - Compile dirs  │  │  - Collective schedules  │
│  - InductorCodeParser│  │  - Failures HTML │  │  - Tensor metadata       │
│  - CompilationMetrics│  │  - Landing page  │  │  - Execution order       │
│  - ArtifactParser    │  │                  │  │                          │
│  - etc.              │  │                  │  │                          │
└──────────────────────┘  └──────────────────┘  └──────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Templates (templates.rs)                            │
│  - HTML templates (TinyTemplate)                                            │
│  - CSS styles                                                               │
│  - JavaScript for interactivity                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Output Files                                     │
│  - index.html (main landing page)                                           │
│  - compile_directory.json                                                   │
│  - Per-compile-id directories with artifacts                                │
│  - chromium_events.json (Perfetto traces)                                   │
│  - Multi-rank: rank_N/ subdirectories                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. CLI Module (`src/cli.rs`)

The CLI module handles command-line argument parsing and orchestrates the overall execution flow.

**Key Structures:**
- `Args`: Clap-derived struct defining all CLI options
- `OutputArg`: Enum for output mode (stdout or directory)

**Key Features:**
- Single file parsing: `tlparse <file.log>`
- Directory parsing with latest file selection: `tlparse --latest <dir>`
- Multi-rank parsing: `tlparse --all-ranks-html <dir>`
- Custom header injection: `--custom-header-html`
- Output format control: `--plain-text`, `--export`, `--inductor-provenance`

**Multi-Rank Mode Flow:**
1. Discovers rank files via pattern: `dedicated_log_torch_trace_rank_N*.log`
2. Parses each rank's log independently
3. Writes per-rank output to `rank_N/` subdirectories
4. Generates combined landing page with diagnostics
5. Combines chromium events across ranks
6. Performs cross-rank analysis (divergence, runtime, collectives)

### 2. Core Library (`src/lib.rs`)

The heart of the parsing logic, containing ~2500 lines of Rust code.

**Key Structures:**

```rust
pub struct ParseConfig {
    pub strict: bool,              // Fail on parse errors
    pub plain_text: bool,          // Output as plain text vs HTML
    pub custom_header_html: String, // Custom header for index
    pub export: bool,              // Export mode for torch.export
    pub inductor_provenance: bool, // Enable provenance tracking
}

pub struct ParseOutput {
    pub output_files: Vec<(PathBuf, String)>,
    pub compile_ids: Vec<String>,
    pub chromium_events: Vec<ChromiumEvent>,
    pub cache_events: Vec<CacheEvent>,
    // ... other diagnostic data
}
```

**Parsing Pipeline:**

1. **Log Reading**: Read JSONL-formatted log file line by line
2. **String Interning**: Parse and maintain a string table for efficient storage
3. **Envelope Deserialization**: Convert JSON to typed `Envelope` structs
4. **Parser Dispatch**: Route each envelope to appropriate `StructuredLogParser`
5. **File Generation**: Collect parser outputs into file map
6. **Index Generation**: Build stack trie and compile directory
7. **Report Rendering**: Generate HTML using TinyTemplate

**Important Functions:**
- `parse_path()`: Main entry point for parsing a single log file
- `parse_payload()`: Deserialize a single log entry with its payload
- `build_line_mappings()`: Build provenance tracking line mappings
- `analyze_execution_order()`: Multi-rank execution order analysis

### 3. Parsers Module (`src/parsers.rs`)

Implements the `StructuredLogParser` trait for different log entry types.

**The Parser Trait:**
```rust
pub trait StructuredLogParser {
    fn get_metadata<'e>(&self, e: &'e Envelope) -> Option<Metadata<'e>>;
    fn parse<'e>(
        &self,
        lineno: usize,
        metadata: Metadata<'e>,
        rank: Option<u32>,
        compile_id: &Option<CompileId>,
        payload: &str,
    ) -> anyhow::Result<ParserResults>;
    fn name(&self) -> &'static str;
}
```

**Parser Output Types:**
```rust
pub enum ParserOutput {
    File(PathBuf, String),          // File with content
    GlobalFile(PathBuf, String),    // Non-unique suffix file
    PayloadFile(PathBuf),           // Use payload directly
    PayloadReformatFile(PathBuf, fn(&str) -> Result<String, anyhow::Error>),
    Link(String, String),           // External href
}
```

**Implemented Parsers:**

| Parser | Handles | Output |
|--------|---------|--------|
| `SentinelFileParser` | Various graph dumps | `*.txt` files |
| `GraphDumpParser` | Generic graph dumps | `{name}.txt` |
| `DynamoOutputGraphParser` | Dynamo output graphs | `dynamo_output_graph.txt` |
| `DynamoGuardParser` | Guard information | `dynamo_guards.html` |
| `InductorOutputCodeParser` | Generated kernel code | `inductor_output_code.html` |
| `CompilationMetricsParser` | Compile time metrics | `compilation_metrics.html` |
| `AOTAutogradBackwardCompilationMetricsParser` | AOT backward metrics | HTML |
| `BwdCompilationMetricsParser` | Backward compilation | HTML |
| `ArtifactParser` | Generic artifacts | `*.txt` or `*.json` |
| `DumpFileParser` | Dump files | `dump_file/*.html` |
| `LinkParser` | External links | Links in directory |
| `PropagateRealTensorsParser` | Symbolic shape info | `symbolic_guard_information.html` |
| `OptimizeDdpSplitChildParser` | DDP split graphs | `optimize_ddp_split_child_*.txt` |

### 4. Types Module (`src/types.rs`)

Defines all data structures for deserialization and internal representation.

**Core Types:**

```rust
#[derive(Deserialize)]
pub struct Envelope {
    pub rank: Option<u32>,
    pub compile_id: Option<CompileId>,
    // Metadata fields for each log type:
    pub dynamo_output_graph: Option<DynamoOutputGraphMetadata>,
    pub inductor_output_code: Option<InductorOutputCodeMetadata>,
    pub compilation_metrics: Option<CompilationMetricsMetadata>,
    // ... many more
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CompileId {
    pub frame_id: u32,
    pub frame_compile_id: Option<u32>,
    pub attempt: Option<u32>,
    pub compiled_autograd_id: Option<u32>,
}
```

**Key Metadata Types:**
- `CompilationMetricsMetadata`: Compile times, cache metrics, graph metrics
- `InductorOutputCodeMetadata`: Generated code filename
- `SymbolicShapeSpecializationMetadata`: Shape specialization info
- `DynamoGuard`: Guard expressions and code
- `StackSummary` / `FrameSummary`: Stack trace representation

**Multi-Rank Analysis Types:**
- `CollectiveSchedule`: Per-rank collective operation sequences
- `GraphRuntime` / `OpRuntime`: Runtime estimation data
- `TensorMetaFingerprint`: Tensor metadata for divergence detection
- `CacheEvent`: Cache hit/miss tracking

### 5. Templates Module (`src/templates.rs`)

Contains HTML templates, CSS, and JavaScript for the generated reports.

**Key Templates:**
- `TEMPLATE_INDEX`: Main landing page with stack trie
- `TEMPLATE_COMPILATION_METRICS`: Per-compilation detail page
- `TEMPLATE_FAILURES_AND_RESTARTS`: Compilation failures summary
- `TEMPLATE_MULTI_RANK_INDEX`: Multi-rank landing page
- `TEMPLATE_PROVENANCE_TRACKING`: Inductor provenance viewer
- `TEMPLATE_EXPORT_INDEX`: Export mode landing page

**Styling:**
- `CSS`: Main stylesheet
- `EXPORT_CSS`: Export-specific styles
- `FAILURES_CSS`: Failure table styles
- `PROVENANCE_CSS`: Provenance viewer styles

**JavaScript:**
- `JAVASCRIPT`: Stack trie toggle functionality
- `TEMPLATE_QUERY_PARAM_SCRIPT`: Query parameter preservation
- `PROVENANCE_JS`: Interactive provenance viewer

## Data Flow

### Single-File Parsing

```
1. Read log file
   └── Split into lines
       └── Parse first line as string table (if present)

2. For each line:
   ├── Parse JSON envelope
   ├── Resolve interned strings
   ├── Match against registered parsers
   └── Generate output files

3. Build indexes:
   ├── Stack trie (hierarchical call stacks)
   ├── Compile directory (artifacts per compile_id)
   ├── Symbolic shape specialization index
   └── Guard index

4. Generate HTML:
   ├── index.html (landing page)
   ├── compile_directory.json
   ├── failures_and_restarts.html
   ├── chromium_events.json
   └── Per-compile-id artifact directories
```

### Multi-Rank Parsing

```
1. Discover rank files in directory
   └── Pattern: dedicated_log_torch_trace_rank_N*.log

2. Parse each rank independently
   └── Output to rank_N/ subdirectory

3. Cross-rank analysis:
   ├── Compile ID divergence detection
   ├── Cache event pattern comparison
   ├── Collective schedule comparison
   ├── Tensor metadata fingerprinting
   ├── Runtime estimation variance
   └── Execution order analysis

4. Generate combined outputs:
   ├── index.html (multi-rank landing)
   ├── chromium_events.json (combined traces)
   ├── chromium_trace_with_runtime.json
   ├── collective_schedules.json
   └── runtime_estimations.json
```

## Key Algorithms

### Stack Trie (`StackTrieNode`)

The stack trie is a prefix tree that organizes compilation events by their call stacks:

```rust
pub struct StackTrieNode {
    pub frame: Option<FrameSummary>,
    pub terminal: Vec<(CompileId, Option<CompilationMetricsMetadata>)>,
    pub children: FxHashMap<FrameSummary, StackTrieNode>,
}
```

**Algorithm:**
1. For each compilation, extract its stack trace
2. Insert stack into trie (most recent call first)
3. Terminal nodes store compile_id and metrics
4. Render as collapsible HTML tree

### Compile ID Divergence Detection

Detects when different ranks compile different sets of graphs:

```rust
fn detect_compile_id_divergence(rank_ids: &[Vec<String>]) -> bool {
    let first = &rank_ids[0];
    rank_ids[1..].iter().any(|ids| ids != first)
}
```

### Collective Schedule Analysis

Groups ranks by their collective operation sequences:

```rust
fn group_ranks_by_collective_schedule(
    schedules: &[CollectiveSchedule]
) -> Vec<RankGroup> {
    // Hash: sorted (graph, ops) pairs -> rank list
    // Detect mismatches across ranks
}
```

### Provenance Tracking

Maps lines between compilation stages (pre-grad → post-grad → output code):

```rust
pub fn build_line_mappings(node_mappings: &Value) -> Value {
    // Build bidirectional mappings:
    // preToPost, postToPre
    // pyCodeToPost, postToPyCode
    // cppCodeToPost, postToCppCode
}
```

## Configuration Options

### ParseConfig

| Field | Default | Description |
|-------|---------|-------------|
| `strict` | false | Fail on parse errors |
| `plain_text` | false | Output plain text instead of HTML |
| `custom_header_html` | "" | Custom HTML header injection |
| `export` | false | Export mode for torch.export |
| `inductor_provenance` | false | Enable provenance tracking |

### CLI Flags

| Flag | Description |
|------|-------------|
| `--overwrite` | Overwrite existing output directory |
| `--no-browser` | Don't open browser after parsing |
| `--latest` | Parse most recent log in directory |
| `--all-ranks-html` | Multi-rank parsing mode |
| `--export` | Export analysis mode |
| `--inductor-provenance` | Enable provenance tracking |
| `--plain-text` | Plain text output |
| `--custom-header-html` | Inject custom header |

## File Output Structure

### Single-Rank Output
```
output/
├── index.html
├── compile_directory.json
├── failures_and_restarts.html
├── chromium_events.json
├── raw.jsonl
├── payloads/
│   └── {hash}.txt
├── dump_file/
│   └── *.html
└── {compile_id}/
    ├── compilation_metrics.html
    ├── dynamo_output_graph.txt
    ├── aot_forward_graph.txt
    ├── inductor_post_grad_graph.txt
    ├── inductor_output_code.html
    └── ...
```

### Multi-Rank Output
```
output/
├── index.html (combined landing page)
├── chromium_events.json (combined)
├── chromium_trace_with_runtime.json
├── collective_schedules.json
├── runtime_estimations.json
├── rank_0/
│   ├── index.html
│   ├── compile_directory.json
│   └── {compile_id}/...
├── rank_1/
│   └── ...
└── rank_N/
    └── ...
```

## Extension Points

### Adding a New Parser

1. Define metadata type in `types.rs`:
```rust
#[derive(Deserialize)]
pub struct MyNewMetadata {
    pub field: String,
}
```

2. Add to `Envelope` in `types.rs`:
```rust
pub struct Envelope {
    pub my_new_log: Option<MyNewMetadata>,
    // ...
}
```

3. Add to `Metadata` enum:
```rust
pub enum Metadata<'e> {
    MyNew(&'e MyNewMetadata),
    // ...
}
```

4. Implement parser in `parsers.rs`:
```rust
pub struct MyNewParser;
impl StructuredLogParser for MyNewParser {
    fn name(&self) -> &'static str { "my_new" }
    fn get_metadata<'e>(&self, e: &'e Envelope) -> Option<Metadata<'e>> {
        e.my_new_log.as_ref().map(|m| Metadata::MyNew(m))
    }
    fn parse<'e>(&self, ...) -> anyhow::Result<ParserResults> {
        // Generate output files
    }
}
```

5. Register in `default_parsers()`:
```rust
vec![
    Box::new(MyNewParser),
    // ...
]
```

### Adding Multi-Rank Analysis

1. Add collection function in `parsers.rs`:
```rust
pub fn read_my_analysis(out_path: &PathBuf, rank_nums: &[u32]) -> anyhow::Result<Vec<MyData>> {
    read_artifacts(out_path, rank_nums, "my_artifact_prefix", |content, rank, graph| {
        // Parse and return data
    })
}
```

2. Add to CLI multi-rank processing in `cli.rs`
3. Add to landing page template in `templates.rs`

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `serde` / `serde_json` | JSON serialization |
| `anyhow` | Error handling |
| `tinytemplate` | HTML template rendering |
| `syntect` | Syntax highlighting |
| `fxhash` | Fast hashing |
| `regex` | Pattern matching |
| `html_escape` | HTML encoding |
| `tempfile` | Temporary files (testing) |
| `assert_cmd` | CLI testing |

## Performance Considerations

1. **String Interning**: Large payloads use MD5-hashed filenames to avoid memory bloat
2. **Lazy Parsing**: Payloads are only parsed when needed by parsers
3. **FxHashMap**: Used for hot paths (faster than std HashMap)
4. **Streaming**: Log files are processed line-by-line, not loaded entirely
5. **Parallel Potential**: Multi-rank parsing is currently sequential but could be parallelized

## Testing Strategy

The test suite in `tests/integration_test.rs` covers:

1. **Basic Parsing**: Verifies expected output files are generated
2. **Parser-Specific Tests**: Tests each parser type (metrics, artifacts, chromium events)
3. **Multi-Rank Tests**: Tests rank discovery, divergence detection, combined outputs
4. **Error Handling**: Tests corrupted JSON, missing files, invalid flags
5. **Provenance Tracking**: Verifies line mapping correctness

Test inputs are stored in `tests/inputs/` and include:
- Simple single-compilation logs
- Multi-compilation with graph breaks
- Cache hit/miss scenarios
- Multi-rank log directories
- Provenance tracking logs (AOT, JIT, CUDA variants)
