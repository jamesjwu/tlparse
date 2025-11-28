# Sub-Issue #11: Client-Side Lazy Loading Framework

## Summary
Implement the JavaScript framework for lazy loading module content in the browser.

## Goals

1. Enable on-demand loading of module content
2. Minimize initial page load time
3. Cache loaded data for repeated access
4. Provide smooth UX with loading indicators

## Tasks

### 12.1 Create `static/modules.js` - Core Loader
- ModuleLoader class for managing data loading
- JSONL parsing utilities
- Caching layer

### 12.2 Create Per-Module Scripts
- `graph_viewer.js` - Graph rendering
- `codegen.js` - Code highlighting
- `guards.js` - Guard table with filtering
- `metrics.js` - Compilation metrics display
- `symbolic_shapes.js` - Expression tree rendering

### 12.3 Index Page Integration
- Lazy container setup
- Click handlers for expand/collapse
- Progress indicators

### 12.4 Client-Side Syntax Highlighting
- Include highlight.js for code highlighting
- Python syntax support
- Custom theme matching tlparse style

## Core Implementation

```javascript
// static/modules.js

class ModuleLoader {
    constructor() {
        this.manifest = null;
        this.jsonlCache = new Map();
        this.renderCache = new Map();
    }

    async init() {
        const resp = await fetch('manifest.json');
        this.manifest = await resp.json();
        return this;
    }

    // Load and parse a JSONL file
    async loadJsonl(filename) {
        if (this.jsonlCache.has(filename)) {
            return this.jsonlCache.get(filename);
        }

        const resp = await fetch(filename);
        const text = await resp.text();

        const entries = text
            .split('\n')
            .filter(line => line.trim())
            .map(line => JSON.parse(line));

        this.jsonlCache.set(filename, entries);
        return entries;
    }

    // Get entries for a specific compile ID from a file
    async getEntriesForCompile(fileType, compileId) {
        const fileInfo = this.manifest.files[fileType];
        if (!fileInfo) return [];

        const entries = await this.loadJsonl(fileInfo.path);
        return entries.filter(e => e.compile_id === compileId);
    }

    // Get entries of a specific type
    async getEntriesByType(fileType, entryType) {
        const fileInfo = this.manifest.files[fileType];
        if (!fileInfo) return [];

        const entries = await this.loadJsonl(fileInfo.path);
        return entries.filter(e => e.type === entryType);
    }
}

// Global loader instance
let loader = null;

async function initLoader() {
    if (!loader) {
        loader = await new ModuleLoader().init();
    }
    return loader;
}

// Utility functions
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function showLoading(container) {
    container.innerHTML = '<div class="loading"><span class="spinner"></span> Loading...</div>';
}

function showError(container, message) {
    container.innerHTML = `<div class="error">${escapeHtml(message)}</div>`;
}

// Export for module scripts
window.tlparse = { loader, initLoader, escapeHtml, showLoading, showError };
```

## Graph Viewer Script

```javascript
// static/graph_viewer.js

async function renderGraph(containerId, compileId, graphType) {
    const container = document.getElementById(containerId);
    tlparse.showLoading(container);

    try {
        await tlparse.initLoader();
        const entries = await tlparse.loader.getEntriesForCompile('graphs', compileId);
        const graph = entries.find(e => e.type === graphType);

        if (graph && graph.payload) {
            container.innerHTML = `
                <div class="graph-viewer">
                    <div class="graph-header">
                        <span class="graph-type">${tlparse.escapeHtml(graphType)}</span>
                        <button onclick="copyToClipboard('${containerId}-content')">Copy</button>
                    </div>
                    <pre id="${containerId}-content" class="graph-content">${tlparse.escapeHtml(graph.payload)}</pre>
                </div>
            `;
        } else {
            container.innerHTML = '<div class="not-found">Graph not available</div>';
        }
    } catch (error) {
        tlparse.showError(container, `Failed to load graph: ${error.message}`);
    }
}

function copyToClipboard(elementId) {
    const element = document.getElementById(elementId);
    navigator.clipboard.writeText(element.textContent);
}
```

## Code Highlighting Script

```javascript
// static/codegen.js

async function renderInductorCode(containerId, compileId) {
    const container = document.getElementById(containerId);
    tlparse.showLoading(container);

    try {
        await tlparse.initLoader();
        const entries = await tlparse.loader.getEntriesForCompile('codegen', compileId);
        const codeEntry = entries.find(e => e.type === 'inductor_output_code');

        if (codeEntry && codeEntry.payload) {
            const pre = document.createElement('pre');
            const code = document.createElement('code');
            code.className = 'language-python';
            code.textContent = codeEntry.payload;
            pre.appendChild(code);

            container.innerHTML = '';
            container.appendChild(pre);

            // Apply syntax highlighting
            if (window.hljs) {
                hljs.highlightElement(code);
            }
        } else {
            container.innerHTML = '<div class="not-found">No output code available</div>';
        }
    } catch (error) {
        tlparse.showError(container, `Failed to load code: ${error.message}`);
    }
}
```

## Index Page Template Updates

```html
<!-- In index.html -->
<head>
    <!-- ... -->
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github.min.css">
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/languages/python.min.js"></script>
    <script src="modules.js"></script>
    <script src="graph_viewer.js"></script>
    <script src="codegen.js"></script>
    <!-- ... other module scripts -->
</head>

<body>
    <!-- Compile entry with lazy content -->
    <div class="compile-entry" data-compile-id="0_0_0">
        <div class="compile-header" onclick="toggleCompile('0_0_0')">
            <span class="compile-id">0/0</span>
            <span class="compile-status success">Success</span>
        </div>
        <div class="compile-details" id="compile-0_0_0-details" style="display:none">
            <!-- Lazy loaded content sections -->
            <div class="section" id="graphs-0_0_0"></div>
            <div class="section" id="code-0_0_0"></div>
            <div class="section" id="metrics-0_0_0"></div>
        </div>
    </div>

    <script>
        async function toggleCompile(compileId) {
            const details = document.getElementById(`compile-${compileId}-details`);

            if (details.style.display === 'none') {
                details.style.display = 'block';

                // Load content if not already loaded
                if (!details.dataset.loaded) {
                    await Promise.all([
                        renderGraph(`graphs-${compileId}`, compileId, 'dynamo_output_graph'),
                        renderInductorCode(`code-${compileId}`, compileId),
                    ]);
                    details.dataset.loaded = 'true';
                }
            } else {
                details.style.display = 'none';
            }
        }
    </script>
</body>
```

## CSS for Loading States

```css
/* static/lazy-loading.css */

.loading {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 20px;
    color: #666;
}

.spinner {
    width: 20px;
    height: 20px;
    border: 2px solid #f3f3f3;
    border-top: 2px solid #3498db;
    border-radius: 50%;
    animation: spin 1s linear infinite;
}

@keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
}

.error {
    padding: 20px;
    color: #c00;
    background: #fee;
    border: 1px solid #c00;
    border-radius: 4px;
}

.not-found {
    padding: 20px;
    color: #666;
    font-style: italic;
}

.compile-entry {
    border: 1px solid #ddd;
    margin-bottom: 10px;
    border-radius: 4px;
}

.compile-header {
    padding: 10px 15px;
    background: #f8f8f8;
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    align-items: center;
}

.compile-header:hover {
    background: #eee;
}

.compile-details {
    padding: 15px;
    border-top: 1px solid #ddd;
}

.section {
    margin-bottom: 20px;
}
```

## Acceptance Criteria
- [ ] ModuleLoader correctly parses JSONL files
- [ ] Caching prevents redundant fetches
- [ ] Loading indicators display during fetch
- [ ] Error states handled gracefully
- [ ] Syntax highlighting works for Python code
- [ ] Smooth expand/collapse UX

## Dependencies
- highlight.js (CDN or bundled)
- Modern browser with fetch API support

## Estimated Complexity
Medium-High - Core infrastructure that other lazy modules depend on.
