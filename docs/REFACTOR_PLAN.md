# TLParse Modular Architecture Refactor Plan

## Executive Summary

This document outlines a plan to refactor tlparse from a monolithic log-to-HTML generator into a modular two-stage architecture:

1. **Stage 1 (Backend)**: Parse PyTorch structured logs → Organized intermediate JSON files
2. **Stage 2 (Frontend)**: JSON files → HTML/visualization outputs via pluggable modules

This separation enables:
- **Easier extensibility**: New analyses just need to consume existing JSON files
- **Decoupled development**: Frontend and backend can evolve independently
- **Tool interoperability**: JSON files can be consumed by external tools
- **Better testing**: Each stage can be tested in isolation

---

## Current Architecture Overview

### Today's Flow
```
PyTorch Structured Logs (.log)
         ↓
    [tlparse]
         ↓
    Output Directory:
    ├── index.html
    ├── compile_directory.json
    ├── raw.jsonl
    ├── chromium_events.json
    ├── failures_and_restarts.html
    ├── payloads/
    │   └── {md5_hash}.txt files
    └── {compile_id}/
        ├── dynamo_output_graph.txt
        ├── inductor_output_code.html
        ├── compilation_metrics.html
        └── ... more artifacts
```

### Pain Points
1. Adding new analysis requires modifying core parser code
2. `raw.jsonl` is a flat dump, not organized for consumption
3. HTML generation is interleaved with parsing logic
4. No clear contract between log types and their consumers

---

## Proposed Architecture

### New Two-Stage Flow
```
PyTorch Structured Logs (.log)
         ↓
    [Stage 1: Parse & Organize]
         ↓
    Intermediate Directory:
    ├── manifest.json              # Index of all generated files
    ├── string_table.json          # Interned strings
    ├── raw.jsonl                  # All envelopes (minus chromium)
    │
    ├── by_type/                   # Envelopes organized by type
    │   ├── dynamo_output_graph.jsonl    # Payloads inlined
    │   ├── compilation_metrics.jsonl
    │   ├── chromium_events.json
    │   ├── dynamo_guards.jsonl
    │   ├── inductor_output_code.jsonl
    │   └── ... (one file per envelope type)
    │
    └── by_compile_id/             # Envelopes organized by compile_id
        ├── 0_0_0/
        │   ├── events.jsonl       # All events for this compile_id
        │   └── summary.json       # Quick metadata
        └── 0_0_1/
            └── ...
         ↓
    [Stage 2: Module Rendering]
         ↓
    Final Output Directory:
    ├── index.html
    ├── compile_directory.json
    └── {compile_id}/
        └── ... HTML artifacts
```

**Key Design Decision: Inlined Payloads**

Payloads (graph dumps, generated code, etc.) are stored directly in the JSONL entries rather than as separate files. This works well because:
1. Files are already split by type, so each JSONL file's payloads are naturally bounded
2. Self-contained files are easier to understand and debug
3. No need to manage cross-references between files
4. Simpler for external tools to consume

---

## Stage 1: Intermediate JSON File Design

### 1.1 Directory Structure

```
intermediate/
├── manifest.json                    # Master index
├── string_table.json                # Filename interning
├── raw.jsonl                        # Complete log (minus chromium, str)
│
├── by_type/
│   │
│   │ # Graph Outputs (payloads inlined)
│   ├── dynamo_output_graph.jsonl
│   ├── optimize_ddp_split_graph.jsonl
│   ├── optimize_ddp_split_child.jsonl
│   ├── compiled_autograd_graph.jsonl
│   ├── aot_forward_graph.jsonl
│   ├── aot_backward_graph.jsonl
│   ├── aot_inference_graph.jsonl
│   ├── aot_joint_graph.jsonl
│   ├── inductor_pre_grad_graph.jsonl
│   ├── inductor_post_grad_graph.jsonl
│   ├── graph_dump.jsonl
│   │
│   │ # Code Generation
│   ├── inductor_output_code.jsonl
│   ├── dynamo_cpp_guards_str.jsonl
│   │
│   │ # Guards & Verification
│   ├── dynamo_guards.jsonl
│   ├── symbolic_shape_specialization.jsonl
│   ├── guard_added_fast.jsonl
│   │
│   │ # Compilation Metrics
│   ├── compilation_metrics.jsonl
│   ├── bwd_compilation_metrics.jsonl
│   ├── aot_autograd_backward_compilation_metrics.jsonl
│   │
│   │ # Stack Traces
│   ├── dynamo_start.jsonl
│   │
│   │ # Symbolic Shapes (for torch.export)
│   ├── propagate_real_tensors_provenance.jsonl
│   ├── guard_added.jsonl
│   ├── create_unbacked_symbol.jsonl
│   ├── expression_created.jsonl
│   │
│   │ # Events & Tracing
│   ├── chromium_events.json         # Array format for Perfetto
│   │
│   │ # Generic Artifacts
│   ├── artifact.jsonl
│   ├── dump_file.jsonl
│   ├── link.jsonl
│   │
│   │ # Tensor Metadata
│   ├── describe_tensor.jsonl
│   ├── describe_storage.jsonl
│   ├── describe_source.jsonl
│   │
│   │ # Export Mode
│   ├── missing_fake_kernel.jsonl
│   ├── mismatched_fake_kernel.jsonl
│   └── exported_program.jsonl
│
└── by_compile_id/
    ├── _none/                       # Events without compile_id
    │   ├── events.jsonl
    │   └── summary.json
    ├── 0_0_0/
    │   ├── events.jsonl             # All events for [0/0] attempt 0
    │   └── summary.json             # Quick access metadata
    └── ...
```

### 1.2 File Format Specifications

#### `manifest.json`
```json
{
  "version": "2.0",
  "generated_at": "2024-11-28T12:00:00Z",
  "source_file": "/path/to/original.log",
  "source_file_hash": "sha256:...",
  "total_envelopes": 12345,
  "envelope_counts": {
    "dynamo_output_graph": 42,
    "compilation_metrics": 42,
    "chromium_event": 5000,
    ...
  },
  "compile_ids": ["0_0_0", "0_0_1", "0_1_0", ...],
  "string_table_entries": 150,
  "parse_mode": "normal",  // or "export", "multi_rank"
  "ranks": [0],            // list of ranks if multi_rank
  "files": {
    "by_type": ["dynamo_output_graph.jsonl", ...],
    "by_compile_id": ["0_0_0/events.jsonl", ...]
  }
}
```

#### Individual JSONL Entry Format (by_type/*.jsonl)
```jsonl
{"compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00Z","thread":12345,"pathname":"torch/_dynamo/convert_frame.py","lineno":42,"metadata":{"sizes":{"L['x']":[[2,3],[3,4]]}},"payload":"def forward(self, x):\n    return x + 1"}
{"compile_id":"0_0_1",...}
```

Each entry includes:
- `compile_id`: String form of CompileId (e.g., "0_0_0" or "!0_0_0" for compiled autograd)
- `rank`: Rank number (for distributed)
- `timestamp`: ISO-8601 timestamp
- `thread`: Thread ID
- `pathname`: Source file (interned ID or full path)
- `lineno`: Line number
- `metadata`: Type-specific metadata object
- `payload`: Inlined payload content (if applicable, as string)

#### `by_compile_id/{id}/summary.json`
```json
{
  "compile_id": "0_0_0",
  "event_count": 15,
  "event_types": ["dynamo_start", "dynamo_output_graph", "compilation_metrics"],
  "has_failure": false,
  "failure_reason": null,
  "stack_summary": {
    "co_name": "forward",
    "co_filename": "model.py",
    "co_firstlineno": 42
  },
  "metrics": {
    "entire_frame_compile_time_s": 1.234,
    "backend_compile_time_s": 0.567
  }
}
```

#### `by_compile_id/{id}/events.jsonl`
```jsonl
{"type":"dynamo_start","timestamp":"...","metadata":{...}}
{"type":"dynamo_output_graph","timestamp":"...","metadata":{...},"payload":"def forward(...):\n    ..."}
{"type":"compilation_metrics","timestamp":"...","metadata":{...}}
```

### 1.3 Envelope Type Categorization

| Category | Types | Output Behavior |
|----------|-------|-----------------|
| **Graph Outputs** | dynamo_output_graph, optimize_ddp_*, compiled_autograd_graph, aot_*, inductor_*_grad_graph, graph_dump | Payload inlined |
| **Code Generation** | inductor_output_code, dynamo_cpp_guards_str | Payload inlined |
| **Guards** | dynamo_guards, symbolic_shape_specialization, guard_added_fast | Metadata only |
| **Metrics** | compilation_metrics, bwd_compilation_metrics, aot_autograd_backward_compilation_metrics | Metadata only (rich) |
| **Stack Traces** | dynamo_start | Stack in metadata |
| **Symbolic Shapes** | propagate_real_tensors_provenance, guard_added, create_unbacked_symbol, expression_created | Metadata with trees |
| **Events** | chromium_event | Special: array format JSON |
| **Artifacts** | artifact, dump_file, link | Payload inlined or URL |
| **Tensor Metadata** | describe_tensor, describe_storage, describe_source | Metadata only |
| **Export** | missing_fake_kernel, mismatched_fake_kernel, exported_program | Metadata or payload inlined |

---

## Stage 2: Module System Design

### 2.1 Module Interface

```rust
/// A module that consumes intermediate JSON and produces output
pub trait OutputModule {
    /// Human-readable name for this module
    fn name(&self) -> &'static str;

    /// Which intermediate files this module needs
    fn required_inputs(&self) -> Vec<IntermediateFile>;

    /// Generate outputs from intermediate data
    fn generate(
        &self,
        intermediate_dir: &Path,
        output_dir: &Path,
        config: &ModuleConfig,
    ) -> anyhow::Result<ModuleOutput>;
}

/// Specifies an intermediate file dependency
pub enum IntermediateFile {
    ByType(&'static str),           // e.g., "compilation_metrics"
    ByCompileId,                    // Access to by_compile_id/
    Manifest,                       // manifest.json
    StringTable,                    // string_table.json
    Raw,                            // raw.jsonl
}

/// Output from a module
pub struct ModuleOutput {
    /// Files to write (path relative to output_dir, content)
    pub files: Vec<(PathBuf, String)>,
    /// Links to add to the index
    pub index_links: Vec<IndexLink>,
    /// Per-compile-id artifacts
    pub compile_artifacts: HashMap<String, Vec<Artifact>>,
}
```

### 2.2 Planned Modules

| Module | Required Inputs | Outputs |
|--------|-----------------|---------|
| `IndexModule` | manifest, by_compile_id, compilation_metrics, dynamo_start | index.html |
| `CompileDirectoryModule` | manifest, by_compile_id | compile_directory.json |
| `GraphViewerModule` | dynamo_output_graph, aot_*, inductor_*_graph, graph_dump | {id}/*.txt files |
| `InductorCodeModule` | inductor_output_code | {id}/inductor_output_code.html |
| `CompilationMetricsModule` | compilation_metrics, symbolic_shape_specialization, guard_added_fast, dynamo_start | {id}/compilation_metrics.html |
| `FailuresModule` | compilation_metrics | failures_and_restarts.html |
| `GuardsModule` | dynamo_guards | {id}/dynamo_guards.html |
| `ChromiumModule` | chromium_events | chromium_events.json (passthrough) |
| `SymbolicShapeModule` | propagate_real_tensors_provenance, guard_added, expression_created, create_unbacked_symbol | {id}/symbolic_guard_information.html |
| `ExportModule` | missing_fake_kernel, mismatched_fake_kernel, exported_program, guard_added, propagate_real_tensors_provenance | index.html (export mode) |
| `MultiRankModule` | (multiple rank intermediate dirs) | index.html (multi-rank), diagnostics |

### 2.3 Module Registration

```rust
/// Default module set for normal mode
pub fn default_modules() -> Vec<Box<dyn OutputModule>> {
    vec![
        Box::new(IndexModule::new()),
        Box::new(CompileDirectoryModule::new()),
        Box::new(GraphViewerModule::new()),
        Box::new(InductorCodeModule::new()),
        Box::new(CompilationMetricsModule::new()),
        Box::new(FailuresModule::new()),
        Box::new(GuardsModule::new()),
        Box::new(ChromiumModule::new()),
        Box::new(SymbolicShapeModule::new()),
    ]
}

/// Modules for export mode
pub fn export_modules() -> Vec<Box<dyn OutputModule>> {
    vec![
        Box::new(ExportModule::new()),
        Box::new(SymbolicShapeModule::new()),
    ]
}
```

---

## CLI Design Changes

### New Subcommands

```
tlparse parse <input.log> -o <intermediate_dir>
    Parse log file and generate intermediate JSON files only.

tlparse render <intermediate_dir> -o <output_dir> [--modules ...]
    Render intermediate JSON files to HTML using specified modules.

tlparse <input.log> -o <output_dir>
    Legacy mode: runs both parse and render in one step.
    (Backward compatible with current behavior)
```

### New Options

```
--intermediate-dir <DIR>    Directory for intermediate JSON files
                            (default: temp directory, cleaned up after)
--keep-intermediate         Don't delete intermediate files after render
--only-intermediate         Only generate intermediate files, skip rendering
--modules <LIST>            Comma-separated list of modules to run
                            (default: all applicable modules)
--skip-modules <LIST>       Modules to skip
```

---

## Migration Path

### Phase 1: Add Intermediate Generation (Non-Breaking)
1. Add code to generate intermediate JSON files alongside current output
2. New `--only-intermediate` flag to skip HTML generation
3. Validate intermediate files match current outputs
4. **Deliverable**: New `tlparse parse` command

### Phase 2: Implement Module System
1. Extract current parsers into modules
2. Implement module interface and registration
3. Modules read from intermediate files, not raw logs
4. **Deliverable**: New `tlparse render` command

### Phase 3: Refactor Main Path
1. Default path: parse → intermediate → render
2. Intermediate files are temporary by default
3. Add `--keep-intermediate` for debugging
4. **Deliverable**: Updated default `tlparse` behavior

### Phase 4: Documentation & External Tool Support
1. Document intermediate JSON schema
2. Provide JSON schema files for validation
3. Examples of external tools consuming intermediate files
4. **Deliverable**: Developer documentation

---

## Backward Compatibility Guarantees

1. **CLI Compatibility**
   - `tlparse <input.log> -o <output_dir>` continues to work identically
   - All existing flags (`--export`, `--all-ranks-html`, etc.) continue to work
   - New flags are additive, don't change defaults

2. **Output Compatibility**
   - All existing output files are generated in the same locations
   - `compile_directory.json` format unchanged
   - `raw.jsonl` format unchanged (may be enhanced with additional fields)
   - `chromium_events.json` format unchanged

3. **Intermediate Files Are Optional**
   - Default mode generates intermediates in temp dir, cleans up after
   - Users opting into intermediate files get new capability, not breaking change

---

## Detailed Intermediate File Schemas

### `by_type/dynamo_output_graph.jsonl`
```jsonl
{
  "compile_id": "0_0_0",
  "rank": 0,
  "timestamp": "2024-11-28T12:00:00.000Z",
  "thread": 12345,
  "pathname": "torch/_dynamo/convert_frame.py",
  "lineno": 456,
  "metadata": {
    "sizes": {
      "L['x']": [[2, 3], [3, 4]],
      "L['y']": [[4, 5]]
    }
  },
  "payload": "class GraphModule(torch.nn.Module):\n    def forward(self, L_x_ : torch.Tensor):\n        l_x_ = L_x_\n        add = l_x_ + 1\n        return (add,)"
}
```

### `by_type/compilation_metrics.jsonl`
```jsonl
{
  "compile_id": "0_0_0",
  "rank": 0,
  "timestamp": "2024-11-28T12:00:05.000Z",
  "thread": 12345,
  "pathname": "torch/_dynamo/convert_frame.py",
  "lineno": 789,
  "metadata": {
    "co_name": "forward",
    "co_filename": "model.py",
    "co_firstlineno": 42,
    "cache_size": 1,
    "accumulated_cache_size": 1,
    "guard_count": 15,
    "shape_env_guard_count": 5,
    "graph_op_count": 100,
    "graph_node_count": 50,
    "graph_input_count": 3,
    "start_time": 1732795200.0,
    "entire_frame_compile_time_s": 1.234,
    "backend_compile_time_s": 0.567,
    "inductor_compile_time_s": 0.456,
    "code_gen_time_s": 0.123,
    "fail_type": null,
    "fail_reason": null,
    "fail_user_frame_filename": null,
    "fail_user_frame_lineno": null,
    "non_compliant_ops": [],
    "compliant_custom_ops": [],
    "restart_reasons": [],
    "dynamo_time_before_restart_s": null
  }
}
```

### `by_type/chromium_events.json`
```json
[
  {
    "name": "compile",
    "cat": "dynamo",
    "ph": "B",
    "ts": 1732795200000000,
    "pid": 1234,
    "tid": 5678
  },
  {
    "name": "compile",
    "cat": "dynamo",
    "ph": "E",
    "ts": 1732795201000000,
    "pid": 1234,
    "tid": 5678
  }
]
```

### `by_type/dynamo_start.jsonl`
```jsonl
{
  "compile_id": "0_0_0",
  "rank": 0,
  "timestamp": "2024-11-28T12:00:00.000Z",
  "thread": 12345,
  "pathname": "torch/_dynamo/convert_frame.py",
  "lineno": 123,
  "metadata": {
    "stack": {
      "frames": [
        {
          "filename": "model.py",
          "line": 42,
          "name": "forward"
        },
        {
          "filename": "trainer.py",
          "line": 100,
          "name": "train_step"
        }
      ]
    }
  }
}
```

### `by_type/symbolic_shape_specialization.jsonl`
```jsonl
{
  "compile_id": "0_0_0",
  "rank": 0,
  "timestamp": "2024-11-28T12:00:02.000Z",
  "thread": 12345,
  "pathname": "torch/fx/experimental/symbolic_shapes.py",
  "lineno": 234,
  "metadata": {
    "symbol": "s0",
    "sources": ["L['x'].size()[0]"],
    "value": "4",
    "reason": "size is statically known",
    "stacks": [
      {
        "frames": [...]
      }
    ]
  }
}
```

---

## Implementation Considerations

### Performance
- **Streaming writes**: Write JSONL files as we parse, don't buffer everything
- **Parallel file writes**: Use rayon for parallel JSON serialization
- **Lazy loading in modules**: Modules should stream-read JSONL, not load all into memory

### Error Handling
- **Partial failures**: If one envelope type fails, continue with others
- **Validation**: Validate compile_id consistency across files
- **Recovery**: Allow re-running from intermediate files if render fails

### Testing Strategy
- **Round-trip tests**: Parse → intermediate → render should match direct parse → render
- **Schema validation**: JSON schema tests for all intermediate file formats
- **Module isolation tests**: Each module tested independently with fixture intermediate files

---

## Open Questions

1. **Compression**: Should intermediate files be compressed (gzip)?
   - **Recommendation**: Not by default, but support `.jsonl.gz` extension

2. **Incremental Updates**: Should we support appending to intermediate files?
   - **Recommendation**: Not in v1, but design schema to allow it later

3. **External Schema Publishing**: Should we publish JSON schemas to a separate repo?
   - **Recommendation**: Start with in-repo docs, consider later

4. **Python Bindings**: How do Python bindings interact with new architecture?
   - **Recommendation**: Python API exposes both `parse()` and `render()` functions

5. **Backward Compatibility for raw.jsonl**: Should we keep generating `payloads/` directory for raw.jsonl compatibility?
   - **Recommendation**: Yes, raw.jsonl continues to use payload_file references for backward compat, but by_type files use inlined payloads

---

## Success Criteria

1. ✅ All existing tests pass with new architecture
2. ✅ `tlparse <input> -o <output>` produces identical outputs
3. ✅ New module can be added by implementing trait + registering
4. ✅ External tool can consume intermediate JSON files (self-contained with inlined payloads)
5. ✅ Performance regression < 10% for typical workloads
6. ✅ Intermediate file size reasonable (some duplication acceptable for self-containment)

---

## Appendix: Complete Envelope Type Inventory

| # | Envelope Type | Category | Payload | Metadata | Current Parser |
|---|---------------|----------|---------|----------|----------------|
| 1 | dynamo_output_graph | Graph | inlined | ✓ (sizes) | DynamoOutputGraphParser |
| 2 | optimize_ddp_split_graph | Graph | inlined | ✗ | SentinelFileParser |
| 3 | optimize_ddp_split_child | Graph | inlined | ✓ (name) | OptimizeDdpSplitChildParser |
| 4 | compiled_autograd_graph | Graph | inlined | ✗ | SentinelFileParser |
| 5 | aot_forward_graph | Graph | inlined | ✗ | SentinelFileParser |
| 6 | aot_backward_graph | Graph | inlined | ✗ | SentinelFileParser |
| 7 | aot_inference_graph | Graph | inlined | ✗ | SentinelFileParser |
| 8 | aot_joint_graph | Graph | inlined | ✗ | SentinelFileParser |
| 9 | inductor_pre_grad_graph | Graph | inlined | ✗ | SentinelFileParser |
| 10 | inductor_post_grad_graph | Graph | inlined | ✗ | SentinelFileParser |
| 11 | graph_dump | Graph | inlined | ✓ (name) | GraphDumpParser |
| 12 | inductor_output_code | Code | inlined | ✓ (filename) | InductorOutputCodeParser |
| 13 | dynamo_cpp_guards_str | Code | inlined | ✗ | SentinelFileParser |
| 14 | dynamo_guards | Guards | inlined | ✗ | DynamoGuardParser |
| 15 | symbolic_shape_specialization | Guards | ✗ | ✓ | Index collector |
| 16 | guard_added_fast | Guards | ✗ | ✓ | Index collector |
| 17 | compilation_metrics | Metrics | ✗ | ✓ (rich) | CompilationMetricsParser |
| 18 | bwd_compilation_metrics | Metrics | ✗ | ✓ | BwdCompilationMetricsParser |
| 19 | aot_autograd_backward_compilation_metrics | Metrics | ✗ | ✓ | AOTAutogradBackwardCompilationMetricsParser |
| 20 | dynamo_start | Stack | ✗ | ✓ (stack) | Index collector |
| 21 | propagate_real_tensors_provenance | Symbolic | ✗ | ✓ | PropagateRealTensorsParser |
| 22 | guard_added | Symbolic | ✗ | ✓ | PropagateRealTensorsParser |
| 23 | create_unbacked_symbol | Symbolic | ✗ | ✓ | Index collector |
| 24 | expression_created | Symbolic | ✗ | ✓ | Index collector |
| 25 | chromium_event | Events | special | ✗ | Special collector |
| 26 | artifact | Artifact | inlined | ✓ (name, encoding) | ArtifactParser |
| 27 | dump_file | Artifact | inlined | ✓ (name) | DumpFileParser |
| 28 | link | Artifact | ✗ | ✓ (name, url) | LinkParser |
| 29 | describe_tensor | Metadata | ✗ | ✓ | Collector (multi-rank) |
| 30 | describe_storage | Metadata | ✗ | ✓ | Collector (multi-rank) |
| 31 | describe_source | Metadata | ✗ | ✓ | Collector (multi-rank) |
| 32 | missing_fake_kernel | Export | ✗ | ✓ | Export mode collector |
| 33 | mismatched_fake_kernel | Export | ✗ | ✓ | Export mode collector |
| 34 | exported_program | Export | inlined | ✗ | SentinelFileParser |
| 35 | str | Internal | ✗ | ✓ | String table builder |
| 36 | stack (toplevel) | Stack | ✗ | ✓ | Unknown stack trie |

---

## Next Steps

1. **Review this plan** - Get feedback on intermediate file design
2. **Implement Phase 1** - Add intermediate JSON generation
3. **Validate equivalence** - Ensure intermediate → render matches direct render
4. **Implement Phase 2** - Create module system
5. **Migrate parsers** - Convert existing parsers to modules
6. **Document schemas** - Create JSON schema documentation
