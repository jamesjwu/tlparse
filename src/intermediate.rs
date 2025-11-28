//! Intermediate JSON file generation for modular tlparse architecture.
//!
//! This module generates organized JSON files from parsed PyTorch structured logs,
//! enabling a two-stage architecture where parsing is separated from rendering.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::types::{CompileId, Envelope};

/// Categories of intermediate files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntermediateFileType {
    Graphs,
    Codegen,
    Guards,
    CompilationMetrics,
    ChromiumEvents,
    Artifacts,
    TensorMetadata,
    Export,
}

impl IntermediateFileType {
    pub fn filename(&self) -> &'static str {
        match self {
            IntermediateFileType::Graphs => "graphs.jsonl",
            IntermediateFileType::Codegen => "codegen.jsonl",
            IntermediateFileType::Guards => "guards.jsonl",
            IntermediateFileType::CompilationMetrics => "compilation_metrics.jsonl",
            IntermediateFileType::ChromiumEvents => "chromium_events.json",
            IntermediateFileType::Artifacts => "artifacts.jsonl",
            IntermediateFileType::TensorMetadata => "tensor_metadata.jsonl",
            IntermediateFileType::Export => "export.jsonl",
        }
    }

    pub fn all() -> &'static [IntermediateFileType] {
        &[
            IntermediateFileType::Graphs,
            IntermediateFileType::Codegen,
            IntermediateFileType::Guards,
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::ChromiumEvents,
            IntermediateFileType::Artifacts,
            IntermediateFileType::TensorMetadata,
            IntermediateFileType::Export,
        ]
    }
}

/// Determines which intermediate file an envelope type belongs to
pub fn envelope_type_to_file(envelope_type: &str) -> Option<IntermediateFileType> {
    match envelope_type {
        // Graphs
        "dynamo_output_graph"
        | "optimize_ddp_split_graph"
        | "optimize_ddp_split_child"
        | "compiled_autograd_graph"
        | "aot_forward_graph"
        | "aot_backward_graph"
        | "aot_inference_graph"
        | "aot_joint_graph"
        | "inductor_pre_grad_graph"
        | "inductor_post_grad_graph"
        | "graph_dump" => Some(IntermediateFileType::Graphs),

        // Codegen
        "inductor_output_code" | "dynamo_cpp_guards_str" => Some(IntermediateFileType::Codegen),

        // Guards (includes symbolic shapes)
        "dynamo_guards"
        | "symbolic_shape_specialization"
        | "guard_added_fast"
        | "propagate_real_tensors_provenance"
        | "guard_added"
        | "create_unbacked_symbol"
        | "expression_created" => Some(IntermediateFileType::Guards),

        // Compilation metrics (includes stacks)
        "compilation_metrics"
        | "bwd_compilation_metrics"
        | "aot_autograd_backward_compilation_metrics"
        | "dynamo_start"
        | "stack" => Some(IntermediateFileType::CompilationMetrics),

        // Chromium events
        "chromium_event" => Some(IntermediateFileType::ChromiumEvents),

        // Artifacts
        "artifact" | "dump_file" | "link" => Some(IntermediateFileType::Artifacts),

        // Tensor metadata
        "describe_tensor" | "describe_storage" | "describe_source" => {
            Some(IntermediateFileType::TensorMetadata)
        }

        // Export
        "missing_fake_kernel" | "mismatched_fake_kernel" | "exported_program" => {
            Some(IntermediateFileType::Export)
        }

        // Internal types that don't go to intermediate files
        "str" => None,

        // Unknown types - skip
        _ => None,
    }
}

/// An entry in an intermediate JSONL file
#[derive(Debug, Serialize, Deserialize)]
pub struct IntermediateEntry {
    /// The original envelope type name
    #[serde(rename = "type")]
    pub entry_type: String,

    /// Compile ID as a string (e.g., "0_0_0" or null)
    pub compile_id: Option<String>,

    /// Rank number for distributed training
    pub rank: Option<u32>,

    /// ISO-8601 timestamp
    pub timestamp: String,

    /// Thread ID
    pub thread: u64,

    /// Source file pathname
    pub pathname: String,

    /// Line number in source
    pub lineno: u64,

    /// Type-specific metadata
    pub metadata: Value,

    /// Inlined payload content (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// Manifest file describing the intermediate output
#[derive(Debug, Serialize, Deserialize)]
pub struct IntermediateManifest {
    pub version: String,
    pub generated_at: String,
    pub source_file: String,
    pub source_file_hash: Option<String>,
    pub total_envelopes: u64,
    pub envelope_counts: HashMap<String, u64>,
    pub compile_ids: Vec<String>,
    pub string_table_entries: u64,
    pub parse_mode: String,
    pub ranks: Vec<u32>,
    pub files: Vec<String>,
}

/// Writer that manages multiple intermediate JSONL files
pub struct IntermediateWriter {
    output_dir: PathBuf,
    writers: HashMap<IntermediateFileType, BufWriter<File>>,
    chromium_events: Vec<Value>,
    envelope_counts: HashMap<String, u64>,
    compile_ids: std::collections::HashSet<String>,
    ranks: std::collections::HashSet<u32>,
    total_envelopes: u64,
}

impl IntermediateWriter {
    /// Create a new IntermediateWriter that writes to the given directory
    pub fn new(output_dir: &Path) -> Result<Self> {
        fs::create_dir_all(output_dir)?;

        let mut writers = HashMap::new();

        // Create writers for all JSONL files (not chromium_events.json which is array format)
        for file_type in IntermediateFileType::all() {
            if *file_type != IntermediateFileType::ChromiumEvents {
                let path = output_dir.join(file_type.filename());
                let file = File::create(&path)?;
                writers.insert(*file_type, BufWriter::new(file));
            }
        }

        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            writers,
            chromium_events: Vec::new(),
            envelope_counts: HashMap::new(),
            compile_ids: std::collections::HashSet::new(),
            ranks: std::collections::HashSet::new(),
            total_envelopes: 0,
        })
    }

    /// Write an entry to the appropriate intermediate file
    pub fn write_entry(
        &mut self,
        entry: IntermediateEntry,
        file_type: IntermediateFileType,
    ) -> Result<()> {
        self.total_envelopes += 1;

        // Track envelope type counts
        *self
            .envelope_counts
            .entry(entry.entry_type.clone())
            .or_insert(0) += 1;

        // Track compile IDs
        if let Some(ref cid) = entry.compile_id {
            self.compile_ids.insert(cid.clone());
        }

        // Track ranks
        if let Some(rank) = entry.rank {
            self.ranks.insert(rank);
        }

        // Special handling for chromium events (array format)
        if file_type == IntermediateFileType::ChromiumEvents {
            // For chromium events, we store the metadata directly (it's the event data)
            self.chromium_events.push(entry.metadata);
            return Ok(());
        }

        // Write to JSONL file
        if let Some(writer) = self.writers.get_mut(&file_type) {
            let json = serde_json::to_string(&entry)?;
            writeln!(writer, "{}", json)?;
        }

        Ok(())
    }

    /// Write a chromium event directly (for events that come as raw JSON)
    pub fn write_chromium_event(&mut self, event: Value) -> Result<()> {
        self.total_envelopes += 1;
        *self
            .envelope_counts
            .entry("chromium_event".to_string())
            .or_insert(0) += 1;
        self.chromium_events.push(event);
        Ok(())
    }

    /// Write the string table to string_table.json
    pub fn write_string_table(&self, string_table: &HashMap<u32, String>) -> Result<()> {
        let path = self.output_dir.join("string_table.json");
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, string_table)?;
        Ok(())
    }

    /// Finalize writing and generate manifest
    pub fn finalize(
        mut self,
        source_file: &str,
        parse_mode: &str,
        string_table_entries: u64,
    ) -> Result<IntermediateManifest> {
        // Flush all writers
        for (_, writer) in self.writers.iter_mut() {
            writer.flush()?;
        }

        // Write chromium_events.json as array
        let chromium_path = self.output_dir.join("chromium_events.json");
        let chromium_file = File::create(chromium_path)?;
        serde_json::to_writer(chromium_file, &self.chromium_events)?;

        // Collect files that have content
        let mut files: Vec<String> = Vec::new();
        for file_type in IntermediateFileType::all() {
            let filename = file_type.filename();
            let path = self.output_dir.join(filename);
            if path.exists() {
                let metadata = fs::metadata(&path)?;
                // Include file if it has content (JSONL files) or is chromium_events.json
                if metadata.len() > 0 || *file_type == IntermediateFileType::ChromiumEvents {
                    files.push(filename.to_string());
                }
            }
        }

        // Create manifest
        let manifest = IntermediateManifest {
            version: "2.0".to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            source_file: source_file.to_string(),
            source_file_hash: None, // TODO: compute hash
            total_envelopes: self.total_envelopes,
            envelope_counts: self.envelope_counts,
            compile_ids: {
                let mut ids: Vec<_> = self.compile_ids.into_iter().collect();
                ids.sort();
                ids
            },
            string_table_entries,
            parse_mode: parse_mode.to_string(),
            ranks: {
                let mut ranks: Vec<_> = self.ranks.into_iter().collect();
                ranks.sort();
                ranks
            },
            files,
        };

        // Write manifest
        let manifest_path = self.output_dir.join("manifest.json");
        let manifest_file = File::create(manifest_path)?;
        serde_json::to_writer_pretty(manifest_file, &manifest)?;

        Ok(manifest)
    }
}

/// Helper to format a CompileId as a string for intermediate files
pub fn format_compile_id(compile_id: &Option<CompileId>) -> Option<String> {
    compile_id.as_ref().map(|cid| {
        let prefix = if let Some(caid) = cid.compiled_autograd_id {
            format!("!{}_", caid)
        } else {
            String::new()
        };

        let frame = cid.frame_id.map(|f| f.to_string()).unwrap_or_default();
        let frame_compile = cid
            .frame_compile_id
            .map(|f| f.to_string())
            .unwrap_or_default();
        let attempt = cid.attempt.map(|a| format!("_{}", a)).unwrap_or_default();

        format!("{}{}_{}{}", prefix, frame, frame_compile, attempt)
    })
}

/// Detect which envelope type is present in an Envelope struct
pub fn detect_envelope_type(e: &Envelope) -> Option<&'static str> {
    // Check each field in order of typical frequency
    if e.dynamo_output_graph.is_some() {
        return Some("dynamo_output_graph");
    }
    if e.compilation_metrics.is_some() {
        return Some("compilation_metrics");
    }
    if e.dynamo_guards.is_some() {
        return Some("dynamo_guards");
    }
    if e.inductor_output_code.is_some() {
        return Some("inductor_output_code");
    }
    if e.chromium_event.is_some() {
        return Some("chromium_event");
    }
    if e.dynamo_start.is_some() {
        return Some("dynamo_start");
    }
    if e.aot_forward_graph.is_some() {
        return Some("aot_forward_graph");
    }
    if e.aot_backward_graph.is_some() {
        return Some("aot_backward_graph");
    }
    if e.aot_joint_graph.is_some() {
        return Some("aot_joint_graph");
    }
    if e.aot_inference_graph.is_some() {
        return Some("aot_inference_graph");
    }
    if e.inductor_pre_grad_graph.is_some() {
        return Some("inductor_pre_grad_graph");
    }
    if e.inductor_post_grad_graph.is_some() {
        return Some("inductor_post_grad_graph");
    }
    if e.optimize_ddp_split_graph.is_some() {
        return Some("optimize_ddp_split_graph");
    }
    if e.optimize_ddp_split_child.is_some() {
        return Some("optimize_ddp_split_child");
    }
    if e.compiled_autograd_graph.is_some() {
        return Some("compiled_autograd_graph");
    }
    if e.graph_dump.is_some() {
        return Some("graph_dump");
    }
    if e.dynamo_cpp_guards_str.is_some() {
        return Some("dynamo_cpp_guards_str");
    }
    if e.bwd_compilation_metrics.is_some() {
        return Some("bwd_compilation_metrics");
    }
    if e.aot_autograd_backward_compilation_metrics.is_some() {
        return Some("aot_autograd_backward_compilation_metrics");
    }
    if e.symbolic_shape_specialization.is_some() {
        return Some("symbolic_shape_specialization");
    }
    if e.guard_added_fast.is_some() {
        return Some("guard_added_fast");
    }
    if e.propagate_real_tensors_provenance.is_some() {
        return Some("propagate_real_tensors_provenance");
    }
    if e.guard_added.is_some() {
        return Some("guard_added");
    }
    if e.create_unbacked_symbol.is_some() {
        return Some("create_unbacked_symbol");
    }
    if e.expression_created.is_some() {
        return Some("expression_created");
    }
    if e.artifact.is_some() {
        return Some("artifact");
    }
    if e.dump_file.is_some() {
        return Some("dump_file");
    }
    if e.link.is_some() {
        return Some("link");
    }
    if e.describe_tensor.is_some() {
        return Some("describe_tensor");
    }
    if e.describe_storage.is_some() {
        return Some("describe_storage");
    }
    if e.describe_source.is_some() {
        return Some("describe_source");
    }
    if e.missing_fake_kernel.is_some() {
        return Some("missing_fake_kernel");
    }
    if e.mismatched_fake_kernel.is_some() {
        return Some("mismatched_fake_kernel");
    }
    if e.exported_program.is_some() {
        return Some("exported_program");
    }
    if e.str.is_some() {
        return Some("str");
    }
    // Check for standalone stack
    if e.stack.is_some() && e.dynamo_start.is_none() {
        return Some("stack");
    }

    None
}

/// Extract metadata as serde_json::Value for an envelope based on its type
pub fn extract_metadata(e: &Envelope, envelope_type: &str) -> Value {
    match envelope_type {
        "dynamo_output_graph" => {
            serde_json::to_value(&e.dynamo_output_graph).unwrap_or(Value::Null)
        }
        "compilation_metrics" => {
            serde_json::to_value(&e.compilation_metrics).unwrap_or(Value::Null)
        }
        "bwd_compilation_metrics" => {
            serde_json::to_value(&e.bwd_compilation_metrics).unwrap_or(Value::Null)
        }
        "aot_autograd_backward_compilation_metrics" => {
            serde_json::to_value(&e.aot_autograd_backward_compilation_metrics)
                .unwrap_or(Value::Null)
        }
        "dynamo_start" => {
            // Include the stack in metadata
            let mut metadata =
                serde_json::to_value(&e.dynamo_start).unwrap_or(Value::Object(Default::default()));
            if let (Value::Object(ref mut map), Some(stack)) = (&mut metadata, &e.stack) {
                if let Ok(stack_val) = serde_json::to_value(stack) {
                    map.insert("stack".to_string(), stack_val);
                }
            }
            metadata
        }
        "stack" => serde_json::to_value(&e.stack).unwrap_or(Value::Null),
        "dynamo_guards"
        | "optimize_ddp_split_graph"
        | "compiled_autograd_graph"
        | "aot_forward_graph"
        | "aot_backward_graph"
        | "aot_inference_graph"
        | "aot_joint_graph"
        | "inductor_pre_grad_graph"
        | "inductor_post_grad_graph"
        | "dynamo_cpp_guards_str"
        | "chromium_event"
        | "exported_program" => Value::Object(Default::default()),
        "optimize_ddp_split_child" => {
            serde_json::to_value(&e.optimize_ddp_split_child).unwrap_or(Value::Null)
        }
        "graph_dump" => serde_json::to_value(&e.graph_dump).unwrap_or(Value::Null),
        "inductor_output_code" => {
            serde_json::to_value(&e.inductor_output_code).unwrap_or(Value::Null)
        }
        "symbolic_shape_specialization" => {
            serde_json::to_value(&e.symbolic_shape_specialization).unwrap_or(Value::Null)
        }
        "guard_added_fast" => serde_json::to_value(&e.guard_added_fast).unwrap_or(Value::Null),
        "propagate_real_tensors_provenance" => {
            serde_json::to_value(&e.propagate_real_tensors_provenance).unwrap_or(Value::Null)
        }
        "guard_added" => serde_json::to_value(&e.guard_added).unwrap_or(Value::Null),
        "create_unbacked_symbol" => {
            serde_json::to_value(&e.create_unbacked_symbol).unwrap_or(Value::Null)
        }
        "expression_created" => serde_json::to_value(&e.expression_created).unwrap_or(Value::Null),
        "artifact" => serde_json::to_value(&e.artifact).unwrap_or(Value::Null),
        "dump_file" => serde_json::to_value(&e.dump_file).unwrap_or(Value::Null),
        "link" => serde_json::to_value(&e.link).unwrap_or(Value::Null),
        "describe_tensor" => serde_json::to_value(&e.describe_tensor).unwrap_or(Value::Null),
        "describe_storage" => serde_json::to_value(&e.describe_storage).unwrap_or(Value::Null),
        "describe_source" => serde_json::to_value(&e.describe_source).unwrap_or(Value::Null),
        "missing_fake_kernel" => {
            serde_json::to_value(&e.missing_fake_kernel).unwrap_or(Value::Null)
        }
        "mismatched_fake_kernel" => {
            serde_json::to_value(&e.mismatched_fake_kernel).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_envelope_type_routing() {
        assert_eq!(
            envelope_type_to_file("dynamo_output_graph"),
            Some(IntermediateFileType::Graphs)
        );
        assert_eq!(
            envelope_type_to_file("aot_forward_graph"),
            Some(IntermediateFileType::Graphs)
        );
        assert_eq!(
            envelope_type_to_file("inductor_output_code"),
            Some(IntermediateFileType::Codegen)
        );
        assert_eq!(
            envelope_type_to_file("dynamo_guards"),
            Some(IntermediateFileType::Guards)
        );
        assert_eq!(
            envelope_type_to_file("compilation_metrics"),
            Some(IntermediateFileType::CompilationMetrics)
        );
        assert_eq!(
            envelope_type_to_file("chromium_event"),
            Some(IntermediateFileType::ChromiumEvents)
        );
        assert_eq!(
            envelope_type_to_file("artifact"),
            Some(IntermediateFileType::Artifacts)
        );
        assert_eq!(
            envelope_type_to_file("describe_tensor"),
            Some(IntermediateFileType::TensorMetadata)
        );
        assert_eq!(
            envelope_type_to_file("missing_fake_kernel"),
            Some(IntermediateFileType::Export)
        );
        assert_eq!(envelope_type_to_file("str"), None);
        assert_eq!(envelope_type_to_file("unknown_type"), None);
    }

    #[test]
    fn test_intermediate_writer_basic() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut writer = IntermediateWriter::new(temp_dir.path())?;

        // Write a test entry
        let entry = IntermediateEntry {
            entry_type: "dynamo_output_graph".to_string(),
            compile_id: Some("0_0_0".to_string()),
            rank: Some(0),
            timestamp: "2024-11-28T12:00:00.000000Z".to_string(),
            thread: 12345,
            pathname: "torch/_dynamo/convert_frame.py".to_string(),
            lineno: 456,
            metadata: serde_json::json!({"sizes": {}}),
            payload: Some("class GraphModule...".to_string()),
        };

        writer.write_entry(entry, IntermediateFileType::Graphs)?;

        // Write a chromium event
        writer.write_chromium_event(serde_json::json!({
            "name": "compile",
            "ph": "B",
            "ts": 1234567890
        }))?;

        // Finalize
        let manifest = writer.finalize("test.log", "normal", 0)?;

        assert_eq!(manifest.total_envelopes, 2);
        assert_eq!(
            manifest.envelope_counts.get("dynamo_output_graph"),
            Some(&1)
        );
        assert_eq!(manifest.envelope_counts.get("chromium_event"), Some(&1));
        assert!(manifest.compile_ids.contains(&"0_0_0".to_string()));

        // Verify files were created
        assert!(temp_dir.path().join("manifest.json").exists());
        assert!(temp_dir.path().join("graphs.jsonl").exists());
        assert!(temp_dir.path().join("chromium_events.json").exists());

        Ok(())
    }

    #[test]
    fn test_format_compile_id() {
        use crate::types::CompileId;

        // Simple compile ID
        let cid = Some(CompileId {
            compiled_autograd_id: None,
            frame_id: Some(0),
            frame_compile_id: Some(1),
            attempt: None,
        });
        assert_eq!(format_compile_id(&cid), Some("0_1".to_string()));

        // With attempt
        let cid_attempt = Some(CompileId {
            compiled_autograd_id: None,
            frame_id: Some(0),
            frame_compile_id: Some(1),
            attempt: Some(2),
        });
        assert_eq!(format_compile_id(&cid_attempt), Some("0_1_2".to_string()));

        // With compiled autograd
        let cid_autograd = Some(CompileId {
            compiled_autograd_id: Some(3),
            frame_id: Some(0),
            frame_compile_id: Some(1),
            attempt: None,
        });
        assert_eq!(format_compile_id(&cid_autograd), Some("!3_0_1".to_string()));

        // None
        assert_eq!(format_compile_id(&None), None);
    }
}
