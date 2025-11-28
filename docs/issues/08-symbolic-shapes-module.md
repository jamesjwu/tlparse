# Sub-Issue #8: SymbolicShapesModule Implementation

## Summary
Convert symbolic shape information rendering (propagate_real_tensors, guard_added) into a module.

## Current Implementation Location
- `parsers.rs`: `PropagateRealTensorsParser`
- `templates.rs`: `TEMPLATE_SYMBOLIC_GUARD_INFO`
- `lib.rs`: `sym_expr_info_index` for expression tree building

## Tasks

### 8.1 Create `src/modules/symbolic_shapes.rs`
- Implement `SymbolicShapesModule` struct
- Subscribe to `guards.jsonl`

### 8.2 Handle Multiple Entry Types
- `propagate_real_tensors_provenance`
- `guard_added`
- `symbolic_shape_specialization` (for index building)
- `create_unbacked_symbol`
- `expression_created`

### 8.3 Build Expression Tree
- Reconstruct expression DAG from `expression_created` entries
- Render as hierarchical tree

### 8.4 Render Symbolic Guard Information
- User stack trace
- Framework stack trace
- Expression trie visualization
- Frame locals

## Module Implementation

```rust
pub struct SymbolicShapesModule;

impl Module for SymbolicShapesModule {
    fn name(&self) -> &'static str {
        "Symbolic Shapes"
    }

    fn id(&self) -> &'static str {
        "symbolic_shapes"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::Guards]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Lazy
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        // First pass: build expression info index
        let sym_expr_info_index = self.build_expr_info_index(ctx)?;

        let mut files = Vec::new();
        let mut directory_entries = HashMap::new();
        let mut output_count = 0;

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type != "propagate_real_tensors_provenance"
                && entry.entry_type != "guard_added" {
                continue;
            }

            let compile_id = entry.compile_id.clone().unwrap_or_default();

            let html = self.render_symbolic_guard_info(
                &entry,
                &sym_expr_info_index,
            )?;

            let filename = format!("symbolic_guard_information_{}.html", output_count);
            let path = PathBuf::from(&compile_id).join(&filename);
            files.push((path.clone(), html));

            directory_entries
                .entry(compile_id)
                .or_insert_with(Vec::new)
                .push(DirectoryEntry {
                    name: filename,
                    url: path.to_string_lossy().to_string(),
                    lazy_loader: Some("loader.renderSymbolicGuard".to_string()),
                });

            output_count += 1;
        }

        Ok(ModuleOutput {
            files,
            directory_entries,
            lazy_scripts: vec![PathBuf::from("symbolic_shapes.js")],
            ..Default::default()
        })
    }
}

impl SymbolicShapesModule {
    fn build_expr_info_index(&self, ctx: &ModuleContext) -> anyhow::Result<SymExprInfoIndex> {
        let mut index = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type != "expression_created" {
                continue;
            }

            let id: u64 = entry.metadata.get("id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            index.insert(id, SymExprInfo {
                result: entry.metadata.get("result").and_then(|v| v.as_str()).map(|s| s.to_string()),
                method: entry.metadata.get("method").and_then(|v| v.as_str()).map(|s| s.to_string()),
                arguments: extract_string_array(&entry.metadata, "arguments"),
                argument_ids: extract_u64_array(&entry.metadata, "argument_ids"),
                user_stack: extract_stack(&entry.metadata, "user_stack"),
                stack: extract_stack(&entry.metadata, "stack"),
            });
        }

        Ok(index)
    }

    fn render_symbolic_guard_info(
        &self,
        entry: &IntermediateEntry,
        sym_expr_info_index: &SymExprInfoIndex,
    ) -> anyhow::Result<String> {
        // Extract metadata
        let expr = entry.metadata.get("expr")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let expr_node_id = entry.metadata.get("expr_node_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Render expression trie
        let sym_expr_trie_html = render_sym_expr_trie(
            expr_node_id,
            sym_expr_info_index,
        );

        // Render stacks
        let user_stack_html = format_stack(&extract_stack(&entry.metadata, "user_stack"));
        let framework_stack_html = format_stack(&extract_stack(&entry.metadata, "stack"));
        let locals_html = format_locals(&entry.metadata.get("frame_locals"));

        // Apply template
        render_template("symbolic_guard_information.html", &SymbolicGuardContext {
            expr: expr.to_string(),
            user_stack_html,
            framework_stack_html,
            sym_expr_trie_html,
            locals_html,
        })
    }
}
```

## Expression Tree Visualization

The expression tree shows how symbolic expressions are built:

```
s0 + 1
├── Method: __add__
├── Arguments: [s0, 1]
├── User Stack: ...
└── Children:
    └── s0
        ├── Method: create_symbol
        ├── Arguments: []
        └── User Stack: ...
```

## Intermediate Format Enhancement

Consider pre-computing the expression tree in `guards.jsonl`:
```jsonl
{"type":"guard_added","compile_id":"0_0_0","expr":"s0 + 1 == 4","expr_tree":{...}}
```

## Acceptance Criteria
- [ ] Symbolic guard information renders correctly
- [ ] Expression tree visualization works
- [ ] User and framework stacks display
- [ ] Frame locals included
- [ ] Works with export mode

## Estimated Complexity
High - Complex expression tree building and rendering.
