# Sub-Issue #10: ExportModule Implementation

## Summary
Handle export mode (torch.export) specific output generation.

## Current Implementation Location
- `lib.rs`: Export mode handling with `export_failures` collection
- `templates.rs`: `TEMPLATE_EXPORT_INDEX`
- Handles: missing_fake_kernel, mismatched_fake_kernel, exported_program

## Entry Types

| Type | Purpose |
|------|---------|
| `missing_fake_kernel` | Operator without fake kernel |
| `mismatched_fake_kernel` | Fake kernel output mismatch |
| `exported_program` | Final exported program output |

## Tasks

### 11.1 Create `src/modules/export.rs`
- Implement `ExportModule` struct
- Subscribe to `export.jsonl`

### 11.2 Generate Export Index Page
- List export failures with reasons
- Link to symbolic guard information
- Output exported program if available

### 11.3 Integrate with Symbolic Shapes Module
- Export failures link to guard information
- Coordinate with SymbolicShapesModule for detail rendering

## Module Implementation

```rust
pub struct ExportModule;

impl Module for ExportModule {
    fn name(&self) -> &'static str {
        "Export"
    }

    fn id(&self) -> &'static str {
        "export"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::Export,
            IntermediateFileType::Guards, // For symbolic guard linking
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager // Export mode needs full index
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut export_failures = Vec::new();
        let mut exported_program = None;

        for entry in ctx.read_jsonl(IntermediateFileType::Export)? {
            match entry.entry_type.as_str() {
                "missing_fake_kernel" => {
                    let op = entry.metadata.get("op")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let reason = entry.metadata.get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No fake kernel registered");

                    export_failures.push(ExportFailure {
                        failure_type: "missing_fake_kernel".to_string(),
                        reason: format!("Op: {} - {}", op, reason),
                        additional_info: String::new(),
                    });
                }
                "mismatched_fake_kernel" => {
                    let op = entry.metadata.get("op")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let reason = entry.metadata.get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Output mismatch");

                    export_failures.push(ExportFailure {
                        failure_type: "mismatched_fake_kernel".to_string(),
                        reason: format!("Op: {} - {}", op, reason),
                        additional_info: String::new(),
                    });
                }
                "exported_program" => {
                    exported_program = entry.payload.clone();
                }
                _ => {}
            }
        }

        // Generate export index HTML
        let html = self.render_export_index(&export_failures, &exported_program)?;

        Ok(ModuleOutput {
            files: vec![
                (PathBuf::from("index.html"), html),
            ],
            ..Default::default()
        })
    }
}

impl ExportModule {
    fn render_export_index(
        &self,
        failures: &[ExportFailure],
        exported_program: &Option<String>,
    ) -> anyhow::Result<String> {
        let context = ExportIndexContext {
            css: EXPORT_CSS,
            failures: failures.to_vec(),
            exported_program: exported_program.clone(),
        };
        render_template("export_index.html", &context)
    }
}
```

## Export Index Structure

```html
<!DOCTYPE html>
<html>
<head>
    <title>Export Analysis</title>
    <style>/* EXPORT_CSS */</style>
</head>
<body>
    <h1>Export Analysis</h1>

    {% if failures %}
    <section id="failures">
        <h2>Export Failures</h2>
        <table>
            <thead>
                <tr>
                    <th>Type</th>
                    <th>Reason</th>
                    <th>Details</th>
                </tr>
            </thead>
            <tbody>
                {% for failure in failures %}
                <tr>
                    <td>{{ failure.failure_type }}</td>
                    <td>{{ failure.reason }}</td>
                    <td>{{ failure.additional_info | format_unescaped }}</td>
                </tr>
                {% endfor %}
            </tbody>
        </table>
    </section>
    {% endif %}

    {% if exported_program %}
    <section id="exported-program">
        <h2>Exported Program</h2>
        <pre>{{ exported_program }}</pre>
    </section>
    {% endif %}
</body>
</html>
```

## Registry Integration

```rust
impl ModuleRegistry {
    pub fn export_mode() -> Self {
        Self {
            modules: vec![
                Box::new(ExportModule::new()),
                Box::new(SymbolicShapesModule::new()), // For guard info
            ],
        }
    }
}
```

## Acceptance Criteria
- [ ] Export index page generated
- [ ] Missing fake kernel failures listed
- [ ] Mismatched fake kernel failures listed
- [ ] Exported program displayed
- [ ] Links to symbolic guard information work

## Estimated Complexity
Medium - Specialized mode with different output structure.
