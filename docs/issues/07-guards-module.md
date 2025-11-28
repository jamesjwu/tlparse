# Sub-Issue #7: GuardsModule Implementation

## Summary
Convert guard-related output generation into a lazy-loadable module.

## Current Implementation Location
- `parsers.rs`: `DynamoGuardParser`, `SentinelFileParser` (for dynamo_cpp_guards_str)
- `templates.rs`: `TEMPLATE_DYNAMO_GUARDS`
- `types.rs`: `DynamoGuard` struct

## Entry Types Handled
- `dynamo_guards` - JSON array of guard objects → `dynamo_guards.html`
- `dynamo_cpp_guards_str` - C++ guard string → `dynamo_cpp_guards_str.txt`

## Tasks

### 7.1 Create `src/modules/guards.rs`
- Implement `GuardsModule` struct
- Subscribe to `guards.jsonl` (includes dynamo_guards)
- Subscribe to `codegen.jsonl` (includes dynamo_cpp_guards_str)

### 7.2 Handle dynamo_guards Entries
- Parse JSON array of guard objects
- Render as HTML table with guard details

### 7.3 Handle dynamo_cpp_guards_str Entries
- Output as plain text file
- C++ code for guard checks

### 7.4 Lazy Loading
- Guards can be loaded on-demand
- Include filter/search functionality

## Module Implementation

```rust
pub struct GuardsModule;

impl Module for GuardsModule {
    fn name(&self) -> &'static str {
        "Dynamo Guards"
    }

    fn id(&self) -> &'static str {
        "guards"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::Guards,
            IntermediateFileType::Codegen, // For dynamo_cpp_guards_str
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Lazy
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries = HashMap::new();

        // Handle dynamo_guards from guards.jsonl
        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type != "dynamo_guards" {
                continue;
            }

            let compile_id = entry.compile_id.clone().unwrap_or_default();

            // Parse guards from payload
            let guards: Vec<DynamoGuard> = serde_json::from_str(
                &entry.payload.unwrap_or_default()
            )?;

            let html = self.render_guards_html(&guards)?;
            let path = PathBuf::from(&compile_id).join("dynamo_guards.html");
            files.push((path.clone(), html));

            directory_entries
                .entry(compile_id)
                .or_insert_with(Vec::new)
                .push(DirectoryEntry {
                    name: "dynamo_guards.html".to_string(),
                    url: path.to_string_lossy().to_string(),
                    lazy_loader: Some("loader.renderGuards".to_string()),
                });
        }

        // Handle dynamo_cpp_guards_str from codegen.jsonl
        for entry in ctx.read_jsonl(IntermediateFileType::Codegen)? {
            if entry.entry_type != "dynamo_cpp_guards_str" {
                continue;
            }

            let compile_id = entry.compile_id.clone().unwrap_or_default();
            let path = PathBuf::from(&compile_id).join("dynamo_cpp_guards_str.txt");
            files.push((path.clone(), entry.payload.unwrap_or_default()));

            directory_entries
                .entry(compile_id)
                .or_insert_with(Vec::new)
                .push(DirectoryEntry {
                    name: "dynamo_cpp_guards_str.txt".to_string(),
                    url: path.to_string_lossy().to_string(),
                    lazy_loader: None, // Plain text, no special rendering
                });
        }

        Ok(ModuleOutput {
            files,
            directory_entries,
            lazy_scripts: vec![PathBuf::from("guards.js")],
            ..Default::default()
        })
    }
}
```

## Client-Side Implementation

```javascript
// guards.js
async function renderGuards(containerId, compileId) {
    const container = document.getElementById(containerId);
    container.innerHTML = '<div class="loading">Loading guards...</div>';

    const guards = await loader.loadCompileData('guards', compileId);
    const guardsEntry = guards.find(e => e.type === 'dynamo_guards');

    if (!guardsEntry || !guardsEntry.payload) {
        container.innerHTML = '<div class="not-found">No guards available</div>';
        return;
    }

    const guardsList = JSON.parse(guardsEntry.payload);

    let html = `
        <div class="guards-filter">
            <input type="text" id="guard-search-${compileId}"
                   placeholder="Filter guards..."
                   oninput="filterGuards('${compileId}')">
        </div>
        <table class="guards-table" id="guards-table-${compileId}">
            <thead>
                <tr>
                    <th>Code</th>
                    <th>Type</th>
                    <th>Guard Types</th>
                </tr>
            </thead>
            <tbody>
    `;

    for (const guard of guardsList) {
        html += `
            <tr class="guard-row" data-searchable="${escapeHtml(JSON.stringify(guard))}">
                <td><pre>${escapeHtml(guard.code || '')}</pre></td>
                <td>${escapeHtml(guard.type || '')}</td>
                <td>${escapeHtml((guard.guard_types || []).join(', '))}</td>
            </tr>
        `;
    }

    html += '</tbody></table>';
    container.innerHTML = html;
}

function filterGuards(compileId) {
    const searchInput = document.getElementById(`guard-search-${compileId}`);
    const table = document.getElementById(`guards-table-${compileId}`);
    const filter = searchInput.value.toLowerCase();
    const rows = table.querySelectorAll('.guard-row');

    rows.forEach(row => {
        const searchable = row.dataset.searchable.toLowerCase();
        row.style.display = searchable.includes(filter) ? '' : 'none';
    });
}
```

## Guard Data Structure

```rust
#[derive(Deserialize)]
pub struct DynamoGuard {
    pub code: Option<String>,
    #[serde(rename = "type")]
    pub guard_type: Option<String>,
    pub guard_types: Option<Vec<String>>,
    // ... other fields
}
```

## Acceptance Criteria
- [ ] Guards render as searchable/filterable table
- [ ] Lazy loading works
- [ ] Handles empty guard lists
- [ ] Client-side filtering is responsive

## Estimated Complexity
Low-Medium - Straightforward data rendering.
