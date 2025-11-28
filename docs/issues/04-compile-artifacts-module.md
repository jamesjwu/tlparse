# Sub-Issue #4: CompileArtifactsModule Implementation

## Summary
Consolidated module for all per-compile-id file artifacts: graphs, generated code, and generic artifacts. This is the main "output files" module that handles most per-compilation outputs.

## Consolidated From
- GraphViewerModule (original #4)
- CodegenModule (original #5)
- ArtifactsModule (original #10)

Note: Cache-related artifacts (cache_hit_*, cache_miss_*, cache_bypass_*) are handled separately by CacheModule.

## Current Implementation Location
- `parsers.rs`: `SentinelFileParser`, `DynamoOutputGraphParser`, `GraphDumpParser`, `InductorOutputCodeParser`, `ArtifactParser`, `DumpFileParser`, `LinkParser`

## Entry Types Handled

### From `graphs.jsonl`
| Type | Output |
|------|--------|
| dynamo_output_graph | `dynamo_output_graph.txt` |
| aot_forward_graph | `aot_forward_graph.txt` |
| aot_backward_graph | `aot_backward_graph.txt` |
| aot_joint_graph | `aot_joint_graph.txt` |
| aot_inference_graph | `aot_inference_graph.txt` |
| inductor_pre_grad_graph | `inductor_pre_grad_graph.txt` |
| inductor_post_grad_graph | `inductor_post_grad_graph.txt` |
| optimize_ddp_split_graph | `optimize_ddp_split_graph.txt` |
| optimize_ddp_split_child | `optimize_ddp_split_child_{name}.txt` |
| compiled_autograd_graph | `compiled_autograd_graph.txt` |
| graph_dump | `{name}.txt` (dynamic naming) |

### From `codegen.jsonl`
| Type | Output |
|------|--------|
| inductor_output_code | `inductor_output_code.html` (syntax highlighted) or `.txt` (plain) |

### From `artifacts.jsonl`
| Type | Output |
|------|--------|
| artifact (encoding: "string") | `{name}.txt` |
| artifact (encoding: "json") | `{name}.json` (pretty-printed) |
| dump_file | `dump_file/{name}.html` (with line anchors) |
| link | External URL in compile directory |

Note: `dynamo_cpp_guards_str` is handled by GuardsModule.

## Tasks

### 4.1 Create `src/modules/compile_artifacts.rs`
- Implement `CompileArtifactsModule` struct
- Subscribe to `graphs.jsonl`, `codegen.jsonl`, `artifacts.jsonl`

### 4.2 Graph Output
- Output all graph types as text files
- Handle dynamic naming for graph_dump

### 4.3 Code Generation Output
- Syntax highlighting with syntect (eager) or highlight.js (lazy)
- Handle multiple inductor output files per compile
- Support plain text mode

### 4.4 Generic Artifact Output
- String artifacts → .txt files
- JSON artifacts → pretty-printed .json files
- Dump files → HTML with line anchors
- Links → External URLs in directory

### 4.5 Lazy Loading Support
- For lazy mode: generate placeholders, load content on demand
- For eager mode: generate all files upfront

## Module Implementation

```rust
pub struct CompileArtifactsModule {
    plain_text: bool,
}

impl CompileArtifactsModule {
    pub fn new(plain_text: bool) -> Self {
        Self { plain_text }
    }
}

impl Module for CompileArtifactsModule {
    fn name(&self) -> &'static str {
        "Compile Artifacts"
    }

    fn id(&self) -> &'static str {
        "compile_artifacts"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::Graphs,
            IntermediateFileType::Codegen,
            IntermediateFileType::Artifacts,
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Lazy // Most artifacts can be lazy-loaded
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries = HashMap::new();

        // Process graphs
        self.process_graphs(ctx, &mut files, &mut directory_entries)?;

        // Process codegen
        self.process_codegen(ctx, &mut files, &mut directory_entries)?;

        // Process artifacts (excluding cache artifacts)
        self.process_artifacts(ctx, &mut files, &mut directory_entries)?;

        Ok(ModuleOutput {
            files,
            directory_entries,
            lazy_scripts: vec![
                PathBuf::from("compile_artifacts.js"),
            ],
            ..Default::default()
        })
    }
}

impl CompileArtifactsModule {
    fn process_graphs(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> anyhow::Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Graphs)? {
            let compile_id = entry.compile_id.clone().unwrap_or_default();

            let filename = match entry.entry_type.as_str() {
                "optimize_ddp_split_child" => {
                    let name = entry.metadata.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    format!("optimize_ddp_split_child_{}.txt", name)
                }
                "graph_dump" => {
                    let name = entry.metadata.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("graph_dump");
                    format!("{}.txt", name)
                }
                other => format!("{}.txt", other),
            };

            let path = PathBuf::from(&compile_id).join(&filename);
            files.push((path.clone(), entry.payload.unwrap_or_default()));

            directory_entries
                .entry(compile_id)
                .or_default()
                .push(DirectoryEntry {
                    name: filename,
                    url: path.to_string_lossy().to_string(),
                    lazy_loader: Some("loader.renderGraph".to_string()),
                });
        }
        Ok(())
    }

    fn process_codegen(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> anyhow::Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Codegen)? {
            // Skip dynamo_cpp_guards_str (handled by GuardsModule)
            if entry.entry_type == "dynamo_cpp_guards_str" {
                continue;
            }

            if entry.entry_type != "inductor_output_code" {
                continue;
            }

            let compile_id = entry.compile_id.clone().unwrap_or_default();

            let base_filename = entry.metadata.get("filename")
                .and_then(|v| v.as_str())
                .and_then(|p| Path::new(p).file_stem())
                .and_then(|s| s.to_str())
                .map(|s| format!("inductor_output_code_{}", s))
                .unwrap_or_else(|| "inductor_output_code".to_string());

            let (filename, content) = if self.plain_text {
                (format!("{}.txt", base_filename), entry.payload.unwrap_or_default())
            } else {
                let html = highlight_python(&entry.payload.unwrap_or_default())?;
                (format!("{}.html", base_filename), html)
            };

            let path = PathBuf::from(&compile_id).join(&filename);
            files.push((path.clone(), content));

            directory_entries
                .entry(compile_id)
                .or_default()
                .push(DirectoryEntry {
                    name: filename,
                    url: path.to_string_lossy().to_string(),
                    lazy_loader: Some("loader.renderInductorCode".to_string()),
                });
        }
        Ok(())
    }

    fn process_artifacts(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> anyhow::Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Artifacts)? {
            let compile_id = entry.compile_id.clone().unwrap_or_default();

            match entry.entry_type.as_str() {
                "artifact" => {
                    let name = entry.metadata.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("artifact");

                    // Skip cache artifacts (handled by CacheModule)
                    if is_cache_artifact(name) {
                        continue;
                    }

                    let encoding = entry.metadata.get("encoding")
                        .and_then(|v| v.as_str())
                        .unwrap_or("string");

                    let (filename, content) = match encoding {
                        "json" => {
                            let formatted = format_json_pretty(&entry.payload.unwrap_or_default())?;
                            (format!("{}.json", name), formatted)
                        }
                        _ => {
                            (format!("{}.txt", name), entry.payload.unwrap_or_default())
                        }
                    };

                    let path = PathBuf::from(&compile_id).join(&filename);
                    files.push((path.clone(), content));

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry {
                            name: filename,
                            url: path.to_string_lossy().to_string(),
                            lazy_loader: None,
                        });
                }
                "dump_file" => {
                    let name = entry.metadata.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("dump");

                    let filename = sanitize_dump_filename(name);
                    let html = anchor_source(&entry.payload.unwrap_or_default());
                    let path = PathBuf::from("dump_file").join(&filename);
                    files.push((path.clone(), html));

                    // dump_file is global
                    directory_entries
                        .entry("__global__".to_string())
                        .or_default()
                        .push(DirectoryEntry {
                            name: filename,
                            url: path.to_string_lossy().to_string(),
                            lazy_loader: None,
                        });
                }
                "link" => {
                    let name = entry.metadata.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Link");
                    let url = entry.metadata.get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("#");

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry {
                            name: name.to_string(),
                            url: url.to_string(),
                            lazy_loader: None,
                        });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Check if artifact name indicates a cache artifact
fn is_cache_artifact(name: &str) -> bool {
    name.contains("cache_hit") || name.contains("cache_miss") || name.contains("cache_bypass")
}

fn sanitize_dump_filename(name: &str) -> String {
    // Handle eval_with_key_<id> pattern
    if let Some(id) = extract_eval_with_key_id(name) {
        return format!("eval_with_key_{}.html", id);
    }
    format!("{}.html", name)
}
```

## Client-Side Script

```javascript
// compile_artifacts.js
async function renderGraph(containerId, compileId, graphType) {
    const container = document.getElementById(containerId);
    tlparse.showLoading(container);

    try {
        await tlparse.initLoader();
        const entries = await tlparse.loader.getEntriesForCompile('graphs', compileId);
        const graph = entries.find(e => e.type === graphType);

        if (graph && graph.payload) {
            container.innerHTML = `
                <div class="artifact-viewer">
                    <pre class="artifact-content">${tlparse.escapeHtml(graph.payload)}</pre>
                </div>
            `;
        } else {
            container.innerHTML = '<div class="not-found">Not available</div>';
        }
    } catch (error) {
        tlparse.showError(container, error.message);
    }
}

async function renderInductorCode(containerId, compileId) {
    const container = document.getElementById(containerId);
    tlparse.showLoading(container);

    try {
        await tlparse.initLoader();
        const entries = await tlparse.loader.getEntriesForCompile('codegen', compileId);
        const code = entries.find(e => e.type === 'inductor_output_code');

        if (code && code.payload) {
            const pre = document.createElement('pre');
            const codeEl = document.createElement('code');
            codeEl.className = 'language-python';
            codeEl.textContent = code.payload;
            pre.appendChild(codeEl);
            container.innerHTML = '';
            container.appendChild(pre);

            if (window.hljs) {
                hljs.highlightElement(codeEl);
            }
        } else {
            container.innerHTML = '<div class="not-found">Not available</div>';
        }
    } catch (error) {
        tlparse.showError(container, error.message);
    }
}
```

## Acceptance Criteria
- [ ] All graph types render correctly
- [ ] Inductor output code has syntax highlighting
- [ ] Multiple output files per compile handled
- [ ] JSON artifacts pretty-printed
- [ ] Dump files have line anchors
- [ ] External links work
- [ ] Cache artifacts excluded (handled by CacheModule)
- [ ] Lazy loading works

## Estimated Complexity
Medium-High - Consolidates multiple artifact types with different handling.
