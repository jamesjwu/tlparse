//! ExportModule - Handles export mode (torch.export) specific output.
//!
//! This module reads from export.jsonl and generates an export-specific
//! index page showing export failures and the exported program.

use anyhow::Result;
use html_escape::encode_text;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{IndexContribution, Module, ModuleOutput};

/// Module that generates export mode output.
pub struct ExportModule;

impl ExportModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExportModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for ExportModule {
    fn name(&self) -> &'static str {
        "Export"
    }

    fn id(&self) -> &'static str {
        "export"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::Export]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut export_failures: Vec<ExportFailure> = Vec::new();
        let mut exported_program: Option<String> = None;
        let mut files = Vec::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Export)? {
            match entry.entry_type.as_str() {
                "missing_fake_kernel" => {
                    let op = entry
                        .metadata
                        .get("op")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let reason = entry
                        .metadata
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("No fake kernel registered");

                    export_failures.push(ExportFailure {
                        failure_type: "missing_fake_kernel".to_string(),
                        op: op.to_string(),
                        reason: reason.to_string(),
                    });
                }

                "mismatched_fake_kernel" => {
                    let op = entry
                        .metadata
                        .get("op")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let reason = entry
                        .metadata
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Output mismatch");

                    export_failures.push(ExportFailure {
                        failure_type: "mismatched_fake_kernel".to_string(),
                        op: op.to_string(),
                        reason: reason.to_string(),
                    });
                }

                "exported_program" => {
                    exported_program = entry.payload;
                }

                _ => {}
            }
        }

        // Only generate export index if running in export mode
        if !ctx.config.export_mode {
            return Ok(ModuleOutput::default());
        }

        // Generate export index HTML
        let html = self.render_export_index(&export_failures, &exported_program);
        files.push((PathBuf::from("index.html"), html));

        // Generate index contribution for failures summary
        let index_contribution = if !export_failures.is_empty() {
            Some(IndexContribution {
                section: "Export Failures".to_string(),
                html: format!(
                    r#"<div class="export-failures-summary">
    <span class="failure-count">{} export failure(s)</span>
</div>"#,
                    export_failures.len()
                ),
            })
        } else {
            None
        };

        Ok(ModuleOutput {
            files,
            directory_entries: HashMap::new(),
            index_contribution,
        })
    }
}

impl ExportModule {
    fn render_export_index(
        &self,
        failures: &[ExportFailure],
        exported_program: &Option<String>,
    ) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Export Analysis</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; width: 100%; margin-bottom: 20px; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; }
tr:nth-child(even) { background-color: #fafafa; }
.error { color: #dc3545; }
.success { color: #28a745; }
pre { background: #f8f8f8; padding: 10px; overflow-x: auto; }
details { margin: 10px 0; }
summary { cursor: pointer; font-weight: bold; }
</style>
</head>
<body>
<h1>Export Analysis</h1>
"#,
        );

        if failures.is_empty() && exported_program.is_some() {
            html.push_str(r#"<p class="success">✅ Export successful</p>"#);
        } else if !failures.is_empty() {
            html.push_str(r#"<p class="error">❌ Export failed</p>"#);
        }

        // Export failures table
        if !failures.is_empty() {
            html.push_str("<h2>Export Failures</h2>\n");
            html.push_str(
                r#"<table>
<thead>
<tr>
<th>Type</th>
<th>Operator</th>
<th>Reason</th>
</tr>
</thead>
<tbody>
"#,
            );

            for failure in failures {
                html.push_str(&format!(
                    r#"<tr>
<td class="error">{}</td>
<td><code>{}</code></td>
<td>{}</td>
</tr>
"#,
                    encode_text(&failure.failure_type),
                    encode_text(&failure.op),
                    encode_text(&failure.reason)
                ));
            }

            html.push_str("</tbody>\n</table>\n");
        }

        // Exported program
        if let Some(program) = exported_program {
            html.push_str("<h2>Exported Program</h2>\n");
            html.push_str("<details open>\n<summary>View Program</summary>\n<pre>");
            html.push_str(&encode_text(program));
            html.push_str("</pre>\n</details>\n");
        }

        html.push_str("</body>\n</html>");
        html
    }
}

struct ExportFailure {
    failure_type: String,
    op: String,
    reason: String,
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
            compile_ids: vec![],
            string_table_entries: 0,
            parse_mode: "export".to_string(),
            ranks: vec![0],
            files: vec!["export.jsonl".to_string()],
        }
    }

    #[test]
    fn test_export_module_with_failures() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let mut config = ModuleConfig::default();
        config.export_mode = true;

        // Create export.jsonl with missing_fake_kernel
        let export_path = temp_dir.path().join("export.jsonl");
        let mut file = File::create(&export_path)?;
        writeln!(
            file,
            r#"{{"type":"missing_fake_kernel","compile_id":null,"rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"op":"my_custom_op","reason":"No fake kernel registered"}}}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = ExportModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(output.files[0].0, PathBuf::from("index.html"));
        assert!(output.files[0].1.contains("missing_fake_kernel"));
        assert!(output.files[0].1.contains("my_custom_op"));

        Ok(())
    }

    #[test]
    fn test_export_module_not_in_export_mode() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default(); // export_mode = false

        // Create export.jsonl
        let export_path = temp_dir.path().join("export.jsonl");
        File::create(&export_path)?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = ExportModule::new();
        let output = module.render(&ctx)?;

        // Should not generate files when not in export mode
        assert!(output.files.is_empty());

        Ok(())
    }

    #[test]
    fn test_export_module_with_exported_program() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let mut config = ModuleConfig::default();
        config.export_mode = true;

        // Create export.jsonl with exported_program
        let export_path = temp_dir.path().join("export.jsonl");
        let mut file = File::create(&export_path)?;
        writeln!(
            file,
            r#"{{"type":"exported_program","compile_id":null,"rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"class ExportedModule(torch.nn.Module):..."}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = ExportModule::new();
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert!(output.files[0].1.contains("ExportedModule"));
        assert!(output.files[0].1.contains("Export successful"));

        Ok(())
    }
}
