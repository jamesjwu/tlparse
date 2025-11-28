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
    ├── graphs.jsonl               # All graph outputs
    ├── codegen.jsonl              # Generated code
    ├── guards.jsonl               # Guards & symbolic shapes
    ├── compilation_metrics.jsonl  # Metrics & stacks
    ├── chromium_events.json       # Trace events
    ├── artifacts.jsonl            # Generic artifacts
    ├── tensor_metadata.jsonl      # Tensor descriptions
    └── export.jsonl               # Export mode data
         ↓
    [Stage 2: Module Rendering]
         ↓
    Final Output Directory:
    ├── index.html
    ├── compile_directory.json
    └── {compile_id}/
        └── ... HTML artifacts
```

**Key Design Decisions:**

1. **Consolidated files (8 instead of 36)**: Related envelope types grouped into single files
2. **Inlined payloads**: Payloads stored directly in JSONL entries (self-contained files)
3. **Type field**: Each entry has a `type` field to identify the original envelope type

---

## Stage 1: Intermediate JSON File Design

### 1.1 Directory Structure

```
intermediate/
├── manifest.json                    # Master index
├── string_table.json                # Filename interning
│
├── graphs.jsonl                     # All graph outputs (dynamo, aot, inductor, etc.)
├── codegen.jsonl                    # Generated code (inductor output, cpp guards)
├── guards.jsonl                     # Guards & symbolic shapes
├── compilation_metrics.jsonl        # Compilation metrics & stacks
├── chromium_events.json             # Trace events (array format for Perfetto)
├── artifacts.jsonl                  # Generic artifacts, dump files, links
├── tensor_metadata.jsonl            # Tensor/storage/source descriptions
└── export.jsonl                     # Export mode failures & output
```

**Consolidated Categories (8 files instead of 36):**

| File | Envelope Types Included |
|------|------------------------|
| `graphs.jsonl` | dynamo_output_graph, optimize_ddp_split_graph, optimize_ddp_split_child, compiled_autograd_graph, aot_forward_graph, aot_backward_graph, aot_inference_graph, aot_joint_graph, inductor_pre_grad_graph, inductor_post_grad_graph, graph_dump |
| `codegen.jsonl` | inductor_output_code, dynamo_cpp_guards_str |
| `guards.jsonl` | dynamo_guards, symbolic_shape_specialization, guard_added_fast, propagate_real_tensors_provenance, guard_added, create_unbacked_symbol, expression_created |
| `compilation_metrics.jsonl` | compilation_metrics, bwd_compilation_metrics, aot_autograd_backward_compilation_metrics, dynamo_start, stack (toplevel) |
| `chromium_events.json` | chromium_event |
| `artifacts.jsonl` | artifact, dump_file, link |
| `tensor_metadata.jsonl` | describe_tensor, describe_storage, describe_source |
| `export.jsonl` | missing_fake_kernel, mismatched_fake_kernel, exported_program |

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
    "chromium_event": 5000
  },
  "compile_ids": ["0_0_0", "0_0_1", "0_1_0"],
  "string_table_entries": 150,
  "parse_mode": "normal",
  "ranks": [0],
  "files": [
    "graphs.jsonl",
    "codegen.jsonl",
    "guards.jsonl",
    "compilation_metrics.jsonl",
    "chromium_events.json",
    "artifacts.jsonl",
    "tensor_metadata.jsonl",
    "export.jsonl"
  ]
}
```

#### JSONL Entry Format
Each entry includes a `type` field to identify the envelope type within the consolidated file:

```jsonl
{"type":"dynamo_output_graph","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00Z","thread":12345,"pathname":"torch/_dynamo/convert_frame.py","lineno":42,"metadata":{"sizes":{"L['x']":[[2,3],[3,4]]}},"payload":"class GraphModule..."}
{"type":"aot_forward_graph","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:01Z",...,"payload":"def forward..."}
```

Fields:
- `type`: The original envelope type name (e.g., "dynamo_output_graph", "aot_forward_graph")
- `compile_id`: String form of CompileId (e.g., "0_0_0" or "!0_0_0" for compiled autograd)
- `rank`: Rank number (for distributed)
- `timestamp`: ISO-8601 timestamp
- `thread`: Thread ID
- `pathname`: Source file
- `lineno`: Line number
- `metadata`: Type-specific metadata object
- `payload`: Inlined payload content (if applicable)

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
    Graphs,              // graphs.jsonl
    Codegen,             // codegen.jsonl
    Guards,              // guards.jsonl
    CompilationMetrics,  // compilation_metrics.jsonl
    ChromiumEvents,      // chromium_events.json
    Artifacts,           // artifacts.jsonl
    TensorMetadata,      // tensor_metadata.jsonl
    Export,              // export.jsonl
    Manifest,            // manifest.json
    StringTable,         // string_table.json
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
| `IndexModule` | manifest, compilation_metrics | index.html |
| `CompileDirectoryModule` | manifest, compilation_metrics | compile_directory.json |
| `GraphViewerModule` | graphs | {id}/*.txt files |
| `CodegenModule` | codegen | {id}/inductor_output_code.html |
| `CompilationMetricsModule` | compilation_metrics, guards | {id}/compilation_metrics.html |
| `FailuresModule` | compilation_metrics | failures_and_restarts.html |
| `GuardsModule` | guards | {id}/dynamo_guards.html, {id}/symbolic_guard_information.html |
| `ChromiumModule` | chromium_events | chromium_events.json (passthrough) |
| `ExportModule` | export, guards | index.html (export mode) |
| `MultiRankModule` | tensor_metadata, (multiple dirs) | index.html (multi-rank), diagnostics |

### 2.3 Module Registration

```rust
/// Default module set for normal mode
pub fn default_modules() -> Vec<Box<dyn OutputModule>> {
    vec![
        Box::new(IndexModule::new()),
        Box::new(CompileDirectoryModule::new()),
        Box::new(GraphViewerModule::new()),
        Box::new(CodegenModule::new()),
        Box::new(CompilationMetricsModule::new()),
        Box::new(FailuresModule::new()),
        Box::new(GuardsModule::new()),
        Box::new(ChromiumModule::new()),
    ]
}

/// Modules for export mode
pub fn export_modules() -> Vec<Box<dyn OutputModule>> {
    vec![
        Box::new(ExportModule::new()),
        Box::new(GuardsModule::new()),  // For symbolic shape info
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

### `graphs.jsonl`
Contains all graph outputs (dynamo, aot, inductor graphs):
```jsonl
{"type":"dynamo_output_graph","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00.000Z","thread":12345,"pathname":"torch/_dynamo/convert_frame.py","lineno":456,"metadata":{"sizes":{"L['x']":[[2,3],[3,4]]}},"payload":"class GraphModule(torch.nn.Module):\n    def forward(self, L_x_):\n        return L_x_ + 1"}
{"type":"aot_forward_graph","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:01.000Z","thread":12345,"pathname":"torch/_functorch/aot_autograd.py","lineno":123,"metadata":{},"payload":"def forward(self, primals_1):\n    add = primals_1 + 1\n    return [add]"}
{"type":"inductor_post_grad_graph","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:02.000Z","thread":12345,"pathname":"torch/_inductor/compile_fx.py","lineno":789,"metadata":{},"payload":"def forward(self, arg0_1):\n    return arg0_1 + 1"}
```

### `compilation_metrics.jsonl`
Contains metrics, stacks, and compilation status:
```jsonl
{"type":"dynamo_start","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00.000Z","thread":12345,"pathname":"torch/_dynamo/convert_frame.py","lineno":123,"metadata":{"stack":{"frames":[{"filename":"model.py","line":42,"name":"forward"}]}}}
{"type":"compilation_metrics","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:05.000Z","thread":12345,"pathname":"torch/_dynamo/convert_frame.py","lineno":789,"metadata":{"co_name":"forward","co_filename":"model.py","co_firstlineno":42,"entire_frame_compile_time_s":1.234,"backend_compile_time_s":0.567,"fail_type":null,"fail_reason":null}}
{"type":"bwd_compilation_metrics","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:06.000Z","thread":12345,"pathname":"torch/_inductor/compile_fx.py","lineno":456,"metadata":{"inductor_compile_time_s":0.234,"code_gen_time_s":0.1}}
```

### `guards.jsonl`
Contains all guard-related entries (dynamo guards, symbolic shapes, etc.):
```jsonl
{"type":"dynamo_guards","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:03.000Z","thread":12345,"pathname":"torch/_dynamo/guards.py","lineno":100,"metadata":{},"payload":"TENSOR_MATCH(L['x'], ...)\nSHAPE_MATCH(...)"}
{"type":"symbolic_shape_specialization","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:02.000Z","thread":12345,"pathname":"torch/fx/experimental/symbolic_shapes.py","lineno":234,"metadata":{"symbol":"s0","sources":["L['x'].size()[0]"],"value":"4","reason":"size is statically known"}}
{"type":"guard_added","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:02.500Z","thread":12345,"pathname":"torch/fx/experimental/symbolic_shapes.py","lineno":300,"metadata":{"expr":"s0 == 4","user_stack":[...]}}
```

### `chromium_events.json`
Array format for Perfetto compatibility:
```json
[
  {"name":"compile","cat":"dynamo","ph":"B","ts":1732795200000000,"pid":1234,"tid":5678},
  {"name":"compile","cat":"dynamo","ph":"E","ts":1732795201000000,"pid":1234,"tid":5678}
]
```

### `codegen.jsonl`
Contains generated code:
```jsonl
{"type":"inductor_output_code","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:04.000Z","thread":12345,"pathname":"torch/_inductor/graph.py","lineno":500,"metadata":{"filename":"output_code.py"},"payload":"# Generated inductor code\nasync_compile = ..."}
{"type":"dynamo_cpp_guards_str","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:03.500Z","thread":12345,"pathname":"torch/_dynamo/guards.py","lineno":200,"metadata":{},"payload":"check_tensor(...)"}
```

### `artifacts.jsonl`
Contains generic artifacts, dump files, and links:
```jsonl
{"type":"artifact","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:07.000Z","thread":12345,"pathname":"torch/_dynamo/output.py","lineno":100,"metadata":{"name":"graph_sizes","encoding":"json"},"payload":"{\"nodes\": 50, \"edges\": 75}"}
{"type":"link","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:08.000Z","thread":12345,"pathname":"torch/_dynamo/output.py","lineno":150,"metadata":{"name":"Detailed Report","url":"https://example.com/report/123"}}
```

### `tensor_metadata.jsonl`
Contains tensor descriptions (used for multi-rank analysis):
```jsonl
{"type":"describe_tensor","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00.000Z","thread":12345,"pathname":"torch/_dynamo/symbolic.py","lineno":50,"metadata":{"id":1,"ndim":2,"dtype":"float32","device":"cuda:0","size":[32,64],"requires_grad":true}}
{"type":"describe_storage","compile_id":"0_0_0","rank":0,"timestamp":"2024-11-28T12:00:00.000Z","thread":12345,"pathname":"torch/_dynamo/symbolic.py","lineno":55,"metadata":{"id":1,"size":8192}}
```

### `export.jsonl`
Contains export mode data:
```jsonl
{"type":"missing_fake_kernel","compile_id":null,"rank":0,"timestamp":"2024-11-28T12:00:00.000Z","thread":12345,"pathname":"torch/export.py","lineno":100,"metadata":{"op":"custom::my_op","reason":"No fake kernel registered"}}
{"type":"exported_program","compile_id":null,"rank":0,"timestamp":"2024-11-28T12:00:10.000Z","thread":12345,"pathname":"torch/export.py","lineno":500,"metadata":{},"payload":"ExportedProgram:\n    ..."}
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

3. **Python Bindings**: How do Python bindings interact with new architecture?
   - **Recommendation**: Python API exposes both `parse()` and `render()` functions

---

## Success Criteria

1. ✅ All existing tests pass with new architecture
2. ✅ `tlparse <input> -o <output>` produces identical outputs
3. ✅ New module can be added by implementing trait + registering
4. ✅ External tool can consume intermediate JSON files (self-contained with inlined payloads)
5. ✅ Performance regression < 10% for typical workloads
6. ✅ Intermediate file size reasonable (some duplication acceptable for self-containment)

---

## Appendix: Envelope Type → File Mapping

| Intermediate File | Envelope Types |
|-------------------|----------------|
| `graphs.jsonl` | dynamo_output_graph, optimize_ddp_split_graph, optimize_ddp_split_child, compiled_autograd_graph, aot_forward_graph, aot_backward_graph, aot_inference_graph, aot_joint_graph, inductor_pre_grad_graph, inductor_post_grad_graph, graph_dump |
| `codegen.jsonl` | inductor_output_code, dynamo_cpp_guards_str |
| `guards.jsonl` | dynamo_guards, symbolic_shape_specialization, guard_added_fast, propagate_real_tensors_provenance, guard_added, create_unbacked_symbol, expression_created |
| `compilation_metrics.jsonl` | compilation_metrics, bwd_compilation_metrics, aot_autograd_backward_compilation_metrics, dynamo_start, stack |
| `chromium_events.json` | chromium_event |
| `artifacts.jsonl` | artifact, dump_file, link |
| `tensor_metadata.jsonl` | describe_tensor, describe_storage, describe_source |
| `export.jsonl` | missing_fake_kernel, mismatched_fake_kernel, exported_program |
| *(not written)* | str (populates string_table.json instead) |

---

## Next Steps

1. **Review this plan** - Get feedback on intermediate file design
2. **Implement Phase 1** - Add intermediate JSON generation
3. **Validate equivalence** - Ensure intermediate → render matches direct render
4. **Implement Phase 2** - Create module system
5. **Migrate parsers** - Convert existing parsers to modules
6. **Document schemas** - Create JSON schema documentation
