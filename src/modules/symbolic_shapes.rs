//! SymbolicShapesModule - Handles symbolic shape information rendering.
//!
//! This module reads guard-related symbolic shape entries and generates
//! symbolic guard information HTML files.

use anyhow::Result;
use html_escape::encode_text;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, Module, ModuleOutput};

/// Module that generates symbolic shape information output.
pub struct SymbolicShapesModule;

impl SymbolicShapesModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SymbolicShapesModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for SymbolicShapesModule {
    fn name(&self) -> &'static str {
        "Symbolic Shapes"
    }

    fn id(&self) -> &'static str {
        "symbolic_shapes"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::Guards]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries: HashMap<String, Vec<DirectoryEntry>> = HashMap::new();

        // Build expression info index first
        let expr_info_index = self.build_expression_index(ctx)?;

        let mut output_count = 0;

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            let compile_id = entry
                .compile_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            match entry.entry_type.as_str() {
                "propagate_real_tensors_provenance" | "guard_added" => {
                    let html = self.render_symbolic_guard_html(&entry, &expr_info_index)?;
                    let filename = format!("symbolic_guard_information_{}.html", output_count);
                    let path = PathBuf::from(&compile_id).join(&filename);
                    files.push((path.clone(), html));

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry::new(
                            filename,
                            path.to_string_lossy().to_string(),
                        ));

                    output_count += 1;
                }

                "symbolic_shape_specialization" => {
                    // Already handled by CompilationMetricsModule, but we could add
                    // additional visualization here if needed
                }

                "create_unbacked_symbol" => {
                    // Could add visualization for unbacked symbols
                }

                _ => {}
            }
        }

        Ok(ModuleOutput {
            files,
            directory_entries,
            index_contribution: None,
        })
    }
}

impl SymbolicShapesModule {
    fn build_expression_index(
        &self,
        ctx: &ModuleContext,
    ) -> Result<HashMap<u64, ExpressionInfo>> {
        let mut index = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type != "expression_created" {
                continue;
            }

            let id = entry
                .metadata
                .get("id")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            index.insert(
                id,
                ExpressionInfo {
                    result: entry
                        .metadata
                        .get("result")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    method: entry
                        .metadata
                        .get("method")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    arguments: entry
                        .metadata
                        .get("arguments")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    argument_ids: entry
                        .metadata
                        .get("argument_ids")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                        .unwrap_or_default(),
                },
            );
        }

        Ok(index)
    }

    fn render_symbolic_guard_html(
        &self,
        entry: &crate::intermediate::IntermediateEntry,
        expr_info_index: &HashMap<u64, ExpressionInfo>,
    ) -> Result<String> {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Symbolic Guard Information</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; margin-bottom: 20px; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; width: 150px; }
pre { background: #f8f8f8; padding: 10px; overflow-x: auto; margin: 0; }
details { margin: 10px 0; }
summary { cursor: pointer; font-weight: bold; }
.expr-tree { padding-left: 20px; }
.expr-node { margin: 5px 0; padding: 5px; border-left: 2px solid #ddd; }
</style>
</head>
<body>
"#,
        );

        html.push_str(&format!(
            "<h1>Symbolic Guard Information - {}</h1>\n",
            entry.entry_type
        ));

        // Expression
        if let Some(expr) = entry.metadata.get("expr").and_then(|v| v.as_str()) {
            html.push_str("<h2>Expression</h2>\n");
            html.push_str(&format!("<pre>{}</pre>\n", encode_text(expr)));
        }

        // User stack
        if let Some(user_stack) = entry.metadata.get("user_stack") {
            html.push_str("<details open>\n<summary>User Stack</summary>\n<pre>");
            if let Some(arr) = user_stack.as_array() {
                for frame in arr {
                    if let Some(frame_str) = frame.as_str() {
                        html.push_str(&format!("{}\n", encode_text(frame_str)));
                    }
                }
            }
            html.push_str("</pre>\n</details>\n");
        }

        // Framework stack
        if let Some(stack) = entry.metadata.get("stack") {
            html.push_str("<details>\n<summary>Framework Stack</summary>\n<pre>");
            if let Some(arr) = stack.as_array() {
                for frame in arr {
                    if let Some(frame_str) = frame.as_str() {
                        html.push_str(&format!("{}\n", encode_text(frame_str)));
                    }
                }
            }
            html.push_str("</pre>\n</details>\n");
        }

        // Expression tree (if expr_node_id is available)
        if let Some(expr_node_id) = entry.metadata.get("expr_node_id").and_then(|v| v.as_u64()) {
            html.push_str("<details>\n<summary>Expression Tree</summary>\n");
            html.push_str("<div class=\"expr-tree\">");
            self.render_expression_tree(&mut html, expr_node_id, expr_info_index, 0);
            html.push_str("</div>\n</details>\n");
        }

        // Frame locals
        if let Some(frame_locals) = entry.metadata.get("frame_locals") {
            html.push_str("<details>\n<summary>Frame Locals</summary>\n<pre>");
            let formatted = serde_json::to_string_pretty(frame_locals).unwrap_or_default();
            html.push_str(&encode_text(&formatted));
            html.push_str("</pre>\n</details>\n");
        }

        html.push_str("</body>\n</html>");
        Ok(html)
    }

    fn render_expression_tree(
        &self,
        html: &mut String,
        node_id: u64,
        expr_info_index: &HashMap<u64, ExpressionInfo>,
        depth: usize,
    ) {
        if depth > 20 {
            html.push_str("<div class=\"expr-node\">... (max depth)</div>");
            return;
        }

        if let Some(info) = expr_info_index.get(&node_id) {
            html.push_str("<div class=\"expr-node\">");
            if let Some(result) = &info.result {
                html.push_str(&format!("<strong>{}</strong>", encode_text(result)));
            }
            if let Some(method) = &info.method {
                html.push_str(&format!(" ({})", encode_text(method)));
            }
            if !info.arguments.is_empty() {
                html.push_str(&format!(
                    "<br>Args: {}",
                    info.arguments
                        .iter()
                        .map(|a| encode_text(a).to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            html.push_str("</div>\n");

            // Recurse into argument_ids
            for arg_id in &info.argument_ids {
                self.render_expression_tree(html, *arg_id, expr_info_index, depth + 1);
            }
        } else {
            html.push_str(&format!(
                "<div class=\"expr-node\">Node {} (not found)</div>",
                node_id
            ));
        }
    }
}

struct ExpressionInfo {
    result: Option<String>,
    method: Option<String>,
    arguments: Vec<String>,
    argument_ids: Vec<u64>,
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
            files: vec!["guards.jsonl".to_string()],
        }
    }

    #[test]
    fn test_symbolic_shapes_module() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create guards.jsonl with guard_added entry
        let guards_path = temp_dir.path().join("guards.jsonl");
        let mut file = File::create(&guards_path)?;
        writeln!(
            file,
            r#"{{"type":"guard_added","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"expr":"s0 == 10","user_stack":["frame1","frame2"]}}}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = SymbolicShapesModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert!(output.files[0]
            .0
            .to_string_lossy()
            .contains("symbolic_guard_information"));
        assert!(output.files[0].1.contains("s0 == 10"));

        Ok(())
    }

    #[test]
    fn test_empty_guards() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create empty guards.jsonl
        File::create(temp_dir.path().join("guards.jsonl"))?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = SymbolicShapesModule::new();
        let output = module.render(&ctx)?;

        assert!(output.files.is_empty());

        Ok(())
    }
}
