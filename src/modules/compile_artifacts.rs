//! CompileArtifactsModule - Handles graphs, generated code, and generic artifacts.
//!
//! This is the main "output files" module that handles most per-compilation outputs:
//! - Graph outputs (dynamo_output_graph, aot graphs, inductor graphs, etc.)
//! - Code generation (inductor_output_code)
//! - Generic artifacts (artifact, dump_file, link)

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, Module, ModuleOutput};

/// Module that generates per-compile artifact files.
pub struct CompileArtifactsModule {
    plain_text: bool,
}

impl CompileArtifactsModule {
    pub fn new(plain_text: bool) -> Self {
        Self { plain_text }
    }
}

impl Default for CompileArtifactsModule {
    fn default() -> Self {
        Self::new(false)
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

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries: HashMap<String, Vec<DirectoryEntry>> = HashMap::new();

        // Process graphs
        self.process_graphs(ctx, &mut files, &mut directory_entries)?;

        // Process codegen (inductor output code)
        self.process_codegen(ctx, &mut files, &mut directory_entries)?;

        // Process generic artifacts
        self.process_artifacts(ctx, &mut files, &mut directory_entries)?;

        Ok(ModuleOutput {
            files,
            directory_entries,
            index_contribution: None,
        })
    }
}

impl CompileArtifactsModule {
    fn process_graphs(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Graphs)? {
            let compile_id = entry.compile_id.clone().unwrap_or_else(|| "unknown".to_string());

            let filename = match entry.entry_type.as_str() {
                "optimize_ddp_split_child" => {
                    let name = entry
                        .metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    format!("optimize_ddp_split_child_{}.txt", name)
                }
                "graph_dump" => {
                    let name = entry
                        .metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("graph_dump");
                    format!("{}.txt", name)
                }
                other => format!("{}.txt", other),
            };

            let path = PathBuf::from(&compile_id).join(&filename);
            let content = entry.payload.unwrap_or_default();
            files.push((path.clone(), content));

            directory_entries
                .entry(compile_id)
                .or_default()
                .push(DirectoryEntry::new(
                    filename,
                    path.to_string_lossy().to_string(),
                ));
        }

        Ok(())
    }

    fn process_codegen(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Codegen)? {
            // Skip dynamo_cpp_guards_str (handled by GuardsModule)
            if entry.entry_type == "dynamo_cpp_guards_str" {
                continue;
            }

            if entry.entry_type != "inductor_output_code" {
                continue;
            }

            let compile_id = entry.compile_id.clone().unwrap_or_else(|| "unknown".to_string());

            // Get filename from metadata if available
            let base_filename = entry
                .metadata
                .get("filename")
                .and_then(|v| v.as_str())
                .and_then(|p| std::path::Path::new(p).file_stem())
                .and_then(|s| s.to_str())
                .map(|s| format!("inductor_output_code_{}", s))
                .unwrap_or_else(|| "inductor_output_code".to_string());

            let payload = entry.payload.unwrap_or_default();

            let (filename, content) = if self.plain_text {
                (format!("{}.txt", base_filename), payload)
            } else {
                // For now, just output as txt since we don't have highlighting wired up
                // Full implementation would use syntect or similar
                (format!("{}.txt", base_filename), payload)
            };

            let path = PathBuf::from(&compile_id).join(&filename);
            files.push((path.clone(), content));

            directory_entries
                .entry(compile_id)
                .or_default()
                .push(DirectoryEntry::new(
                    filename,
                    path.to_string_lossy().to_string(),
                ));
        }

        Ok(())
    }

    fn process_artifacts(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
        directory_entries: &mut HashMap<String, Vec<DirectoryEntry>>,
    ) -> Result<()> {
        for entry in ctx.read_jsonl(IntermediateFileType::Artifacts)? {
            let compile_id = entry.compile_id.clone().unwrap_or_else(|| "unknown".to_string());

            match entry.entry_type.as_str() {
                "artifact" => {
                    let name = entry
                        .metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("artifact");

                    // Skip cache artifacts (handled by CacheModule)
                    if is_cache_artifact(name) {
                        continue;
                    }

                    let encoding = entry
                        .metadata
                        .get("encoding")
                        .and_then(|v| v.as_str())
                        .unwrap_or("string");

                    let payload = entry.payload.unwrap_or_default();
                    let (filename, content) = match encoding {
                        "json" => {
                            // Pretty-print JSON
                            let formatted = serde_json::from_str::<serde_json::Value>(&payload)
                                .map(|v| serde_json::to_string_pretty(&v).unwrap_or(payload.clone()))
                                .unwrap_or(payload);
                            (format!("{}.json", name), formatted)
                        }
                        _ => (format!("{}.txt", name), payload),
                    };

                    let path = PathBuf::from(&compile_id).join(&filename);
                    files.push((path.clone(), content));

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry::new(
                            filename,
                            path.to_string_lossy().to_string(),
                        ));
                }
                "dump_file" => {
                    let name = entry
                        .metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("dump");

                    let filename = format!("{}.html", sanitize_dump_filename(name));
                    let content = anchor_source(&entry.payload.unwrap_or_default());
                    let path = PathBuf::from("dump_file").join(&filename);

                    files.push((path.clone(), content));

                    // dump_file is global (not tied to compile_id)
                    directory_entries
                        .entry("__global__".to_string())
                        .or_default()
                        .push(DirectoryEntry::new(
                            filename,
                            path.to_string_lossy().to_string(),
                        ));
                }
                "link" => {
                    let name = entry
                        .metadata
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Link");
                    let url = entry
                        .metadata
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("#");

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry::new(name, url));
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

/// Sanitize dump filename, handling eval_with_key pattern
fn sanitize_dump_filename(name: &str) -> String {
    // Handle eval_with_key_<id> pattern
    if name.starts_with("eval_with_key_") {
        // Extract just the ID part
        if let Some(id) = name.strip_prefix("eval_with_key_") {
            return format!("eval_with_key_{}", id);
        }
    }
    name.to_string()
}

/// Add line anchors to source code for easy linking
fn anchor_source(source: &str) -> String {
    use html_escape::encode_text;

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
<style>
pre { margin: 0; }
.line { display: block; }
.line:target { background-color: #ffffcc; }
.lineno { color: #999; width: 4em; display: inline-block; text-align: right; margin-right: 1em; }
</style>
</head>
<body>
<pre>"#,
    );

    for (i, line) in source.lines().enumerate() {
        let lineno = i + 1;
        html.push_str(&format!(
            r#"<span class="line" id="L{}"><span class="lineno">{}</span>{}</span>
"#,
            lineno,
            lineno,
            encode_text(line)
        ));
    }

    html.push_str("</pre>\n</body>\n</html>");
    html
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
            files: vec!["graphs.jsonl".to_string()],
        }
    }

    #[test]
    fn test_process_graphs() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create graphs.jsonl
        let graphs_path = temp_dir.path().join("graphs.jsonl");
        let mut file = File::create(&graphs_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"class GraphModule(nn.Module):..."}}"#
        )?;

        let ctx = ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CompileArtifactsModule::new(false);
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(
            output.files[0].0,
            PathBuf::from("0_0/dynamo_output_graph.txt")
        );

        Ok(())
    }

    #[test]
    fn test_is_cache_artifact() {
        assert!(is_cache_artifact("cache_hit_abc123"));
        assert!(is_cache_artifact("cache_miss_def456"));
        assert!(is_cache_artifact("cache_bypass_ghi789"));
        assert!(!is_cache_artifact("dynamo_output_graph"));
        assert!(!is_cache_artifact("some_other_artifact"));
    }

    #[test]
    fn test_anchor_source() {
        let source = "line 1\nline 2";
        let html = anchor_source(source);

        assert!(html.contains("id=\"L1\""));
        assert!(html.contains("id=\"L2\""));
        assert!(html.contains("line 1"));
        assert!(html.contains("line 2"));
    }
}
