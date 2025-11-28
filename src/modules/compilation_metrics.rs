//! CompilationMetricsModule - Handles compilation metrics and failures output.
//!
//! This module reads from compilation_metrics.jsonl and generates:
//! - compilation_metrics.html per compile ID
//! - bwd_compilation_metrics.html per compile ID
//! - aot_autograd_backward_compilation_metrics.html per compile ID
//! - failures_and_restarts.html (global summary)

use anyhow::Result;
use html_escape::encode_text;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{DirectoryEntry, IndexContribution, Module, ModuleOutput};

/// Module that generates compilation metrics output.
pub struct CompilationMetricsModule {
    plain_text: bool,
}

impl CompilationMetricsModule {
    pub fn new(plain_text: bool) -> Self {
        Self { plain_text }
    }
}

impl Default for CompilationMetricsModule {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Module for CompilationMetricsModule {
    fn name(&self) -> &'static str {
        "Compilation Metrics"
    }

    fn id(&self) -> &'static str {
        "compilation_metrics"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::Guards,
            IntermediateFileType::Stacks,
        ]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        let mut files = Vec::new();
        let mut directory_entries: HashMap<String, Vec<DirectoryEntry>> = HashMap::new();
        let mut failures: Vec<FailureEntry> = Vec::new();

        // Build stack index from stacks.jsonl
        let stack_index = self.build_stack_index(ctx)?;

        // Build specialization index from guards.jsonl
        let specialization_index = self.build_specialization_index(ctx)?;

        // Process compilation metrics
        for entry in ctx.read_jsonl(IntermediateFileType::CompilationMetrics)? {
            let compile_id = entry
                .compile_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            match entry.entry_type.as_str() {
                "compilation_metrics" => {
                    // Parse the metrics metadata
                    let metrics: CompilationMetrics =
                        serde_json::from_value(entry.metadata.clone()).unwrap_or_default();

                    // Track failures
                    if metrics.fail_type.is_some() {
                        failures.push(FailureEntry {
                            compile_id: compile_id.clone(),
                            fail_type: metrics.fail_type.clone().unwrap_or_default(),
                            fail_reason: metrics.fail_reason.clone(),
                            co_name: metrics.co_name.clone(),
                            co_filename: metrics.co_filename.clone(),
                        });
                    }

                    // Get associated stack
                    let stack = stack_index.get(&compile_id);

                    // Get specializations
                    let specializations = specialization_index.get(&compile_id);

                    let html = self.render_compilation_metrics_html(
                        &compile_id,
                        &metrics,
                        stack,
                        specializations,
                    );

                    let filename = "compilation_metrics.html".to_string();
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

                "bwd_compilation_metrics" => {
                    let metrics: BwdCompilationMetrics =
                        serde_json::from_value(entry.metadata.clone()).unwrap_or_default();

                    // Track failures
                    if metrics.fail_type.is_some() {
                        failures.push(FailureEntry {
                            compile_id: compile_id.clone(),
                            fail_type: metrics.fail_type.clone().unwrap_or_default(),
                            fail_reason: metrics.fail_reason.clone(),
                            co_name: None,
                            co_filename: None,
                        });
                    }

                    let html = self.render_bwd_compilation_metrics_html(&compile_id, &metrics);

                    let filename = "bwd_compilation_metrics.html".to_string();
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

                "aot_autograd_backward_compilation_metrics" => {
                    let metrics: AotAutogradBackwardMetrics =
                        serde_json::from_value(entry.metadata.clone()).unwrap_or_default();

                    // Track failures
                    if metrics.fail_type.is_some() {
                        failures.push(FailureEntry {
                            compile_id: compile_id.clone(),
                            fail_type: metrics.fail_type.clone().unwrap_or_default(),
                            fail_reason: metrics.fail_reason.clone(),
                            co_name: None,
                            co_filename: None,
                        });
                    }

                    let html = self.render_aot_autograd_metrics_html(&compile_id, &metrics);

                    let filename = "aot_autograd_backward_compilation_metrics.html".to_string();
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

                _ => {}
            }
        }

        // Generate failures_and_restarts.html if there are failures
        let index_contribution = if !failures.is_empty() {
            let failures_html = self.render_failures_html(&failures);
            files.push((PathBuf::from("failures_and_restarts.html"), failures_html));

            Some(IndexContribution {
                section: "Failures and Restarts".to_string(),
                html: format!(
                    r#"<div class="failures-summary">
    <span class="failure-count">{} failure(s)</span>
    <a href="failures_and_restarts.html">View Details</a>
</div>"#,
                    failures.len()
                ),
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

impl CompilationMetricsModule {
    fn build_stack_index(&self, ctx: &ModuleContext) -> Result<HashMap<String, StackData>> {
        let mut index = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Stacks)? {
            if entry.entry_type != "dynamo_start" {
                continue;
            }

            if let Some(compile_id) = entry.compile_id {
                if let Some(stack) = entry.metadata.get("stack") {
                    index.insert(
                        compile_id,
                        StackData {
                            stack_json: stack.clone(),
                        },
                    );
                }
            }
        }

        Ok(index)
    }

    fn build_specialization_index(
        &self,
        ctx: &ModuleContext,
    ) -> Result<HashMap<String, Vec<Specialization>>> {
        let mut index: HashMap<String, Vec<Specialization>> = HashMap::new();

        for entry in ctx.read_jsonl(IntermediateFileType::Guards)? {
            if entry.entry_type != "symbolic_shape_specialization" {
                continue;
            }

            if let Some(compile_id) = entry.compile_id {
                let spec = Specialization {
                    symbol: entry
                        .metadata
                        .get("symbol")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    value: entry
                        .metadata
                        .get("value")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    reason: entry
                        .metadata
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                };
                index.entry(compile_id).or_default().push(spec);
            }
        }

        Ok(index)
    }

    fn render_compilation_metrics_html(
        &self,
        compile_id: &str,
        metrics: &CompilationMetrics,
        stack: Option<&StackData>,
        specializations: Option<&Vec<Specialization>>,
    ) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Compilation Metrics</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; margin-bottom: 20px; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; width: 200px; }
.error { color: #dc3545; }
.success { color: #28a745; }
.warning { color: #ffc107; }
pre { background: #f8f8f8; padding: 10px; overflow-x: auto; }
details { margin: 10px 0; }
summary { cursor: pointer; font-weight: bold; }
</style>
</head>
<body>
"#,
        );

        html.push_str(&format!("<h1>Compilation Metrics - {}</h1>\n", compile_id));

        // Status indicator
        if metrics.fail_type.is_some() {
            html.push_str(r#"<p class="error">❌ Compilation Failed</p>"#);
        } else {
            html.push_str(r#"<p class="success">✅ Compilation Successful</p>"#);
        }

        // Basic info table
        html.push_str("<h2>Basic Information</h2>\n<table>\n");
        if let Some(name) = &metrics.co_name {
            html.push_str(&format!(
                "<tr><th>Function Name</th><td>{}</td></tr>\n",
                encode_text(name)
            ));
        }
        if let Some(filename) = &metrics.co_filename {
            html.push_str(&format!(
                "<tr><th>Filename</th><td>{}</td></tr>\n",
                encode_text(filename)
            ));
        }
        if let Some(lineno) = metrics.co_firstlineno {
            html.push_str(&format!(
                "<tr><th>First Line</th><td>{}</td></tr>\n",
                lineno
            ));
        }
        html.push_str("</table>\n");

        // Timing information
        html.push_str("<h2>Timing</h2>\n<table>\n");
        if let Some(time) = metrics.entire_frame_compile_time_s {
            html.push_str(&format!(
                "<tr><th>Total Compile Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(time) = metrics.backend_compile_time_s {
            html.push_str(&format!(
                "<tr><th>Backend Compile Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(time) = metrics.inductor_compile_time_s {
            html.push_str(&format!(
                "<tr><th>Inductor Compile Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(time) = metrics.code_gen_time_s {
            html.push_str(&format!(
                "<tr><th>Code Gen Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        html.push_str("</table>\n");

        // Graph statistics
        html.push_str("<h2>Graph Statistics</h2>\n<table>\n");
        if let Some(count) = metrics.graph_op_count {
            html.push_str(&format!(
                "<tr><th>Graph Op Count</th><td>{}</td></tr>\n",
                count
            ));
        }
        if let Some(count) = metrics.graph_node_count {
            html.push_str(&format!(
                "<tr><th>Graph Node Count</th><td>{}</td></tr>\n",
                count
            ));
        }
        if let Some(count) = metrics.graph_input_count {
            html.push_str(&format!(
                "<tr><th>Graph Input Count</th><td>{}</td></tr>\n",
                count
            ));
        }
        if let Some(count) = metrics.guard_count {
            html.push_str(&format!(
                "<tr><th>Guard Count</th><td>{}</td></tr>\n",
                count
            ));
        }
        if let Some(count) = metrics.shape_env_guard_count {
            html.push_str(&format!(
                "<tr><th>Shape Env Guard Count</th><td>{}</td></tr>\n",
                count
            ));
        }
        html.push_str("</table>\n");

        // Failure information
        if let Some(fail_type) = &metrics.fail_type {
            html.push_str("<h2 class=\"error\">Failure Information</h2>\n<table>\n");
            html.push_str(&format!(
                "<tr><th>Failure Type</th><td>{}</td></tr>\n",
                encode_text(fail_type)
            ));
            if let Some(reason) = &metrics.fail_reason {
                html.push_str(&format!(
                    "<tr><th>Failure Reason</th><td><pre>{}</pre></td></tr>\n",
                    encode_text(reason)
                ));
            }
            if let Some(filename) = &metrics.fail_user_frame_filename {
                html.push_str(&format!(
                    "<tr><th>User Frame</th><td>{}:{}</td></tr>\n",
                    encode_text(filename),
                    metrics.fail_user_frame_lineno.unwrap_or(0)
                ));
            }
            html.push_str("</table>\n");
        }

        // Restart reasons
        if let Some(reasons) = &metrics.restart_reasons {
            if !reasons.is_empty() {
                html.push_str("<h2 class=\"warning\">Restart Reasons</h2>\n<ul>\n");
                for reason in reasons {
                    html.push_str(&format!("<li>{}</li>\n", encode_text(reason)));
                }
                html.push_str("</ul>\n");
            }
        }

        // Specializations
        if let Some(specs) = specializations {
            if !specs.is_empty() {
                html.push_str("<h2>Symbolic Shape Specializations</h2>\n<table>\n");
                html.push_str("<tr><th>Symbol</th><th>Value</th><th>Reason</th></tr>\n");
                for spec in specs {
                    html.push_str(&format!(
                        "<tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                        encode_text(spec.symbol.as_deref().unwrap_or("")),
                        encode_text(spec.value.as_deref().unwrap_or("")),
                        encode_text(spec.reason.as_deref().unwrap_or(""))
                    ));
                }
                html.push_str("</table>\n");
            }
        }

        // Stack trace
        if let Some(_stack) = stack {
            html.push_str("<details>\n<summary>Stack Trace</summary>\n");
            html.push_str("<pre>Stack trace data available in stacks.jsonl</pre>\n");
            html.push_str("</details>\n");
        }

        html.push_str("</body>\n</html>");
        html
    }

    fn render_bwd_compilation_metrics_html(
        &self,
        compile_id: &str,
        metrics: &BwdCompilationMetrics,
    ) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Backward Compilation Metrics</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; width: 200px; }
.error { color: #dc3545; }
pre { background: #f8f8f8; padding: 10px; overflow-x: auto; }
</style>
</head>
<body>
"#,
        );

        html.push_str(&format!(
            "<h1>Backward Compilation Metrics - {}</h1>\n",
            compile_id
        ));

        html.push_str("<table>\n");
        if let Some(time) = metrics.inductor_compile_time_s {
            html.push_str(&format!(
                "<tr><th>Inductor Compile Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(time) = metrics.code_gen_time_s {
            html.push_str(&format!(
                "<tr><th>Code Gen Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(fail_type) = &metrics.fail_type {
            html.push_str(&format!(
                "<tr><th>Failure Type</th><td class=\"error\">{}</td></tr>\n",
                encode_text(fail_type)
            ));
        }
        if let Some(fail_reason) = &metrics.fail_reason {
            html.push_str(&format!(
                "<tr><th>Failure Reason</th><td><pre>{}</pre></td></tr>\n",
                encode_text(fail_reason)
            ));
        }
        html.push_str("</table>\n</body>\n</html>");

        html
    }

    fn render_aot_autograd_metrics_html(
        &self,
        compile_id: &str,
        metrics: &AotAutogradBackwardMetrics,
    ) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>AOT Autograd Backward Compilation Metrics</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; width: 200px; }
.error { color: #dc3545; }
pre { background: #f8f8f8; padding: 10px; overflow-x: auto; }
</style>
</head>
<body>
"#,
        );

        html.push_str(&format!(
            "<h1>AOT Autograd Backward Compilation Metrics - {}</h1>\n",
            compile_id
        ));

        html.push_str("<table>\n");
        if let Some(time) = metrics.start_time {
            html.push_str(&format!(
                "<tr><th>Start Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(time) = metrics.elapsed_time {
            html.push_str(&format!(
                "<tr><th>Elapsed Time</th><td>{:.3}s</td></tr>\n",
                time
            ));
        }
        if let Some(fail_type) = &metrics.fail_type {
            html.push_str(&format!(
                "<tr><th>Failure Type</th><td class=\"error\">{}</td></tr>\n",
                encode_text(fail_type)
            ));
        }
        if let Some(fail_reason) = &metrics.fail_reason {
            html.push_str(&format!(
                "<tr><th>Failure Reason</th><td><pre>{}</pre></td></tr>\n",
                encode_text(fail_reason)
            ));
        }
        html.push_str("</table>\n</body>\n</html>");

        html
    }

    fn render_failures_html(&self, failures: &[FailureEntry]) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html>
<head>
<title>Failures and Restarts</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 20px; }
table { border-collapse: collapse; width: 100%; }
th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
th { background-color: #f5f5f5; }
tr:nth-child(even) { background-color: #fafafa; }
.error { color: #dc3545; }
pre { margin: 0; white-space: pre-wrap; word-wrap: break-word; font-size: 12px; }
</style>
</head>
<body>
<h1>Failures and Restarts</h1>
<p>Found "#,
        );

        html.push_str(&format!("{} failure(s)</p>\n", failures.len()));

        html.push_str(
            r#"<table>
<thead>
<tr>
<th>Compile ID</th>
<th>Function</th>
<th>Failure Type</th>
<th>Reason</th>
</tr>
</thead>
<tbody>
"#,
        );

        for failure in failures {
            let function = failure
                .co_name
                .as_ref()
                .map(|n| {
                    let filename = failure.co_filename.as_deref().unwrap_or("");
                    format!("{} ({})", encode_text(n), encode_text(filename))
                })
                .unwrap_or_else(|| "-".to_string());

            let reason = failure
                .fail_reason
                .as_ref()
                .map(|r| encode_text(r).to_string())
                .unwrap_or_else(|| "-".to_string());

            html.push_str(&format!(
                r#"<tr>
<td><a href="{}/compilation_metrics.html">{}</a></td>
<td>{}</td>
<td class="error">{}</td>
<td><pre>{}</pre></td>
</tr>
"#,
                failure.compile_id,
                failure.compile_id,
                function,
                encode_text(&failure.fail_type),
                reason
            ));
        }

        html.push_str("</tbody>\n</table>\n</body>\n</html>");
        html
    }
}

struct StackData {
    stack_json: serde_json::Value,
}

struct Specialization {
    symbol: Option<String>,
    value: Option<String>,
    reason: Option<String>,
}

struct FailureEntry {
    compile_id: String,
    fail_type: String,
    fail_reason: Option<String>,
    co_name: Option<String>,
    co_filename: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct CompilationMetrics {
    co_name: Option<String>,
    co_filename: Option<String>,
    co_firstlineno: Option<i32>,
    cache_size: Option<u64>,
    accumulated_cache_size: Option<u64>,
    guard_count: Option<u64>,
    shape_env_guard_count: Option<u64>,
    graph_op_count: Option<u64>,
    graph_node_count: Option<u64>,
    graph_input_count: Option<u64>,
    start_time: Option<f64>,
    entire_frame_compile_time_s: Option<f64>,
    backend_compile_time_s: Option<f64>,
    inductor_compile_time_s: Option<f64>,
    code_gen_time_s: Option<f64>,
    fail_type: Option<String>,
    fail_reason: Option<String>,
    fail_user_frame_filename: Option<String>,
    fail_user_frame_lineno: Option<u32>,
    non_compliant_ops: Option<Vec<String>>,
    compliant_custom_ops: Option<Vec<String>>,
    restart_reasons: Option<Vec<String>>,
    dynamo_time_before_restart_s: Option<f64>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct BwdCompilationMetrics {
    inductor_compile_time_s: Option<f64>,
    code_gen_time_s: Option<f64>,
    fail_type: Option<String>,
    fail_reason: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct AotAutogradBackwardMetrics {
    start_time: Option<f64>,
    elapsed_time: Option<f64>,
    fail_type: Option<String>,
    fail_reason: Option<String>,
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
            files: vec!["compilation_metrics.jsonl".to_string()],
        }
    }

    #[test]
    fn test_compilation_metrics_module() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create compilation_metrics.jsonl
        let metrics_path = temp_dir.path().join("compilation_metrics.jsonl");
        let mut file = File::create(&metrics_path)?;
        writeln!(
            file,
            r#"{{"type":"compilation_metrics","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"co_name":"forward","entire_frame_compile_time_s":1.5}}}}"#
        )?;

        // Create empty stacks.jsonl and guards.jsonl
        File::create(temp_dir.path().join("stacks.jsonl"))?;
        File::create(temp_dir.path().join("guards.jsonl"))?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CompilationMetricsModule::new(false);
        let output = module.render(&ctx)?;

        assert_eq!(output.files.len(), 1);
        assert!(output.files[0].0.to_string_lossy().contains("compilation_metrics.html"));
        assert!(output.files[0].1.contains("forward"));

        Ok(())
    }

    #[test]
    fn test_failure_tracking() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create compilation_metrics.jsonl with a failure
        let metrics_path = temp_dir.path().join("compilation_metrics.jsonl");
        let mut file = File::create(&metrics_path)?;
        writeln!(
            file,
            r#"{{"type":"compilation_metrics","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"co_name":"forward","fail_type":"graph_break","fail_reason":"unsupported op"}}}}"#
        )?;

        // Create empty stacks.jsonl and guards.jsonl
        File::create(temp_dir.path().join("stacks.jsonl"))?;
        File::create(temp_dir.path().join("guards.jsonl"))?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = CompilationMetricsModule::new(false);
        let output = module.render(&ctx)?;

        // Should have compilation_metrics.html and failures_and_restarts.html
        assert_eq!(output.files.len(), 2);
        assert!(output.files.iter().any(|(p, _)| p.to_string_lossy().contains("failures_and_restarts")));

        // Should have index contribution
        assert!(output.index_contribution.is_some());

        Ok(())
    }
}
