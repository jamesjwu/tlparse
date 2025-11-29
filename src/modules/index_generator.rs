//! IndexGeneratorModule - Generates the final index.html from combined module outputs.
//!
//! This module is a special "meta-module" that runs after all other modules
//! and combines their outputs into the final index.html.

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

use crate::modules::context::ModuleContext;
use crate::modules::{CombinedOutput, DirectoryEntry, IndexContribution, Module, ModuleOutput};
use crate::templates::{CSS, JAVASCRIPT, TEMPLATE_INDEX, TEMPLATE_QUERY_PARAM_SCRIPT};
use crate::types::OutputFile;
use crate::intermediate::IntermediateFileType;

/// Module that generates the final index.html from combined module outputs.
pub struct IndexGeneratorModule {
    custom_header_html: String,
    has_inductor_provenance: bool,
}

impl IndexGeneratorModule {
    pub fn new(custom_header_html: String, has_inductor_provenance: bool) -> Self {
        Self {
            custom_header_html,
            has_inductor_provenance,
        }
    }

    /// Generate index.html from the combined output of all other modules.
    pub fn generate_index(
        &self,
        combined: &CombinedOutput,
        stack_trie_html: String,
        unknown_stack_trie_html: String,
        num_breaks: usize,
        has_chromium_events: bool,
    ) -> Result<String> {
        let mut tt = TinyTemplate::new();
        tt.add_formatter("format_unescaped", tinytemplate::format_unescaped);
        tt.add_template("index.html", TEMPLATE_INDEX)?;

        // Convert CombinedOutput directory entries to the format expected by template
        let directory = self.build_directory(&combined.directory_entries);
        let directory_names: Vec<String> = directory.iter().map(|(k, _)| k.clone()).collect();

        let context = IndexContext {
            css: CSS,
            javascript: JAVASCRIPT,
            directory,
            stack_trie_html,
            unknown_stack_trie_html: unknown_stack_trie_html.clone(),
            has_unknown_stack_trie: !unknown_stack_trie_html.is_empty(),
            num_breaks,
            custom_header_html: self.custom_header_html.clone(),
            has_chromium_events,
            qps: TEMPLATE_QUERY_PARAM_SCRIPT,
            has_inductor_provenance: self.has_inductor_provenance,
            directory_names,
        };

        tt.render("index.html", &context).map_err(|e| e.into())
    }

    fn build_directory(
        &self,
        entries: &HashMap<String, Vec<DirectoryEntry>>,
    ) -> Vec<(String, Vec<OutputFile>)> {
        let mut result: Vec<(String, Vec<OutputFile>)> = entries
            .iter()
            .filter(|(k, _)| *k != "__global__")
            .map(|(compile_id, dir_entries)| {
                let output_files: Vec<OutputFile> = dir_entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| OutputFile {
                        url: entry.url.clone(),
                        name: entry.name.clone(),
                        number: i as i32,
                        suffix: entry.suffix.clone(),
                        readable_url: None,
                    })
                    .collect();
                (compile_id.clone(), output_files)
            })
            .collect();

        // Sort by compile_id for consistent ordering
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }
}

impl Module for IndexGeneratorModule {
    fn name(&self) -> &'static str {
        "Index Generator"
    }

    fn id(&self) -> &'static str {
        "index_generator"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        // This module doesn't directly subscribe to files - it uses combined output
        &[]
    }

    fn render(&self, _ctx: &ModuleContext) -> Result<ModuleOutput> {
        // This module's main work is done via generate_index(), not render()
        Ok(ModuleOutput::default())
    }
}

/// Context for index.html template rendering
#[derive(Debug, serde::Serialize)]
struct IndexContext {
    css: &'static str,
    javascript: &'static str,
    directory: Vec<(String, Vec<OutputFile>)>,
    stack_trie_html: String,
    unknown_stack_trie_html: String,
    has_unknown_stack_trie: bool,
    num_breaks: usize,
    custom_header_html: String,
    has_chromium_events: bool,
    qps: &'static str,
    has_inductor_provenance: bool,
    directory_names: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_directory() {
        let module = IndexGeneratorModule::new(String::new(), false);

        let mut entries = HashMap::new();
        entries.insert(
            "0_0".to_string(),
            vec![
                DirectoryEntry::new("test.txt", "0_0/test.txt"),
                DirectoryEntry::new("graph.txt", "0_0/graph.txt").with_suffix("✅"),
            ],
        );

        let directory = module.build_directory(&entries);
        assert_eq!(directory.len(), 1);
        assert_eq!(directory[0].0, "0_0");
        assert_eq!(directory[0].1.len(), 2);
        assert_eq!(directory[0].1[1].suffix, "✅");
    }

    #[test]
    fn test_generate_index() -> Result<()> {
        let module = IndexGeneratorModule::new(String::new(), false);
        let combined = CombinedOutput::default();

        let html = module.generate_index(
            &combined,
            "<div>stack trie</div>".to_string(),
            String::new(),
            0,
            false,
        )?;

        assert!(html.contains("stack trie"));
        assert!(html.contains("IR dumps"));
        Ok(())
    }
}
