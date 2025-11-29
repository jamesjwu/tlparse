//! CompileDirectoryModule - Generates compile_directory.json from intermediate files.
//!
//! This module scans all intermediate JSONL files and aggregates entries by compile_id
//! to generate a comprehensive directory of available artifacts.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{Module, ModuleOutput};

/// Module that generates the compile directory JSON.
pub struct CompileDirectoryModule;

impl CompileDirectoryModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CompileDirectoryModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for CompileDirectoryModule {
    fn name(&self) -> &'static str {
        "Compile Directory"
    }

    fn id(&self) -> &'static str {
        "compile_directory"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::CompileArtifacts,
            IntermediateFileType::Guards,
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::Stacks,
            IntermediateFileType::Cache,
        ]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut directory: HashMap<String, CompileDirectoryEntry> = HashMap::new();

        // Process each intermediate file type and aggregate by compile_id
        for file_type in self.subscriptions() {
            for entry in ctx.read_jsonl(*file_type)? {
                let compile_id = entry
                    .compile_id
                    .clone()
                    .unwrap_or_else(|| "__global__".to_string());

                let dir_entry = directory
                    .entry(compile_id.clone())
                    .or_insert_with(|| CompileDirectoryEntry {
                        display_name: format_display_name(&compile_id),
                        status: "unknown".to_string(),
                        artifacts: Vec::new(),
                        links: Vec::new(),
                    });

                // Add artifact based on entry type
                match entry.entry_type.as_str() {
                    // Compile artifacts
                    "dynamo_output_graph"
                    | "aot_forward_graph"
                    | "aot_backward_graph"
                    | "aot_joint_graph"
                    | "aot_inference_graph"
                    | "inductor_pre_grad_graph"
                    | "inductor_post_grad_graph"
                    | "optimize_ddp_split_graph"
                    | "compiled_autograd_graph"
                    | "graph_dump" => {
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: format!("{}.txt", entry.entry_type),
                            artifact_type: "graph".to_string(),
                        });
                    }

                    "inductor_output_code" => {
                        let base_name = entry
                            .metadata
                            .get("filename")
                            .and_then(|v| v.as_str())
                            .and_then(|p| std::path::Path::new(p).file_stem())
                            .and_then(|s| s.to_str())
                            .map(|s| format!("inductor_output_code_{}.txt", s))
                            .unwrap_or_else(|| "inductor_output_code.txt".to_string());

                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: base_name,
                            artifact_type: "codegen".to_string(),
                        });
                    }

                    // Guards
                    "dynamo_guards" => {
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: "dynamo_guards.html".to_string(),
                            artifact_type: "guards".to_string(),
                        });
                    }

                    "dynamo_cpp_guards_str" => {
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: "dynamo_cpp_guards_str.txt".to_string(),
                            artifact_type: "guards".to_string(),
                        });
                    }

                    // Compilation metrics
                    "compilation_metrics" => {
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: "compilation_metrics.html".to_string(),
                            artifact_type: "metrics".to_string(),
                        });

                        // Update status based on metrics
                        if let Some(fail_type) = entry.metadata.get("fail_type") {
                            if !fail_type.is_null() {
                                dir_entry.status = "failure".to_string();
                            }
                        } else {
                            if dir_entry.status == "unknown" {
                                dir_entry.status = "success".to_string();
                            }
                        }
                    }

                    "bwd_compilation_metrics" => {
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: "bwd_compilation_metrics.html".to_string(),
                            artifact_type: "metrics".to_string(),
                        });
                    }

                    // Cache artifacts
                    "artifact" if is_cache_artifact(&entry) => {
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

                        let ext = if encoding == "json" { "json" } else { "txt" };
                        dir_entry.artifacts.push(DirectoryArtifact {
                            name: format!("{}.{}", name, ext),
                            artifact_type: "cache".to_string(),
                        });
                    }

                    // Links
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

                        dir_entry.links.push(DirectoryLink {
                            name: name.to_string(),
                            url: url.to_string(),
                        });
                    }

                    _ => {}
                }
            }
        }

        // Deduplicate artifacts
        for entry in directory.values_mut() {
            entry.artifacts.sort_by(|a, b| a.name.cmp(&b.name));
            entry.artifacts.dedup_by(|a, b| a.name == b.name);
        }

        // Generate JSON output
        let json = serde_json::to_string_pretty(&directory)?;

        Ok(ModuleOutput {
            files: vec![(PathBuf::from("compile_directory.json"), json)],
            directory_entries: HashMap::new(),
            index_contribution: None,
        })
    }
}

fn format_display_name(compile_id: &str) -> String {
    if compile_id == "__global__" {
        return "Global".to_string();
    }

    // Parse compile_id format: [!<autograd>/]<frame>/<frame_compile>[_<attempt>]
    // e.g., "0_1" -> "0/1", "0_1_2" -> "0/1 (attempt 2)", "!3_0_1" -> "!3/0/1"
    let parts: Vec<&str> = compile_id.split('_').collect();
    match parts.len() {
        2 => format!("{}/{}", parts[0], parts[1]),
        3 => format!("{}/{} (attempt {})", parts[0], parts[1], parts[2]),
        _ => compile_id.to_string(),
    }
}

fn is_cache_artifact(entry: &crate::intermediate::IntermediateEntry) -> bool {
    entry
        .metadata
        .get("name")
        .and_then(|v| v.as_str())
        .map(|name| {
            name.contains("cache_hit")
                || name.contains("cache_miss")
                || name.contains("cache_bypass")
        })
        .unwrap_or(false)
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct CompileDirectoryEntry {
    display_name: String,
    status: String,
    artifacts: Vec<DirectoryArtifact>,
    links: Vec<DirectoryLink>,
}

#[derive(Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
struct DirectoryArtifact {
    name: String,
    #[serde(rename = "type")]
    artifact_type: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
struct DirectoryLink {
    name: String,
    url: String,
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
            files: vec!["compile_artifacts.jsonl".to_string()],
        }
    }

    #[test]
    fn test_compile_directory_module() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create compile_artifacts.jsonl
        let artifacts_path = temp_dir.path().join("compile_artifacts.jsonl");
        let mut file = File::create(&artifacts_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_output_graph","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"graph"}}"#
        )?;

        // Create empty files for other types
        File::create(temp_dir.path().join("guards.jsonl"))?;
        File::create(temp_dir.path().join("compilation_metrics.jsonl"))?;
        File::create(temp_dir.path().join("stacks.jsonl"))?;
        File::create(temp_dir.path().join("cache.jsonl"))?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CompileDirectoryModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(output.files[0].0, PathBuf::from("compile_directory.json"));

        // Parse and verify the JSON
        let dir: HashMap<String, CompileDirectoryEntry> =
            serde_json::from_str(&output.files[0].1)?;
        assert!(dir.contains_key("0_0"));
        assert_eq!(dir["0_0"].display_name, "0/0");
        assert!(dir["0_0"]
            .artifacts
            .iter()
            .any(|a| a.name == "dynamo_output_graph.txt"));

        Ok(())
    }

    #[test]
    fn test_format_display_name() {
        assert_eq!(format_display_name("0_0"), "0/0");
        assert_eq!(format_display_name("0_1_2"), "0/1 (attempt 2)");
        assert_eq!(format_display_name("__global__"), "Global");
    }
}
