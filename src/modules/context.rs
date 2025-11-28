//! Module context for accessing intermediate files.
//!
//! Provides read access to intermediate JSONL files and helper methods
//! for common query patterns.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::intermediate::{IntermediateEntry, IntermediateFileType, IntermediateManifest};
use crate::modules::ModuleConfig;

/// Context provided to modules during rendering.
///
/// Contains paths to intermediate/output directories and provides
/// methods to read intermediate JSONL files.
pub struct ModuleContext<'a> {
    /// Directory containing intermediate JSONL files
    pub intermediate_dir: &'a Path,

    /// Directory where output files should be written
    pub output_dir: &'a Path,

    /// Manifest describing the intermediate files
    pub manifest: &'a IntermediateManifest,

    /// Module configuration
    pub config: &'a ModuleConfig,
}

impl<'a> ModuleContext<'a> {
    /// Create a new module context
    pub fn new(
        intermediate_dir: &'a Path,
        output_dir: &'a Path,
        manifest: &'a IntermediateManifest,
        config: &'a ModuleConfig,
    ) -> Self {
        Self {
            intermediate_dir,
            output_dir,
            manifest,
            config,
        }
    }

    /// Read all entries from a JSONL intermediate file
    pub fn read_jsonl(&self, file_type: IntermediateFileType) -> Result<Vec<IntermediateEntry>> {
        let filename = file_type.filename();
        let path = self.intermediate_dir.join(filename);

        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)
            .with_context(|| format!("Failed to open intermediate file: {}", path.display()))?;
        let reader = BufReader::new(file);

        let mut entries = Vec::new();
        for (line_num, line) in reader.lines().enumerate() {
            let line = line.with_context(|| {
                format!(
                    "Failed to read line {} from {}",
                    line_num + 1,
                    path.display()
                )
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let entry: IntermediateEntry = serde_json::from_str(&line).with_context(|| {
                format!(
                    "Failed to parse JSON at line {} in {}",
                    line_num + 1,
                    path.display()
                )
            })?;

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Read entries filtered by compile_id
    pub fn get_entries_for_compile(
        &self,
        file_type: IntermediateFileType,
        compile_id: &str,
    ) -> Result<Vec<IntermediateEntry>> {
        let entries = self.read_jsonl(file_type)?;
        Ok(entries
            .into_iter()
            .filter(|e| e.compile_id.as_deref() == Some(compile_id))
            .collect())
    }

    /// Read entries filtered by entry type
    pub fn get_entries_by_type(
        &self,
        file_type: IntermediateFileType,
        entry_type: &str,
    ) -> Result<Vec<IntermediateEntry>> {
        let entries = self.read_jsonl(file_type)?;
        Ok(entries
            .into_iter()
            .filter(|e| e.entry_type == entry_type)
            .collect())
    }

    /// Group entries by compile_id
    pub fn group_by_compile_id(
        &self,
        file_type: IntermediateFileType,
    ) -> Result<std::collections::HashMap<Option<String>, Vec<IntermediateEntry>>> {
        let entries = self.read_jsonl(file_type)?;
        let mut grouped: std::collections::HashMap<Option<String>, Vec<IntermediateEntry>> =
            std::collections::HashMap::new();

        for entry in entries {
            grouped
                .entry(entry.compile_id.clone())
                .or_default()
                .push(entry);
        }

        Ok(grouped)
    }

    /// Read chromium events as JSON array
    pub fn read_chromium_events(&self) -> Result<Vec<serde_json::Value>> {
        let path = self
            .intermediate_dir
            .join(IntermediateFileType::ChromiumEvents.filename());

        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&path)
            .with_context(|| format!("Failed to open chromium events file: {}", path.display()))?;

        let events: Vec<serde_json::Value> = serde_json::from_reader(file)
            .with_context(|| format!("Failed to parse chromium events: {}", path.display()))?;

        Ok(events)
    }

    /// Get the list of all compile IDs from manifest
    pub fn compile_ids(&self) -> &[String] {
        &self.manifest.compile_ids
    }

    /// Check if a specific intermediate file type has any entries
    pub fn has_entries(&self, file_type: IntermediateFileType) -> bool {
        self.manifest.files.contains(&file_type.filename().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_manifest() -> IntermediateManifest {
        IntermediateManifest {
            version: "2.0".to_string(),
            generated_at: "2024-01-01T00:00:00Z".to_string(),
            source_file: "test.log".to_string(),
            source_file_hash: None,
            total_envelopes: 2,
            envelope_counts: HashMap::new(),
            compile_ids: vec!["0_0".to_string(), "0_1".to_string()],
            string_table_entries: 0,
            parse_mode: "normal".to_string(),
            ranks: vec![0],
            files: vec!["graphs.jsonl".to_string()],
        }
    }

    #[test]
    fn test_read_jsonl() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create a test JSONL file
        let graphs_path = temp_dir.path().join("graphs.jsonl");
        let mut file = File::create(&graphs_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"graph content"}}"#
        )?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_1","rank":0,"timestamp":"2024-01-01T00:00:01Z","thread":1,"pathname":"test.py","lineno":2,"metadata":{{}},"payload":"graph content 2"}}"#
        )?;

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);

        let entries = ctx.read_jsonl(IntermediateFileType::Graphs)?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].compile_id, Some("0_0".to_string()));
        assert_eq!(entries[1].compile_id, Some("0_1".to_string()));

        Ok(())
    }

    #[test]
    fn test_get_entries_for_compile() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        let graphs_path = temp_dir.path().join("graphs.jsonl");
        let mut file = File::create(&graphs_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"graph1"}}"#
        )?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_1","rank":0,"timestamp":"2024-01-01T00:00:01Z","thread":1,"pathname":"test.py","lineno":2,"metadata":{{}},"payload":"graph2"}}"#
        )?;

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);

        let entries = ctx.get_entries_for_compile(IntermediateFileType::Graphs, "0_0")?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].payload, Some("graph1".to_string()));

        Ok(())
    }

    #[test]
    fn test_missing_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);

        // Reading a non-existent file should return empty vec, not error
        let entries = ctx.read_jsonl(IntermediateFileType::Guards)?;
        assert!(entries.is_empty());

        Ok(())
    }
}
