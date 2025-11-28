//! StackTrieModule - Generates stack trie visualization for index page.
//!
//! This module reads stack traces from stacks.jsonl (dynamo_start entries)
//! and builds a hierarchical trie visualization showing compilation call sites.

use anyhow::Result;
use html_escape::encode_text;
use std::collections::HashMap;
use std::fmt::Write;

use crate::intermediate::IntermediateFileType;
use crate::modules::context::ModuleContext;
use crate::modules::{IndexContribution, Module, ModuleOutput};
use crate::types::{
    CompilationMetricsIndex, CompilationMetricsMetadata, CompileId, FrameSummary, StackSummary,
};

/// Module that generates stack trie visualization for the index page.
pub struct StackTrieModule;

impl StackTrieModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StackTrieModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for StackTrieModule {
    fn name(&self) -> &'static str {
        "Stack Trie"
    }

    fn id(&self) -> &'static str {
        "stack_trie"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::Stacks,
            IntermediateFileType::CompilationMetrics,
        ]
    }

    fn render(&self, ctx: &ModuleContext) -> Result<ModuleOutput> {
        // Build metrics index for status indicators
        let metrics_index = self.build_metrics_index(ctx)?;

        // Build stack trie from stacks.jsonl
        let stack_trie = self.build_stack_trie(ctx)?;

        if stack_trie.is_empty() {
            return Ok(ModuleOutput::default());
        }

        // Render the stack trie HTML
        let html = stack_trie.fmt(Some(&metrics_index), "Stack Trie", true)?;

        Ok(ModuleOutput {
            files: Vec::new(),
            directory_entries: HashMap::new(),
            index_contribution: Some(IndexContribution {
                section: "Stack Trie".to_string(),
                html,
            }),
        })
    }
}

impl StackTrieModule {
    fn build_metrics_index(&self, ctx: &ModuleContext) -> Result<CompilationMetricsIndex> {
        let mut index = CompilationMetricsIndex::default();

        for entry in ctx.read_jsonl(IntermediateFileType::CompilationMetrics)? {
            if entry.entry_type != "compilation_metrics" {
                continue;
            }

            let compile_id = self.parse_compile_id(&entry.compile_id);

            // Parse metadata from the entry
            let metadata: Option<CompilationMetricsMetadata> =
                serde_json::from_value(entry.metadata.clone()).ok();

            if let Some(m) = metadata {
                index.entry(compile_id).or_default().push(m);
            }
        }

        Ok(index)
    }

    fn build_stack_trie(&self, ctx: &ModuleContext) -> Result<StackTrieNode> {
        let mut trie = StackTrieNode::default();

        for entry in ctx.read_jsonl(IntermediateFileType::Stacks)? {
            if entry.entry_type != "dynamo_start" {
                continue;
            }

            let compile_id = self.parse_compile_id(&entry.compile_id);

            // Extract stack from metadata
            if let Some(stack_value) = entry.metadata.get("stack") {
                if let Ok(mut stack) = serde_json::from_value::<StackSummary>(stack_value.clone()) {
                    // Remove convert_frame suffixes similar to lib.rs
                    maybe_remove_convert_frame_suffixes(&mut stack);
                    // Reverse to show from top to bottom
                    stack.reverse();
                    trie.insert(stack, compile_id);
                }
            }
        }

        Ok(trie)
    }

    fn parse_compile_id(&self, compile_id_str: &Option<String>) -> Option<CompileId> {
        compile_id_str.as_ref().and_then(|s| {
            // Parse compile_id string back to CompileId struct
            // Format: "0_1" or "0_1_2" or "!3_0_1"
            let s = s.trim();
            let (has_autograd, rest) = if s.starts_with('!') {
                (true, &s[1..])
            } else {
                (false, s)
            };

            let parts: Vec<&str> = rest.split('_').collect();

            let (compiled_autograd_id, frame_id, frame_compile_id, attempt) = if has_autograd {
                // !<autograd>_<frame>_<frame_compile>[_<attempt>]
                let autograd = parts.first().and_then(|p| p.parse().ok());
                let frame = parts.get(1).and_then(|p| p.parse().ok());
                let frame_compile = parts.get(2).and_then(|p| p.parse().ok());
                let attempt = parts.get(3).and_then(|p| p.parse().ok());
                (autograd, frame, frame_compile, attempt)
            } else {
                // <frame>_<frame_compile>[_<attempt>]
                let frame = parts.first().and_then(|p| p.parse().ok());
                let frame_compile = parts.get(1).and_then(|p| p.parse().ok());
                let attempt = parts.get(2).and_then(|p| p.parse().ok());
                (None, frame, frame_compile, attempt)
            };

            Some(CompileId {
                compiled_autograd_id,
                frame_id,
                frame_compile_id,
                attempt,
            })
        })
    }
}

/// Remove common convert_frame suffixes from stack traces
fn maybe_remove_convert_frame_suffixes(frames: &mut Vec<FrameSummary>) {
    let all_target_frames = [
        [
            ("torch/_dynamo/convert_frame.py", "catch_errors"),
            ("torch/_dynamo/convert_frame.py", "_convert_frame"),
            ("torch/_dynamo/convert_frame.py", "_convert_frame_assert"),
        ],
        [
            ("torch/_dynamo/convert_frame.py", "__call__"),
            ("torch/_dynamo/convert_frame.py", "__call__"),
            ("torch/_dynamo/convert_frame.py", "__call__"),
        ],
    ];

    let len = frames.len();
    for target_frames in all_target_frames {
        if len >= target_frames.len() {
            let suffix = &frames[len - target_frames.len()..];
            if suffix.iter().zip(target_frames.iter()).all(|(frame, target)| {
                let filename = frame
                    .uninterned_filename
                    .as_deref()
                    .unwrap_or("(unknown)");
                simplify_filename(filename) == target.0 && frame.name == target.1
            }) {
                frames.truncate(len - target_frames.len());
            }
        }
    }
}

fn simplify_filename(filename: &str) -> &str {
    let parts: Vec<&str> = filename.split("#link-tree/").collect();
    if parts.len() > 1 {
        return parts[1];
    }
    filename
}

/// Stack trie node for building hierarchical visualization
#[derive(Default)]
struct StackTrieNode {
    terminal: Vec<Option<CompileId>>,
    children: indexmap::IndexMap<FrameKey, StackTrieNode, fxhash::FxBuildHasher>,
}

/// Key for indexing into stack trie children
#[derive(Eq, PartialEq, Hash)]
struct FrameKey {
    filename: String,
    line: i32,
    name: String,
    loc: Option<String>,
}

impl From<&FrameSummary> for FrameKey {
    fn from(frame: &FrameSummary) -> Self {
        Self {
            filename: frame
                .uninterned_filename
                .clone()
                .unwrap_or_else(|| "(unknown)".to_string()),
            line: frame.line,
            name: frame.name.clone(),
            loc: frame.loc.clone(),
        }
    }
}

impl StackTrieNode {
    fn insert(&mut self, mut stack: StackSummary, compile_id: Option<CompileId>) {
        let mut cur = self;
        for frame in stack.drain(..) {
            let key = FrameKey::from(&frame);
            cur = cur.children.entry(key).or_default();
        }
        cur.terminal.push(compile_id);
    }

    fn is_empty(&self) -> bool {
        self.children.is_empty() && self.terminal.is_empty()
    }

    fn fmt(
        &self,
        metrics_index: Option<&CompilationMetricsIndex>,
        caption: &str,
        open: bool,
    ) -> Result<String, std::fmt::Error> {
        let mut f = String::new();
        write!(f, "<details{}>", if open { " open" } else { "" })?;
        write!(f, "<summary>{}</summary>", caption)?;
        write!(f, "<div class='stack-trie'>")?;
        write!(f, "<ul>")?;
        self.fmt_inner(&mut f, metrics_index)?;
        write!(f, "</ul>")?;
        write!(f, "</div>")?;
        write!(f, "</details>")?;
        Ok(f)
    }

    fn fmt_inner(
        &self,
        f: &mut String,
        mb_metrics_index: Option<&CompilationMetricsIndex>,
    ) -> std::fmt::Result {
        for (frame, node) in self.children.iter() {
            let mut star = String::new();
            for t in &node.terminal {
                if let Some(c) = t {
                    let ok_class = mb_metrics_index.map_or("status-missing", |metrics_index| {
                        metrics_index.get(t).map_or("status-missing", |m| {
                            if m.iter().any(|n| n.fail_type.is_some()) {
                                "status-error"
                            } else if m.iter().any(|n| n.graph_op_count.unwrap_or(0) == 0) {
                                "status-empty"
                            } else if m
                                .iter()
                                .any(|n| !n.restart_reasons.as_ref().map_or(false, |o| o.is_empty()))
                            {
                                "status-break"
                            } else {
                                "status-ok"
                            }
                        })
                    });
                    write!(
                        star,
                        "<a href='#{cid}' class='{ok_class}'>{cid}</a> ",
                        cid = c,
                        ok_class = ok_class
                    )?;
                } else {
                    write!(star, "(unknown) ")?;
                }
            }

            let frame_html = format_frame(frame);

            if self.children.len() > 1 {
                writeln!(
                    f,
                    "<li><span onclick='toggleList(this)' class='marker'></span>{star}",
                    star = star
                )?;
                writeln!(f, "{}<ul>", frame_html)?;
                node.fmt_inner(f, mb_metrics_index)?;
                write!(f, "</ul></li>")?;
            } else {
                writeln!(f, "<li>{star}{}</li>", frame_html, star = star)?;
                node.fmt_inner(f, mb_metrics_index)?;
            }
        }
        Ok(())
    }
}

fn format_frame(frame: &FrameKey) -> String {
    let filename = simplify_filename(&frame.filename);

    // Check for eval_with_key pattern
    if let Some(fx_id) = extract_eval_with_key_id(&frame.filename) {
        format!(
            "<a href='dump_file/eval_with_key_{fx_id}.html#L{line}'>{filename}:{line}</a> in {name}",
            fx_id = fx_id,
            filename = encode_text(filename),
            line = frame.line,
            name = encode_text(&frame.name)
        )
    } else {
        let loc_str = frame
            .loc
            .as_ref()
            .map(|l| format!("<br>&nbsp;&nbsp;&nbsp;&nbsp;{}", encode_text(l)))
            .unwrap_or_default();
        format!(
            "{}:{} in {}{}",
            encode_text(filename),
            frame.line,
            encode_text(&frame.name),
            loc_str
        )
    }
}

fn extract_eval_with_key_id(filename: &str) -> Option<u64> {
    use regex::Regex;
    let re = Regex::new(r"<eval_with_key>\.([0-9]+)").ok()?;
    re.captures(filename)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse().ok())
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
            files: vec!["stacks.jsonl".to_string()],
        }
    }

    #[test]
    fn test_stack_trie_module() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create stacks.jsonl with a dynamo_start entry
        let stacks_path = temp_dir.path().join("stacks.jsonl");
        let mut file = File::create(&stacks_path)?;
        writeln!(
            file,
            r#"{{"type":"dynamo_start","compile_id":"0_0","rank":0,"timestamp":"2024-01-01T00:00:00Z","thread":1,"pathname":"test.py","lineno":1,"metadata":{{"stack":[{{"filename":0,"line":10,"name":"forward","loc":"model.py"}}]}}}}"#
        )?;

        // Create empty compilation_metrics.jsonl
        let metrics_path = temp_dir.path().join("compilation_metrics.jsonl");
        File::create(&metrics_path)?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = StackTrieModule::new();
        let output = module.render(&ctx)?;

        // Should have an index contribution with stack trie HTML
        assert!(output.index_contribution.is_some());
        let contribution = output.index_contribution.unwrap();
        assert_eq!(contribution.section, "Stack Trie");
        assert!(contribution.html.contains("stack-trie"));

        Ok(())
    }

    #[test]
    fn test_empty_stacks() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let manifest = create_test_manifest();
        let config = ModuleConfig::default();

        // Create empty stacks.jsonl
        let stacks_path = temp_dir.path().join("stacks.jsonl");
        File::create(&stacks_path)?;

        // Create empty compilation_metrics.jsonl
        let metrics_path = temp_dir.path().join("compilation_metrics.jsonl");
        File::create(&metrics_path)?;

        let ctx =
            crate::modules::context::ModuleContext::new(temp_dir.path(), temp_dir.path(), &manifest, &config);
        let module = StackTrieModule::new();
        let output = module.render(&ctx)?;

        // Should have no index contribution when empty
        assert!(output.index_contribution.is_none());

        Ok(())
    }
}
