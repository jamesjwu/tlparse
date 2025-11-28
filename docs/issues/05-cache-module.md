# Sub-Issue #5: CacheModule Implementation

## Summary
Handle cache-related artifacts with special UI indicators for hit/miss/bypass status.

## Background

PyTorch compilation has a caching system. When a compilation is cached, artifacts are generated with special naming patterns:
- `cache_hit_*` - Cache hit, compilation was skipped
- `cache_miss_*` - Cache miss, compilation was performed
- `cache_bypass_*` - Cache was bypassed (e.g., due to dynamic shapes)

These artifacts should be visually distinguished in the UI with status indicators.

## Current Implementation Location
- `lib.rs`: Special handling in `add_file_output()` that adds emoji suffixes (✅/❌/❓)

## Intermediate File Changes

Add a new `cache.jsonl` intermediate file to separate cache artifacts from regular artifacts.

### Update `intermediate.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntermediateFileType {
    Graphs,
    Codegen,
    Guards,
    CompilationMetrics,
    ChromiumEvents,
    Artifacts,
    TensorMetadata,
    Export,
    Cache,  // NEW
}

impl IntermediateFileType {
    pub fn filename(&self) -> &'static str {
        match self {
            // ... existing ...
            IntermediateFileType::Cache => "cache.jsonl",
        }
    }
}

pub fn envelope_type_to_file(envelope_type: &str) -> Option<IntermediateFileType> {
    match envelope_type {
        // ... existing ...
        // Note: cache artifacts are detected by name pattern in artifact entries
        _ => None,
    }
}
```

### Routing Logic

During intermediate file generation, when processing `artifact` entries:

```rust
fn route_artifact(entry: &IntermediateEntry) -> IntermediateFileType {
    let name = entry.metadata.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if name.contains("cache_hit") || name.contains("cache_miss") || name.contains("cache_bypass") {
        IntermediateFileType::Cache
    } else {
        IntermediateFileType::Artifacts
    }
}
```

## Module Implementation

```rust
pub struct CacheModule;

impl Module for CacheModule {
    fn name(&self) -> &'static str {
        "Cache"
    }

    fn id(&self) -> &'static str {
        "cache"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::Cache]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Hybrid // Summary eager, details lazy
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries = HashMap::new();
        let mut cache_summary = CacheSummary::default();

        for entry in ctx.read_jsonl(IntermediateFileType::Cache)? {
            let compile_id = entry.compile_id.clone().unwrap_or_default();

            let name = entry.metadata.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("cache_artifact");

            let encoding = entry.metadata.get("encoding")
                .and_then(|v| v.as_str())
                .unwrap_or("string");

            // Determine cache status
            let status = if name.contains("cache_hit") {
                CacheStatus::Hit
            } else if name.contains("cache_miss") {
                CacheStatus::Miss
            } else if name.contains("cache_bypass") {
                CacheStatus::Bypass
            } else {
                CacheStatus::Unknown
            };

            // Track summary
            match status {
                CacheStatus::Hit => cache_summary.hits += 1,
                CacheStatus::Miss => cache_summary.misses += 1,
                CacheStatus::Bypass => cache_summary.bypasses += 1,
                CacheStatus::Unknown => {}
            }

            // Generate output file
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

            // Add to directory with status indicator
            let suffix = match status {
                CacheStatus::Hit => "✅",
                CacheStatus::Miss => "❌",
                CacheStatus::Bypass => "❓",
                CacheStatus::Unknown => "",
            };

            directory_entries
                .entry(compile_id)
                .or_default()
                .push(DirectoryEntry {
                    name: filename,
                    url: path.to_string_lossy().to_string(),
                    suffix: suffix.to_string(),
                    cache_status: Some(status),
                    lazy_loader: None,
                });
        }

        // Generate cache summary for index
        let summary_html = self.render_cache_summary(&cache_summary);

        Ok(ModuleOutput {
            files,
            directory_entries,
            index_entries: vec![
                IndexEntry {
                    section: IndexSection::Diagnostics,
                    title: "Cache Status".to_string(),
                    content: IndexContent::Html(summary_html),
                },
            ],
            ..Default::default()
        })
    }
}

#[derive(Default)]
struct CacheSummary {
    hits: usize,
    misses: usize,
    bypasses: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum CacheStatus {
    Hit,
    Miss,
    Bypass,
    Unknown,
}

impl CacheModule {
    fn render_cache_summary(&self, summary: &CacheSummary) -> String {
        let total = summary.hits + summary.misses + summary.bypasses;
        if total == 0 {
            return "<span class=\"muted\">No cache data</span>".to_string();
        }

        format!(
            r#"<span class="cache-summary">
                <span class="cache-hit" title="Cache hits">✅ {}</span>
                <span class="cache-miss" title="Cache misses">❌ {}</span>
                <span class="cache-bypass" title="Cache bypasses">❓ {}</span>
            </span>"#,
            summary.hits,
            summary.misses,
            summary.bypasses
        )
    }
}
```

## Directory Entry Extension

Update `DirectoryEntry` to support cache status:

```rust
pub struct DirectoryEntry {
    pub name: String,
    pub url: String,
    pub lazy_loader: Option<String>,
    pub suffix: String,  // NEW: For emoji indicators
    pub cache_status: Option<CacheStatus>,  // NEW
}
```

## CSS for Cache Indicators

```css
.cache-summary {
    display: flex;
    gap: 12px;
}

.cache-hit {
    color: #28a745;
}

.cache-miss {
    color: #dc3545;
}

.cache-bypass {
    color: #ffc107;
}

.artifact-row .suffix {
    margin-left: 8px;
}
```

## UI Display

In compile directory:
```
0/0 - forward (✅ cache hit)
├── cache_hit_abc123.json ✅
├── dynamo_output_graph.txt
└── inductor_output_code.html

0/1 - backward (❌ cache miss)
├── cache_miss_def456.json ❌
├── dynamo_output_graph.txt
└── inductor_output_code.html
```

## Acceptance Criteria
- [ ] Cache artifacts separated from regular artifacts
- [ ] Cache status correctly detected (hit/miss/bypass)
- [ ] Status indicators (✅/❌/❓) displayed
- [ ] Cache summary on index page
- [ ] Intermediate file routing works

## Intermediate File Changes Required
- [ ] Add `Cache` variant to `IntermediateFileType`
- [ ] Add `cache.jsonl` filename
- [ ] Update artifact routing in intermediate generation

## Estimated Complexity
Medium - Requires intermediate file changes plus new module.
