# Sub-Issue #6: CompilationMetricsModule Implementation (includes Failures)

## Summary
Convert compilation metrics HTML generation AND failures_and_restarts.html into a single module with hybrid eager/lazy loading. These are combined because failures are derived from compilation metrics data.

## Current Implementation Location
- `parsers.rs`: `CompilationMetricsParser`, `BwdCompilationMetricsParser`, `AOTAutogradBackwardCompilationMetricsParser`
- `templates.rs`: `TEMPLATE_COMPILATION_METRICS`, `TEMPLATE_BWD_COMPILATION_METRICS`, `TEMPLATE_FAILURES_AND_RESTARTS`
- `lib.rs`: `breaks` (RestartsAndFailuresContext) tracking
- `types.rs`: `CompilationMetricsMetadata`, `BwdCompilationMetricsMetadata`

## Tasks

### 6.1 Create `src/modules/compilation_metrics.rs`
- Implement `CompilationMetricsModule` struct
- Subscribe to `compilation_metrics.jsonl`

### 6.2 Handle Multiple Metric Types
- `compilation_metrics` - Forward compilation metrics (includes fail_type/fail_reason)
- `bwd_compilation_metrics` - Backward compilation metrics
- `aot_autograd_backward_compilation_metrics` - AOT autograd backward metrics

### 6.3 Generate failures_and_restarts.html
- Extract failure information from `compilation_metrics` entries
- Track compile IDs with `fail_type` set
- Render failures table with links to detailed metrics

### 6.4 Integrate with Stack/Guard Data
- Pull symbolic shape specializations from `guards.jsonl`
- Include stack traces from associated `dynamo_start`
- Render guard_added_fast entries

### 6.5 Hybrid Loading Strategy
- **Eager**: Include summary in compile directory (status, timing), failures summary on index
- **Lazy**: Full metrics HTML and detailed failures list loaded on demand

## Module Implementation

```rust
pub struct CompilationMetricsModule;

impl Module for CompilationMetricsModule {
    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::Guards, // For symbolic shape info
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Hybrid
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        // Build indexes
        let stack_index = self.build_stack_index(ctx)?;
        let specialization_index = self.build_specialization_index(ctx)?;
        let guard_fast_index = self.build_guard_fast_index(ctx)?;

        let mut files = Vec::new();
        let mut directory_entries = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::CompilationMetrics)? {
            match entry.entry_type.as_str() {
                "compilation_metrics" => {
                    let html = self.render_compilation_metrics(
                        &entry,
                        &stack_index,
                        &specialization_index,
                        &guard_fast_index,
                    )?;

                    let compile_id = entry.compile_id.clone().unwrap_or_default();
                    let path = PathBuf::from(&compile_id).join("compilation_metrics.html");
                    files.push((path.clone(), html));

                    directory_entries
                        .entry(compile_id)
                        .or_insert_with(Vec::new)
                        .push(DirectoryEntry {
                            name: "compilation_metrics.html".to_string(),
                            url: path.to_string_lossy().to_string(),
                            lazy_loader: Some("loader.renderMetrics".to_string()),
                        });
                }
                "bwd_compilation_metrics" => {
                    // Similar handling
                }
                "aot_autograd_backward_compilation_metrics" => {
                    // Similar handling
                }
                _ => {}
            }
        }

        // Also generate failures_and_restarts.html
        let failures_html = self.render_failures_html(&failures)?;
        files.push((PathBuf::from("failures_and_restarts.html"), failures_html));

        // Add failures summary to index
        let failures_summary = self.generate_failures_summary(&failures);

        Ok(ModuleOutput {
            files,
            directory_entries,
            index_entries: vec![
                IndexEntry {
                    section: IndexSection::Diagnostics,
                    title: "Failures and Restarts".to_string(),
                    content: IndexContent::Hybrid {
                        summary_html: failures_summary,
                        detail_url: "failures_and_restarts.html".to_string(),
                    },
                },
            ],
            ..Default::default()
        })
    }
}

impl CompilationMetricsModule {
    fn render_failures_html(&self, failures: &[FailureEntry]) -> anyhow::Result<String> {
        // ... render failures table
        todo!()
    }

    fn generate_failures_summary(&self, failures: &[FailureEntry]) -> String {
        if failures.is_empty() {
            return "<span class=\"success\">No failures</span>".to_string();
        }
        format!("<span class=\"warning\">{} failure(s)</span>", failures.len())
    }

impl CompilationMetricsModule {
    fn build_stack_index(&self, ctx: &ModuleContext) -> anyhow::Result<StackIndex> {
        let mut index = HashMap::new();
        for entry in ctx.read_jsonl(IntermediateFileType::CompilationMetrics)? {
            if entry.entry_type == "dynamo_start" {
                if let Some(stack) = entry.metadata.get("stack") {
                    let compile_id = entry.compile_id.clone();
                    let stack: StackSummary = serde_json::from_value(stack.clone())?;
                    index.insert(compile_id, stack);
                }
            }
        }
        Ok(index)
    }

    fn build_specialization_index(&self, ctx: &ModuleContext) -> anyhow::Result<SpecializationIndex> {
        let mut index = HashMap::new();
        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type == "symbolic_shape_specialization" {
                let compile_id = entry.compile_id.clone();
                index.entry(compile_id).or_insert_with(Vec::new).push(entry);
            }
        }
        Ok(index)
    }
}
```

## Data Dependencies

```
compilation_metrics.jsonl
  ├── type: "dynamo_start" → stack traces
  ├── type: "compilation_metrics" → timing, status
  ├── type: "bwd_compilation_metrics" → backward timing
  └── type: "aot_autograd_backward_compilation_metrics"

guards.jsonl
  ├── type: "symbolic_shape_specialization" → specializations
  └── type: "guard_added_fast" → fast guard additions
```

## Eager Summary Output

For the compile directory, include a summary:
```json
{
  "0_0_0": {
    "metrics_summary": {
      "status": "success",
      "total_time_ms": 1234,
      "backend_time_ms": 567,
      "cache_status": "miss"
    }
  }
}
```

## Failure Data Structure

```rust
struct FailureEntry {
    compile_id: String,
    fail_type: String,
    fail_reason: Option<String>,
    co_name: Option<String>,
    co_filename: Option<String>,
}
```

Common failure types:
- `graph_break` - Graph break due to unsupported operation
- `guard_failure` - Guard check failed
- `compile_timeout` - Compilation timed out
- `backend_error` - Backend (inductor) error

## Index Page Integration

Show failures summary in diagnostics section:
```html
<section id="diagnostics">
    <h2>Diagnostics</h2>
    <div class="diagnostic-item">
        <span class="label">Compilation Failures:</span>
        <span class="warning">3 failure(s)</span>
        <a href="failures_and_restarts.html">View Details</a>
    </div>
</section>
```

## Acceptance Criteria
- [ ] All three metric types render correctly
- [ ] Stack traces display properly
- [ ] Symbolic shape specializations included
- [ ] Guard additions included
- [ ] failures_and_restarts.html generated correctly
- [ ] Failures summary shows on index page
- [ ] Hybrid loading works (summary eager, details lazy)

## Estimated Complexity
High - Multiple data sources, failures tracking, and complex rendering logic.
