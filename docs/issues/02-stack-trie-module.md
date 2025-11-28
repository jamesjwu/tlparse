# Sub-Issue #2: StackTrieModule Implementation

## Summary
Convert the stack trie generation from `lib.rs` into a standalone eager module.

## Current Implementation Location
- `lib.rs`: `StackTrieNode` struct and trie building logic
- `types.rs`: `StackTrieNode`, `FrameSummary`, `StackSummary` types
- `templates.rs`: `TEMPLATE_INDEX` with stack trie rendering

## Tasks

### 2.1 Create `src/modules/stack_trie.rs`
- Implement `StackTrieModule` struct
- Move `StackTrieNode` rendering logic to module

### 2.2 Subscribe to `compilation_metrics.jsonl`
- Filter for `dynamo_start` entries (which contain stack traces)
- Build trie from stack data

### 2.3 Generate Output
- Render stack trie HTML
- Provide as `IndexEntry` for the index module

### 2.4 Handle Metrics Association
- Associate compilation metrics with trie terminal nodes
- Show success/failure status in trie

## Intermediate File Usage

```rust
impl Module for StackTrieModule {
    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[IntermediateFileType::CompilationMetrics]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager // Stack trie must be pre-rendered
    }
}
```

## Data Flow
```
compilation_metrics.jsonl
  └─ filter(type == "dynamo_start")
      └─ extract stack traces
          └─ build StackTrieNode tree
              └─ render to HTML
```

## Changes to Intermediate Format
Consider adding pre-computed stack trie to manifest for optimization:
```json
{
  "stack_trie_data": {
    "nodes": [...],
    "terminals": {...}
  }
}
```

## Acceptance Criteria
- [ ] Stack trie renders identically to current implementation
- [ ] Works with compilation metrics association
- [ ] Handles edge cases (empty stacks, unknown compile IDs)

## Lazy Loading Opportunity
While the trie structure must be eager, individual terminal details could be lazy:
- Show collapsed trie eagerly
- Load compilation metrics details on expansion

## Estimated Complexity
Medium - Core functionality, but well-defined.
