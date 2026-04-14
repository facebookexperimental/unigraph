# Next.js Turbopack Analyze Data Format

> Output of `next experimental-analyze -o`, read by `unigraph_turbopack`.

## How to Generate

```bash
cd your-next-app
./node_modules/.bin/next experimental-analyze -o
# Produces: .next/diagnostics/analyze/data/
```

## Directory Layout

```
.next/diagnostics/analyze/data/
├── modules.data                    # Global module dependency graph (binary)
├── routes.json                     # List of all routes
├── analyze.data                    # Size data for route "/"
├── about/
│   └── analyze.data                # Size data for route "/about"
├── dashboard/
│   └── analyze.data                # Size data for route "/dashboard"
└── blog/
    └── [slug]/
        └── analyze.data            # Size data for route "/blog/[slug]"
```

There are exactly three kinds of files. Everything else is directory structure mirroring the route tree.

---

## 1. `routes.json`

A flat JSON array of route path strings:

```json
["/", "/about", "/dashboard", "/blog/[slug]"]
```

Each entry maps to a subdirectory containing an `analyze.data` file. The root route `"/"` maps to `analyze.data` directly (not inside a subdirectory).

---

## 2. `modules.data` — The Global Module Graph

This is the **single most important file**. It contains every module Turbopack discovered across all routes and all RSC layers, plus the full dependency graph between them.

### Binary Envelope Format

Both `modules.data` and `analyze.data` share this envelope:

```
┌─────────────────────────────────────────┐
│ 4 bytes: JSON header length (BE u32)    │
├─────────────────────────────────────────┤
│ N bytes: JSON header (UTF-8)            │
├─────────────────────────────────────────┤
│ Remaining bytes: binary edges section   │
└─────────────────────────────────────────┘
```

Read the first 4 bytes as a big-endian `u32` to get `N`, then read `N` bytes of JSON, then the rest is binary edge data.

### JSON Header

```json
{
  "modules": [
    { "ident": "[project]/src/app/page.tsx [app-rsc] (ecmascript)", "path": "[project]/src/app/page.tsx" },
    { "ident": "[project]/src/app/layout.tsx [app-rsc] (ecmascript)", "path": "[project]/src/app/layout.tsx" },
    { "ident": "[project]/node_modules/react/index.js [app-client] (ecmascript)", "path": "[project]/node_modules/react/index.js" },
    { "ident": "[project]/src/utils.ts [app-rsc] (ecmascript) <exports>", "path": "[project]/src/utils.ts" }
  ],
  "module_dependencies":       { "offset": 0,    "length": 1024 },
  "async_module_dependencies":  { "offset": 1024, "length": 512  },
  "module_dependents":          { "offset": 1536, "length": 1024 },
  "async_module_dependents":    { "offset": 2560, "length": 512  }
}
```

**`modules`** — Ordered array of every module. The index in this array is the module's ID, used by the edge data. Each module has:

- **`ident`** — Full turbopack module identifier (see "Module Ident Format" below)
- **`path`** — The source file path on disk (without layer/type/fragment suffixes)

**`module_dependencies`** / **`async_module_dependencies`** — Edge references pointing into the binary section. These are the outgoing edges: "module X depends on modules Y, Z".

- `module_dependencies` = static `import` / `require` (loaded synchronously, in the same chunk group)
- `async_module_dependencies` = dynamic `import()` (loaded on demand, separate chunk group)

**`module_dependents`** / **`async_module_dependents`** — Reverse edges: "module X is depended on by modules Y, Z". These are the inverse of the dependency edges.

Each edge reference is `{ "offset": N, "length": N }` pointing into the binary tail.

### Module Ident Format

```
[project]/src/utils.ts [app-rsc] (ecmascript) <exports>
 ╰──────── path ──────╯ ╰ layer ╯ ╰── type ──╯ ╰fragment╯
```

Parsed right-to-left by peeling bracketed suffixes:

| Segment | Brackets | Values | Meaning |
|---------|----------|--------|---------|
| **fragment** | `<...>` | `exports`, `module evaluation`, `internal part N` | Tree-shaking split. A module can be split into multiple fragments with different dependency edges. |
| **type** | `(...)` | `ecmascript`, `css`, `css/module`, `static` | What kind of module this is. |
| **layer** | `[...]` | `app-rsc`, `app-client`, `app-ssr`, `app-route` | RSC compilation layer (see below). |
| **template args** | `{...}` | (rare) | Template parameters for virtual modules. |
| **path** | remainder | `[project]/src/...`, `[next]/...` | File path with origin prefix. |

**Origin prefixes:**

| Prefix | Meaning |
|--------|---------|
| `[project]/` | Your application code and node_modules |
| `[next]/` | Next.js framework internals |
| `[turbopack]/` | Turbopack runtime modules |

**All segments after path are optional.** A module might be just `[project]/public/favicon.ico (static)` with no layer or fragment.

### RSC Layers

The same source file can appear as **multiple modules** in different layers. Each layer has its own compilation context, resolve rules, and transforms:

| Layer | Environment | Purpose |
|-------|-------------|---------|
| `app-rsc` | Node.js, `react-server` condition | Server Component graph. Files with `"use client"` become thin proxies here. |
| `app-client` | Browser | Client Component code. The actual implementation of `"use client"` files. |
| `app-ssr` | Node.js | Server-side rendering of Client Components. Same code as `app-client` but compiled for Node. |
| `app-route` | Node.js | API route handlers (`route.ts` files). |

**Example:** `Button.tsx` with `"use client"` at the top appears as:

- `[project]/src/Button.tsx [app-rsc] (ecmascript)` — A proxy that calls `registerClientReference()` for each export
- `[project]/src/Button.tsx [app-client] (ecmascript)` — The real implementation with React hooks, event handlers, etc.
- `[project]/src/Button.tsx [app-ssr] (ecmascript)` — Same implementation but compiled for server-side rendering

These three modules have **completely different dependency edges**. The RSC proxy depends on the client reference system; the client version depends on React, other components, etc.

### Tree-Shaking Fragments

When Turbopack's tree-shaking analysis determines that a module's exports can be split, it creates multiple fragment modules:

| Fragment | Meaning |
|----------|---------|
| `<exports>` | Only the exported bindings (and their transitive dependencies within the file) |
| `<module evaluation>` | Side-effect code that runs when the module is first imported |
| `<internal part N>` | Internal code blocks that are independently shakeable |

Fragments have **genuinely different edges**. `utils.ts <exports>` might only depend on a few helper functions, while `utils.ts <module evaluation>` might pull in a logging library for its top-level initialization.

**Note:** Not every module gets fragmented. Turbopack only fragments modules where it can prove the splits are independent. Most modules appear as a single entry.

### Binary Edge Encoding

Each `{ "offset": N, "length": N }` reference points to a section of the binary tail that encodes adjacency lists for all modules:

```
┌────────────────────────────────────────────────────┐
│ u32 BE: num_nodes                                  │
├────────────────────────────────────────────────────┤
│ u32 BE × num_nodes: cumulative end-offsets         │
│   offsets[0] = number of edges for node 0          │
│   offsets[1] = offsets[0] + edges for node 1       │
│   offsets[2] = offsets[1] + edges for node 2       │
│   ...                                              │
├────────────────────────────────────────────────────┤
│ u32 BE × total_edges: edge target indices          │
│   (indices into the modules array)                 │
└────────────────────────────────────────────────────┘
```

To read edges for module `i`:

```
prev = if i == 0 { 0 } else { offsets[i-1] }
curr = offsets[i]
edges = targets[prev..curr]
```

Each target is a `u32` index into the `modules` array.

**Example:** 3 modules, module 0 depends on [1, 2], module 1 depends on [2], module 2 depends on nothing:

```
num_nodes = 3
offsets   = [2, 3, 3]       ← cumulative: 2 edges, then 1 more, then 0 more
targets   = [1, 2, 2]       ← module indices
```

---

## 3. `analyze.data` — Per-Route Size Attribution

Each route gets its own `analyze.data` file containing **size data only** (no dependency edges). This tells you how much each source file contributes to the route's output chunks.

### JSON Header

```json
{
  "sources": [
    { "parent_source_index": null, "path": "[project]/" },
    { "parent_source_index": 0,    "path": "src/" },
    { "parent_source_index": 1,    "path": "app/" },
    { "parent_source_index": 2,    "path": "page.tsx" },
    { "parent_source_index": 1,    "path": "utils.ts" }
  ],
  "chunk_parts": [
    { "source_index": 3, "output_file_index": 0, "size": 1234, "compressed_size": 456 },
    { "source_index": 4, "output_file_index": 0, "size": 567,  "compressed_size": 234 },
    { "source_index": 3, "output_file_index": 1, "size": 89,   "compressed_size": 45  }
  ],
  "output_files": [
    { "filename": "_next/static/chunks/app/page-abc123.js" },
    { "filename": "_next/static/css/app/page-def456.css" }
  ],
  "source_chunk_parts": { "offset": 0, "length": 100 },
  "output_file_chunk_parts": { "offset": 100, "length": 80 },
  "source_children": { "offset": 180, "length": 60 },
  "source_roots": [0]
}
```

### Sources — A Directory Tree

Sources are stored as a **tree** using parent pointers, NOT as full paths. Each source has:

- **`path`** — A single path segment (directory name or filename)
- **`parent_source_index`** — Index of the parent source, or `null` for roots

To reconstruct the full path, walk up the parent chain and concatenate:

```
source[3].path = "page.tsx"
  parent = source[2].path = "app/"
    parent = source[1].path = "src/"
      parent = source[0].path = "[project]/"
        parent = null (root)

Full path: "[project]/src/app/page.tsx"
```

This tree structure mirrors the file system hierarchy of the source files contributing to the route.

### Chunk Parts — Size Attribution

Each chunk part says: "source file X contributed Y bytes (Z compressed) to output file W."

- **`source_index`** — Index into `sources` (walk up to get full path)
- **`output_file_index`** — Index into `output_files` (which chunk file)
- **`size`** — Uncompressed size in bytes (from source map attribution)
- **`compressed_size`** — Estimated gzip-compressed size

**A source can appear in multiple chunk parts** — it might contribute to both a JS chunk and a CSS chunk, or to multiple JS chunks (e.g., shared code that gets duplicated). The same source can also appear in multiple routes' `analyze.data` files.

### Output Files

The actual chunk files that Turbopack writes to disk for this route:

```json
{ "filename": "_next/static/chunks/app/page-abc123.js" }
```

### Binary Edge References

The binary section contains additional adjacency data (same encoding as `modules.data`):

- **`source_chunk_parts`** — For each source, which chunk_parts include it
- **`output_file_chunk_parts`** — For each output file, which chunk_parts it contains
- **`source_children`** — For each source, its children in the directory tree

These are used by the bundle analyzer UI for drill-down navigation but are not needed for Unigraph conversion (we only use `chunk_parts` from the JSON header).

---

## Key Relationships Between Files

```
modules.data                    Per-route analyze.data
╔══════════════╗                ╔══════════════════════╗
║ modules[]    ║                ║ sources[]            ║
║  .ident ─────╫── same path ──╫── .path (tree)       ║
║  .path       ║                ║                      ║
║              ║                ║ chunk_parts[]        ║
║ edges        ║                ║  .source_index       ║
║  (global,    ║                ║  .size               ║
║   all routes)║                ║  .compressed_size    ║
╚══════════════╝                ╚══════════════════════╝
       │                                  │
       │ edges are the SAME               │ sizes DIFFER
       │ regardless of route              │ per route
       │                                  │
       └──────────┬───────────────────────┘
                  │
                  ▼
           Unigraph MapGraph
           (single graph, routes as labels)
```

- **Edges** come from `modules.data` — one global graph shared by all routes
- **Sizes** come from per-route `analyze.data` — aggregated (summed) across routes
- **Route membership** — a module appears in route X if its `path` shows up in that route's `analyze.data` sources
- **Matching key** — `modules.data` modules are matched to `analyze.data` sources by file path (the `path` field in modules, the reconstructed full path in sources)

---

## Size Attribution: How It Works

Turbopack uses **source maps** to attribute output chunk bytes back to source files. During a production build:

1. Each module is compiled and bundled into output chunks
2. Source maps track which output bytes came from which source location
3. `turbopack-analyze` uses these source maps to split each chunk into per-source contributions
4. Compressed sizes are estimated by running deflate on each source's contribution

This means sizes reflect the **actual output size** including:
- Compiled/transformed code (not source size)
- Runtime overhead from module wrapping
- Minification effects

But NOT including:
- Shared chunk overhead (module loader runtime)
- Actual network transfer size (depends on full-file gzip, not per-module)
