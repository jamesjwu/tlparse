//! GuardsModule - Handles dynamo_guards and dynamo_cpp_guards_str output.
//!
//! This module reads guard-related data from guards.jsonl and generates:
//! - dynamo_guards.html - Rendered guard tables per compile ID
//! - dynamo_cpp_guards_str.txt - C++ guard strings per compile ID

use anyhow::Result;
use html_escape::encode_text;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, Module, ModuleOutput};

/// Module that generates guard output files.
pub struct GuardsModule {
    plain_text: bool,
}

impl GuardsModule {
    pub fn new(plain_text: bool) -> Self {
        Self { plain_text }
    }
}

impl Default for GuardsModule {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Module for GuardsModule {
    fn name(&self) -> &'static str {
        "Dynamo Guards"
    }

    fn id(&self) -> &'static str {
        "guards"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::Guards]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries: HashMap<String, Vec<DirectoryEntry>> = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            let compile_id = entry
                .compile_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            match entry.entry_type.as_str() {
                "dynamo_guards" => {
                    // Parse guards from payload
                    let guards_json = entry.payload.clone().unwrap_or_else(|| "[]".to_string());

                    let guards: Vec<DynamoGuard> =
                        serde_json::from_str(&guards_json).unwrap_or_default();

                    let html = self.render_guards_html(&guards);
                    let filename = "dynamo_guards.html".to_string();
                    let path = PathBuf::from(&compile_id).join(&filename);
                    files.push((path.clone(), html));

                    directory_entries
                        .entry(compile_id)
                        .or_default()
                        .push(DirectoryEntry::new(
                            filename,
                            path.to_string_lossy().to_string(),
                        ));
                }

                "dynamo_cpp_guards_str" => {
                    let content = entry.payload.unwrap_or_default();
                    let filename = "dynamo_cpp_guards_str.txt".to_string();
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

impl GuardsModule {
    fn render_guards_html(&self, guards: &[DynamoGuard]) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Dynamo Guards</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; vertical-align: top; }
th { background-color: #f5f5f5; }
tr:nth-child(even) { background-color: #fafafa; }
pre { margin: 0; white-space: pre-wrap; word-wrap: break-word; font-size: 12px; }
.guard-types { font-size: 11px; color: #666; }
.filter-box { margin-bottom: 16px; }
.filter-box input { padding: 8px; width: 300px; border: 1px solid #ddd; border-radius: 4px; }
.count { color: #666; font-size: 14px; margin-left: 10px; }
</style>
<script>
function filterGuards() {
    const input = document.getElementById('filter-input');
    const filter = input.value.toLowerCase();
    const table = document.getElementById('guards-table');
    const rows = table.getElementsByClassName('guard-row');
    let visible = 0;

    for (let i = 0; i < rows.length; i++) {
        const row = rows[i];
        const text = row.textContent.toLowerCase();
        if (text.includes(filter)) {
            row.style.display = '';
            visible++;
        } else {
            row.style.display = 'none';
        }
    }

    document.getElementById('count').textContent = visible + ' / ' + rows.length + ' guards';
}
</script>
</head>
<body>
<h1>Dynamo Guards</h1>
<div class="filter-box">
    <input type="text" id="filter-input" placeholder="Filter guards..." oninput="filterGuards()">
    <span id="count" class="count">"#,
        );

        html.push_str(&format!("{} guards</span>\n</div>\n", guards.len()));

        html.push_str(
            r#"<table id="guards-table">
<thead>
<tr>
<th>Code</th>
<th>Type</th>
<th>Guard Types</th>
</tr>
</thead>
<tbody>
"#,
        );

        for guard in guards {
            let code = guard
                .code
                .as_ref()
                .map(|c| encode_text(c).to_string())
                .unwrap_or_default();
            let guard_type = guard
                .guard_type
                .as_ref()
                .map(|t| encode_text(t).to_string())
                .unwrap_or_default();
            let guard_types = guard
                .guard_types
                .as_ref()
                .map(|ts| ts.join(", "))
                .unwrap_or_default();

            html.push_str(&format!(
                r#"<tr class="guard-row">
<td><pre>{}</pre></td>
<td>{}</td>
<td class="guard-types">{}</td>
</tr>
"#,
                code, guard_type, guard_types
            ));
        }

        html.push_str(
            r#"</tbody>
</table>
</body>
</html>"#,
        );

        html
    }
}

/// Structure representing a dynamo guard
#[derive(Debug, serde::Deserialize)]
struct DynamoGuard {
    code: Option<String>,
    #[serde(rename = "type")]
    guard_type: Option<String>,
    guard_types: Option<Vec<String>>,
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
    fn test_guards_module() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create guards.jsonl
        let guards_path = temp_dir.path().join("guards.jsonl");
        let mut file = File::create(&guards_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_guards","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"[{{\"code\":\"x.size(0) == 10\",\"type\":\"SHAPE_ENV\"}}]"}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = GuardsModule::new(false);
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(
            output.files[0].0,
            PathBuf::from("0_0/dynamo_guards.html")
        );
        assert!(output.files[0].1.contains("x.size(0) == 10"));

        Ok(())
    }

    #[test]
    fn test_cpp_guards_str() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create guards.jsonl with dynamo_cpp_guards_str
        let guards_path = temp_dir.path().join("guards.jsonl");
        let mut file = File::create(&guards_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_cpp_guards_str","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{}},"payload":"// CPP guard code here"}}"#
        )?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = GuardsModule::new(false);
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert_eq!(
            output.files[0].0,
            PathBuf::from("0_0/dynamo_cpp_guards_str.txt")
        );
        assert!(output.files[0].1.contains("CPP guard code"));

        Ok(())
    }
}
