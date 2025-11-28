# Sub-Issue #12: MultiRankModule Implementation

## Summary
Handle multi-rank analysis for distributed training scenarios. This module remains eager due to the complexity of cross-rank comparisons.

## Current Implementation Location
- `cli.rs`: Multi-rank orchestration and landing page generation
- `parsers.rs`: `read_collective_schedules`, `read_runtime_estimations`, `read_tensor_meta_fingerprints`, `check_collectives_parity`
- `lib.rs`: Divergence detection, execution order analysis

## Multi-Rank Features

1. **Compile ID Divergence** - Detect when ranks compile different graphs
2. **Runtime Estimation** - Compare estimated runtimes across ranks
3. **Collective Schedules** - Analyze collective operation ordering
4. **Tensor Metadata Fingerprints** - Detect tensor metadata divergence
5. **Execution Order** - Analyze graph execution order across ranks
6. **Collectives Parity** - Check collective counts match

## Tasks

### 13.1 Create `src/modules/multi_rank.rs`
- Implement `MultiRankModule` struct
- Coordinate reading from multiple rank directories

### 13.2 Cross-Rank Analysis Functions
- Divergence detection
- Runtime variance calculation
- Collective schedule comparison
- Execution order analysis

### 13.3 Generate Multi-Rank Index
- Combined landing page
- Per-rank links
- Divergence warnings

### 13.4 Generate Analysis Artifacts
- `collective_schedules.json`
- `runtime_estimations.json`
- `chromium_trace_with_runtime.json`
- Per-rank `collectives_parity.json`

## Module Implementation

```rust
pub struct MultiRankModule;

impl Module for MultiRankModule {
    fn name(&self) -> &'static str {
        "Multi-Rank Analysis"
    }

    fn id(&self) -> &'static str {
        "multi_rank"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        &[
            IntermediateFileType::CompilationMetrics,
            IntermediateFileType::Artifacts,
            IntermediateFileType::TensorMetadata,
            IntermediateFileType::ChromiumEvents,
        ]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager // Cross-rank analysis must be eager
    }
}

impl MultiRankModule {
    pub fn analyze_ranks(
        &self,
        rank_dirs: &[PathBuf],
    ) -> anyhow::Result<MultiRankAnalysis> {
        let mut analysis = MultiRankAnalysis::default();

        // Collect data from each rank
        let rank_data: Vec<RankData> = rank_dirs.iter()
            .map(|dir| self.read_rank_data(dir))
            .collect::<Result<Vec<_>, _>>()?;

        // Detect compile ID divergence
        analysis.compile_id_divergence = self.detect_compile_id_divergence(&rank_data);

        // Compare collective schedules
        analysis.collective_schedule_divergence =
            self.compare_collective_schedules(&rank_data);

        // Compare runtime estimations
        analysis.runtime_variance = self.analyze_runtime_variance(&rank_data);

        // Compare tensor metadata
        analysis.tensor_metadata_divergence =
            self.compare_tensor_metadata(&rank_data);

        // Analyze execution order
        analysis.execution_order_issues =
            self.analyze_execution_order(&rank_data);

        Ok(analysis)
    }

    fn detect_compile_id_divergence(&self, rank_data: &[RankData]) -> Option<DivergenceReport> {
        let first_ids = &rank_data[0].compile_ids;
        let divergent_ranks: Vec<u32> = rank_data.iter()
            .enumerate()
            .skip(1)
            .filter(|(_, data)| &data.compile_ids != first_ids)
            .map(|(i, _)| i as u32)
            .collect();

        if divergent_ranks.is_empty() {
            None
        } else {
            Some(DivergenceReport {
                divergent_ranks,
                expected: first_ids.clone(),
            })
        }
    }

    fn compare_collective_schedules(&self, rank_data: &[RankData]) -> Vec<CollectiveDivergence> {
        // Group ranks by their collective schedule
        let mut schedule_groups: HashMap<Vec<String>, Vec<u32>> = HashMap::new();

        for (rank, data) in rank_data.iter().enumerate() {
            for schedule in &data.collective_schedules {
                schedule_groups
                    .entry(schedule.ops.clone())
                    .or_default()
                    .push(rank as u32);
            }
        }

        // If all ranks have same schedule, no divergence
        if schedule_groups.len() <= 1 {
            return Vec::new();
        }

        // Report divergences
        schedule_groups.into_iter()
            .map(|(ops, ranks)| CollectiveDivergence { ops, ranks })
            .collect()
    }

    fn analyze_runtime_variance(&self, rank_data: &[RankData]) -> RuntimeVarianceReport {
        // Calculate per-graph runtime variance across ranks
        let mut graph_runtimes: HashMap<String, Vec<f64>> = HashMap::new();

        for data in rank_data {
            for runtime in &data.runtimes {
                let total: f64 = runtime.ops.iter()
                    .map(|op| op.estimated_runtime_us.unwrap_or(0.0))
                    .sum();
                graph_runtimes
                    .entry(runtime.graph.clone())
                    .or_default()
                    .push(total);
            }
        }

        RuntimeVarianceReport {
            per_graph_variance: graph_runtimes.into_iter()
                .map(|(graph, times)| {
                    let mean = times.iter().sum::<f64>() / times.len() as f64;
                    let variance = times.iter()
                        .map(|t| (t - mean).powi(2))
                        .sum::<f64>() / times.len() as f64;
                    (graph, variance.sqrt())
                })
                .collect(),
        }
    }
}
```

## Multi-Rank Index Template

```html
<!DOCTYPE html>
<html>
<head>
    <title>Multi-Rank Analysis</title>
    <style>/* ... */</style>
</head>
<body>
    <h1>Multi-Rank Analysis ({{ num_ranks }} ranks)</h1>

    <!-- Divergence Warnings -->
    {% if has_divergence %}
    <section class="warning-banner">
        <h2>⚠️ Divergence Detected</h2>
        <ul>
            {% if compile_id_divergence %}
            <li>Compile ID divergence across ranks</li>
            {% endif %}
            {% if collective_schedule_divergence %}
            <li>Collective schedule divergence</li>
            {% endif %}
            {% if tensor_metadata_divergence %}
            <li>Tensor metadata divergence</li>
            {% endif %}
        </ul>
    </section>
    {% endif %}

    <!-- Per-Rank Links -->
    <section id="rank-links">
        <h2>Per-Rank Reports</h2>
        <ul>
            {% for rank in ranks %}
            <li><a href="rank_{{ rank }}/index.html">Rank {{ rank }}</a></li>
            {% endfor %}
        </ul>
    </section>

    <!-- Downloads -->
    <section id="downloads">
        <h2>Analysis Artifacts</h2>
        <ul>
            <li><a href="collective_schedules.json">Collective Schedules</a></li>
            <li><a href="runtime_estimations.json">Runtime Estimations</a></li>
            <li><a href="chromium_events.json">Combined Chromium Trace</a></li>
        </ul>
    </section>
</body>
</html>
```

## Output Files

| File | Description |
|------|-------------|
| `index.html` | Multi-rank landing page |
| `collective_schedules.json` | All collective schedules |
| `runtime_estimations.json` | Runtime data per graph/rank |
| `chromium_events.json` | Combined trace from all ranks |
| `chromium_trace_with_runtime.json` | Trace with runtime annotations |
| `rank_N/collectives_parity.json` | Per-rank parity check |

## Acceptance Criteria
- [ ] Multi-rank index page generated
- [ ] Divergence detection works
- [ ] Collective schedule comparison correct
- [ ] Runtime variance calculated
- [ ] Per-rank directories linked
- [ ] Combined chromium trace generated

## Estimated Complexity
High - Complex cross-rank analysis with multiple comparison algorithms.
