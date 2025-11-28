//! ChromiumTraceModule - Generates chromium trace viewer JSON file.
//!
//! This is a simple module that reads chromium events from the intermediate
//! file and outputs them as a JSON file that can be loaded in chrome://tracing.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, IndexContribution, Module, ModuleOutput};

/// Module that generates chromium trace viewer JSON.
pub struct ChromiumTraceModule;

impl ChromiumTraceModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ChromiumTraceModule {
    fn default() -> Self {
        Self::new()
    }
}

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

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let events = ctx.read_chromium_events()?;

        if events.is_empty() {
            return Ok(ModuleOutput::default());
        }

        // Write out the chromium events JSON
        let content = serde_json::to_string_pretty(&events)?;
        let path = PathBuf::from("chromium_events.json");

        // Add index contribution with link to trace viewer
        let index_html = format!(
            r#"<div class="chromium-trace">
                <a href="chromium_events.json" target="_blank">View Chromium Trace</a>
                <span class="hint">(Open in chrome://tracing)</span>
            </div>"#
        );

        Ok(ModuleOutput {
            files: vec![(path.clone(), content)],
            directory_entries: {
                let mut entries = HashMap::new();
                entries.insert(
                    "__global__".to_string(),
                    vec![DirectoryEntry::new(
                        "chromium_events.json",
                        "chromium_events.json",
                    )],
                );
                entries
            },
            index_contribution: Some(IndexContribution {
                section: "Chromium Trace".to_string(),
                html: index_html,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::IntermediateManifest;
    use crate::modules::ModuleConfig;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_manifest(files: Vec<String>) -> IntermediateManifest {
        IntermediateManifest {
            version: "2.0".to_string(),
            generated_at: "2024-01-01T00:00:00Z".to_string(),
            source_file: "test.log".to_string(),
            source_file_hash: None,
            total_envelopes: 1,
            envelope_counts: std::collections::HashMap::new(),
            compile_ids: vec![],
            string_table_entries: 0,
            parse_mode: "normal".to_string(),
            ranks: vec![0],
            files,
        }
    }

    #[test]
    fn test_chromium_trace_module_with_events() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest(vec!["chromium_events.json".to_string()]);
        let config = ModuleConfig::default();

        // Create chromium events file
        let events_path = temp_dir.path().join("chromium_events.json");
        let mut file = File::create(&events_path)?;
        let events = serde_json::json!([
            {"name": "compile", "ph": "B", "ts": 1000, "pid": 1, "tid": 1},
            {"name": "compile", "ph": "E", "ts": 2000, "pid": 1, "tid": 1}
        ]);
        write!(file, "{}", serde_json::to_string(&events)?)?;

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = ChromiumTraceModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(output.files[0].0, PathBuf::from("chromium_events.json"));
        assert!(output.index_contribution.is_some());

        Ok(())
    }

    #[test]
    fn test_chromium_trace_module_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest(vec![]);
        let config = ModuleConfig::default();

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = ChromiumTraceModule::new();
        let output = module.render(&ctx)?;

        // Should produce no output when there are no events
        assert!(output.files.is_empty());
        assert!(output.index_contribution.is_none());

        Ok(())
    }
}
