# Sub-Issue #3: CompileDirectoryModule Implementation

## Summary
Create a module that generates `compile_directory.json` from intermediate files.

## Current Implementation Location
- `lib.rs`: `directory_to_json()` function
- `lib.rs`: `FxIndexMap<Option<CompileId>, Vec<OutputFile>>` tracking

## Tasks

### 3.1 Create `src/modules/compile_directory.rs`
- Implement `CompileDirectoryModule` struct
- Generate compile_directory.json from manifest + all intermediate files

### 3.2 Aggregate Artifacts by Compile ID
- Scan all intermediate JSONL files
- Group entries by compile_id
- Track which artifact types are available per compile ID

### 3.3 Generate Directory JSON
```json
{
  "0_0_0": {
    "display_name": "0/0",
    "status": "success",
    "artifacts": [
      {"name": "dynamo_output_graph.txt", "type": "graph", "lazy": true},
      {"name": "inductor_output_code.html", "type": "codegen", "lazy": true},
      {"name": "compilation_metrics.html", "type": "metrics", "lazy": true}
    ],
    "links": [
      {"name": "External Report", "url": "https://..."}
    ]
  }
}
```

### 3.4 Support Lazy Module References
- For lazy modules, include metadata about how to load them
- Include loader function names for client-side JS

## Module Implementation

```rust
impl Module for CompileDirectoryModule {
    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::Graphs,
            IntermediateFileType::Codegen,
            IntermediateFileType::Guards,
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::Artifacts,
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager // Directory index must be pre-computed
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut directory = HashMap::new();

        // Scan each intermediate file for compile IDs
        for file_type in self.subscriptions() {
            for entry in ctx.read_jsonl(file_type)? {
                if let Some(compile_id) = &entry.compile_id {
                    let dir_entry = directory.entry(compile_id.clone()).or_default();
                    dir_entry.push(artifact_for_entry(&entry));
                }
            }
        }

        // Generate JSON output
        let json = serde_json::to_string_pretty(&directory)?;
        Ok(ModuleOutput {
            files: vec![(PathBuf::from("compile_directory.json"), json)],
            ..Default::default()
        })
    }
}
```

## Acceptance Criteria
- [ ] compile_directory.json format matches current output
- [ ] All compile IDs are discovered
- [ ] Artifact types are correctly identified
- [ ] Works with lazy module references

## Estimated Complexity
Low-Medium - Straightforward aggregation logic.
