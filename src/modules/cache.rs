//! CacheModule - Handles cache hit/miss/bypass artifacts with status indicators.
//!
//! This module reads cache-related artifacts from cache.jsonl and generates:
//! - Cache artifact files with appropriate status indicators (✅/❌/❓)
//! - Cache summary for the index page

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, IndexContribution, Module, ModuleOutput};

/// Module that handles cache-related artifacts.
pub struct CacheModule;

impl CacheModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CacheModule {
    fn default() -> Self {
        Self::new()
    }
}

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

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries: HashMap<String, Vec<DirectoryEntry>> = HashMap::new();
        let mut cache_summary = CacheSummary::default();

        for entry in ctx.read_jsonl(IntermediateFileType::Cache)? {
            if entry.entry_type != "artifact" {
                continue;
            }

            let compile_id = entry
                .compile_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            let name = entry
                .metadata
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("cache_artifact");

            let encoding = entry
                .metadata
                .get("encoding")
                .and_then(|v| v.as_str())
                .unwrap_or("string");

            // Determine cache status from artifact name
            let status = if name.contains("cache_hit") {
                cache_summary.hits += 1;
                CacheStatus::Hit
            } else if name.contains("cache_miss") {
                cache_summary.misses += 1;
                CacheStatus::Miss
            } else if name.contains("cache_bypass") {
                cache_summary.bypasses += 1;
                CacheStatus::Bypass
            } else {
                CacheStatus::Unknown
            };

            // Generate output file
            let payload = entry.payload.unwrap_or_default();
            let (filename, content) = match encoding {
                "json" => {
                    let formatted = serde_json::from_str::<serde_json::Value>(&payload)
                        .map(|v| serde_json::to_string_pretty(&v).unwrap_or(payload.clone()))
                        .unwrap_or(payload);
                    (format!("{}.json", name), formatted)
                }
                _ => (format!("{}.txt", name), payload),
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
                .push(DirectoryEntry::new(&filename, path.to_string_lossy().to_string()).with_suffix(suffix));
        }

        // Generate cache summary for index
        let index_contribution = if cache_summary.has_data() {
            Some(IndexContribution {
                section: "Cache Status".to_string(),
                html: self.render_cache_summary(&cache_summary),
            })
        } else {
            None
        };

        Ok(ModuleOutput {
            files,
            directory_entries,
            index_contribution,
        })
    }
}

impl CacheModule {
    fn render_cache_summary(&self, summary: &CacheSummary) -> String {
        let total = summary.hits + summary.misses + summary.bypasses;
        format!(
            r#"<div class="cache-summary">
    <span class="cache-stat cache-hit" title="Cache hits">✅ {} hit(s)</span>
    <span class="cache-stat cache-miss" title="Cache misses">❌ {} miss(es)</span>
    <span class="cache-stat cache-bypass" title="Cache bypasses">❓ {} bypass(es)</span>
    <span class="cache-total">({} total)</span>
</div>"#,
            summary.hits, summary.misses, summary.bypasses, total
        )
    }
}

#[derive(Default)]
struct CacheSummary {
    hits: usize,
    misses: usize,
    bypasses: usize,
}

impl CacheSummary {
    fn has_data(&self) -> bool {
        self.hits > 0 || self.misses > 0 || self.bypasses > 0
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CacheStatus {
    Hit,
    Miss,
    Bypass,
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intermediate::IntermediateManifest;
    use crate::modules::ModuleConfig;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_manifest() -> IntermediateManifest {
        IntermediateManifest {
            version: "2.0".to_string(),
            generated_at: "2024-01-01T00:00:00Z".to_string(),
            source_file: "test.log".to_string(),
            source_file_hash: None,
            total_envelopes: 1,
            envelope_counts: std::collections::HashMap::new(),
            compile_ids: vec!["0_0".to_string()],
            string_table_entries: 0,
            parse_mode: "normal".to_string(),
            ranks: vec![0],
            files: vec!["cache.jsonl".to_string()],
        }
    }

    #[test]
    fn test_cache_module_hit() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create cache.jsonl with cache_hit artifact
        let cache_path = temp_dir.path().join("cache.jsonl");
        let mut file = File::create(&cache_path)?;
        writeln!(
            file,
            r#"{{"type":"artifact","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"name":"cache_hit_abc123","encoding":"string"}},"payload":"cache hit data"}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CacheModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert!(output.files[0].0.to_string_lossy().contains("cache_hit"));

        // Check directory entry has hit suffix
        let entries = output.directory_entries.get("0_0").unwrap();
        assert_eq!(entries[0].suffix, "✅");

        // Check index contribution
        assert!(output.index_contribution.is_some());
        let contribution = output.index_contribution.unwrap();
        assert!(contribution.html.contains("1 hit"));

        Ok(())
    }

    #[test]
    fn test_cache_module_miss() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create cache.jsonl with cache_miss artifact
        let cache_path = temp_dir.path().join("cache.jsonl");
        let mut file = File::create(&cache_path)?;
        writeln!(
            file,
            r#"{{"type":"artifact","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"name":"cache_miss_def456","encoding":"string"}},"payload":"cache miss data"}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CacheModule::new();
        let output = module.render(&ctx)?;

        // Check directory entry has miss suffix
        let entries = output.directory_entries.get("0_0").unwrap();
        assert_eq!(entries[0].suffix, "❌");

        Ok(())
    }

    #[test]
    fn test_empty_cache() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create empty cache.jsonl
        let cache_path = temp_dir.path().join("cache.jsonl");
        File::create(&cache_path)?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CacheModule::new();
        let output = module.render(&ctx)?;

        assert!(output.files.is_empty());
        assert!(output.index_contribution.is_none());

        Ok(())
    }
}
