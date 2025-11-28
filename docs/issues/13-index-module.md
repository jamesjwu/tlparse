# Sub-Issue #13: IndexModule Implementation (Shell Page)

## Summary
Generate the main index.html shell page that aggregates content from all modules.

## Current Implementation Location
- `lib.rs`: Index generation with stack trie and compile directory
- `templates.rs`: `TEMPLATE_INDEX`

## Responsibilities

1. **Aggregate Module Outputs** - Collect index entries from all modules
2. **Render Shell HTML** - Generate minimal HTML that supports lazy loading
3. **Include Static Assets** - CSS, JS files
4. **Copy Intermediate Files** - Make JSONL files accessible for client-side loading

## Tasks

### 14.1 Create `src/modules/index.rs`
- Implement `IndexModule` struct
- Orchestrate other modules

### 14.2 Aggregate Index Content
- Collect `IndexEntry` from all modules
- Organize by section

### 14.3 Generate Shell HTML
- Minimal eager content
- Lazy loading containers
- Include all necessary scripts

### 14.4 Static Asset Management
- CSS files
- JavaScript modules
- Third-party libraries (highlight.js)

## Module Implementation

```rust
pub struct IndexModule {
    modules: Vec<Box<dyn Module>>,
}

impl IndexModule {
    pub fn new(modules: Vec<Box<dyn Module>>) -> Self {
        Self { modules }
    }
}

impl Module for IndexModule {
    fn name(&self) -> &'static str {
        "Index"
    }

    fn id(&self) -> &'static str {
        "index"
    }

    fn subscriptions(&self) -> &[IntermediateFileType] {
        // Index module doesn't directly subscribe to files
        // It aggregates from other modules
        &[]
    }

    fn loading_strategy(&self) -> LoadingStrategy {
        LoadingStrategy::Eager
    }

    fn render(&self, ctx: &ModuleContext) -> anyhow::Result<ModuleOutput> {
        let mut all_index_entries: Vec<IndexEntry> = Vec::new();
        let mut all_files: Vec<(PathBuf, String)> = Vec::new();
        let mut all_lazy_scripts: Vec<PathBuf> = Vec::new();

        // Run all other modules and collect their outputs
        for module in &self.modules {
            let output = module.render(ctx)?;

            all_index_entries.extend(output.index_entries);
            all_files.extend(output.files);
            all_lazy_scripts.extend(output.lazy_scripts);
        }

        // Group index entries by section
        let mut sections: HashMap<IndexSection, Vec<IndexEntry>> = HashMap::new();
        for entry in all_index_entries {
            sections.entry(entry.section.clone()).or_default().push(entry);
        }

        // Generate index HTML
        let html = self.render_index_html(&sections, &all_lazy_scripts, ctx)?;
        all_files.push((PathBuf::from("index.html"), html));

        // Copy intermediate files for client-side access
        self.copy_intermediate_files(ctx, &mut all_files)?;

        // Include static assets
        all_files.push((PathBuf::from("style.css"), CSS.to_string()));
        all_files.push((PathBuf::from("modules.js"), MODULES_JS.to_string()));
        for script in &all_lazy_scripts {
            let script_content = self.get_script_content(script)?;
            all_files.push((script.clone(), script_content));
        }

        Ok(ModuleOutput {
            files: all_files,
            ..Default::default()
        })
    }
}

impl IndexModule {
    fn render_index_html(
        &self,
        sections: &HashMap<IndexSection, Vec<IndexEntry>>,
        scripts: &[PathBuf],
        ctx: &ModuleContext,
    ) -> anyhow::Result<String> {
        let context = IndexContext {
            css: CSS,
            custom_header_html: &ctx.config.custom_header_html,
            stack_trie_html: sections.get(&IndexSection::StackTrie)
                .and_then(|entries| entries.first())
                .map(|e| match &e.content {
                    IndexContent::Html(html) => html.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default(),
            diagnostics: sections.get(&IndexSection::Diagnostics)
                .unwrap_or(&Vec::new())
                .clone(),
            compile_directory: sections.get(&IndexSection::CompileDirectory)
                .unwrap_or(&Vec::new())
                .clone(),
            downloads: sections.get(&IndexSection::Downloads)
                .unwrap_or(&Vec::new())
                .clone(),
            scripts: scripts.iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
        };

        render_template("index.html", &context)
    }

    fn copy_intermediate_files(
        &self,
        ctx: &ModuleContext,
        files: &mut Vec<(PathBuf, String)>,
    ) -> anyhow::Result<()> {
        // Copy JSONL files for client-side lazy loading
        for file_type in IntermediateFileType::all() {
            let filename = file_type.filename();
            let src_path = ctx.intermediate_dir.join(filename);
            if src_path.exists() {
                let content = std::fs::read_to_string(&src_path)?;
                files.push((PathBuf::from(filename), content));
            }
        }

        // Copy manifest
        let manifest_content = std::fs::read_to_string(
            ctx.intermediate_dir.join("manifest.json")
        )?;
        files.push((PathBuf::from("manifest.json"), manifest_content));

        Ok(())
    }
}
```

## Index HTML Template

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>tlparse Report</title>
    <link rel="stylesheet" href="style.css">
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github.min.css">
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/languages/python.min.js"></script>
    <script src="modules.js"></script>
    {% for script in scripts %}
    <script src="{{ script }}"></script>
    {% endfor %}
</head>
<body>
    <header>
        <h1>tlparse Compilation Report</h1>
        {{ custom_header_html | format_unescaped }}
    </header>

    <!-- Stack Trie (always eager) -->
    <section id="stack-trie">
        <h2>Compilation Stack</h2>
        {{ stack_trie_html | format_unescaped }}
    </section>

    <!-- Diagnostics -->
    <section id="diagnostics">
        <h2>Diagnostics</h2>
        {% for entry in diagnostics %}
        <div class="diagnostic-item">
            <span class="label">{{ entry.title }}:</span>
            {% match entry.content %}
            {% when IndexContent::Html(html) %}
            {{ html | format_unescaped }}
            {% when IndexContent::Link(url) %}
            <a href="{{ url }}">View</a>
            {% when IndexContent::Hybrid { summary_html, detail_url } %}
            {{ summary_html | format_unescaped }}
            <a href="{{ detail_url }}">Details</a>
            {% endmatch %}
        </div>
        {% endfor %}
    </section>

    <!-- Compile Directory -->
    <section id="compile-directory">
        <h2>Compilations</h2>
        <div id="compile-list">
            <!-- Populated by CompileDirectoryModule -->
            {{ compile_directory_html | format_unescaped }}
        </div>
    </section>

    <!-- Downloads -->
    <section id="downloads">
        <h2>Downloads</h2>
        <ul>
            {% for entry in downloads %}
            <li>
                <a href="{{ entry.content.url }}" download>{{ entry.title }}</a>
            </li>
            {% endfor %}
        </ul>
    </section>

    <script>
        // Initialize lazy loading
        document.addEventListener('DOMContentLoaded', async () => {
            await tlparse.initLoader();
        });
    </script>
</body>
</html>
```

## Section Ordering

1. Header with custom HTML
2. Stack Trie (eager, collapsible)
3. Diagnostics (failures summary, etc.)
4. Compile Directory (lazy-expandable entries)
5. Downloads (chromium trace, raw files)

## Acceptance Criteria
- [ ] Index page generated with all sections
- [ ] Stack trie rendered (from StackTrieModule)
- [ ] Diagnostics section populated
- [ ] Compile directory entries listed
- [ ] Download links work
- [ ] Lazy loading scripts included
- [ ] Intermediate files copied for client access
- [ ] Custom header HTML supported

## Estimated Complexity
Medium - Orchestration role, depends on all other modules.
