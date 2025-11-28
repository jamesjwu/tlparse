# Sub-Issue #9: ChromiumTraceModule Implementation

## Summary
Handle chromium trace events for Perfetto visualization.

## Current Implementation Location
- `lib.rs`: Direct collection of `chromium_event` entries into `chromium_events.json`
- No rendering, just pass-through aggregation

## Tasks

### 9.1 Create `src/modules/chromium_trace.rs`
- Implement `ChromiumTraceModule` struct
- Subscribe to `chromium_events.json` (already array format)

### 9.2 Pass-Through Output
- Copy chromium_events.json to output directory
- No transformation needed (Perfetto compatibility)

### 9.3 Index Page Link
- Add download link on index page
- Link to Perfetto UI for viewing

## Module Implementation

```rust
pub struct ChromiumTraceModule;

impl Module for ChromiumTraceModule {
    fn name(&self) -> &'static str {
        "Chromium Trace"
    }

    fn id(&self) -> &'static str {
        "chromium_trace"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::ChromiumEvents]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager // Just file copy, very fast
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        // Read chromium_events.json and pass through
        let events_path = ctx.intermediate_dir.join("chromium_events.json");
        let events = std::fs::read_to_string(&events_path)?;

        Ok(ModuleOutput {
            files: vec![
                (PathBuf::from("chromium_events.json"), events),
            ],
            index_entries: vec![
                IndexEntry {
                    section: IndexSection::Downloads,
                    title: "Chromium Trace".to_string(),
                    content: IndexContent::Link("chromium_events.json".to_string()),
                },
            ],
            ..Default::default()
        })
    }
}
```

## Perfetto Integration

Add link to open in Perfetto:
```html
<a href="https://ui.perfetto.dev/#!/viewer?url=<encoded-url-to-chromium_events.json>"
   target="_blank">
    Open in Perfetto
</a>
```

Note: For local files, users would need to manually upload to Perfetto UI.

## Acceptance Criteria
- [ ] chromium_events.json copied to output
- [ ] Download link on index page
- [ ] Format compatible with Perfetto

## Estimated Complexity
Low - Simple pass-through module.
