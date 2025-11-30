# Canvas View Overhaul: Relationship Navigation

## Overview

Transform the canvas from a "toy" global visualization into a **relationship navigation tool** centered on local graph views. The core interaction model shifts from "see everything at once" to "explore the neighborhood around any atom and navigate outward."

### Design Principles
1. **Local-first**: Start with a focused view, expand on demand
2. **Semantic-aware**: Connections based on embeddings, not just tags
3. **Visually intuitive**: Connection strength, clustering, and color coding communicate meaning
4. **Integrated**: Entry points throughout the app, not a separate "mode"

---

## Phase 1: Semantic Edge Infrastructure

Pre-compute semantic relationships during the embedding pipeline to avoid O(n²) runtime computation.

### 1.1 Database Schema

Add `semantic_edges` table:

```sql
CREATE TABLE semantic_edges (
  id TEXT PRIMARY KEY,
  source_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
  target_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
  similarity_score REAL NOT NULL,
  source_chunk_index INTEGER,
  target_chunk_index INTEGER,
  created_at TEXT NOT NULL,
  UNIQUE(source_atom_id, target_atom_id)
);

CREATE INDEX idx_semantic_edges_source ON semantic_edges(source_atom_id);
CREATE INDEX idx_semantic_edges_target ON semantic_edges(target_atom_id);
CREATE INDEX idx_semantic_edges_score ON semantic_edges(similarity_score DESC);
```

### 1.2 Edge Computation

**Modify `embedding.rs`** - after embedding generation completes for an atom:

```rust
async fn compute_semantic_edges(
    conn: &Connection,
    atom_id: &str,
    threshold: f32,     // 0.5 default - lower than UI to capture more relationships
    max_edges: i32,     // 15 per atom
) -> Result<Vec<SemanticEdge>, String>
```

Algorithm:
1. Get all chunks for the atom
2. For each chunk, query `vec_chunks` for similar chunks (sqlite-vec is O(log n))
3. Map chunk matches to atoms, keep highest similarity per atom pair
4. Store top N edges bidirectionally

**Complexity**: O(c × k × log n) per atom where c=chunks, k=query limit, n=total chunks.

### 1.3 New Tauri Commands

```rust
// Get edges for local graph
#[tauri::command]
fn get_atom_neighborhood(
    atom_id: String,
    depth: i32,           // 1 = direct connections, 2 = friends-of-friends
    min_similarity: f32,  // Filter threshold
) -> Result<NeighborhoodGraph, String>;

// Get all edges for global view
#[tauri::command]
fn get_semantic_edges(
    min_similarity: f32,
) -> Result<Vec<SemanticEdge>, String>;

// Rebuild edges for existing atoms (migration)
#[tauri::command]
async fn rebuild_semantic_edges() -> Result<i32, String>;
```

**Files to modify:**
- `src-tauri/src/db.rs` - Add migration
- `src-tauri/src/embedding.rs` - Hook edge computation
- `src-tauri/src/commands.rs` - Add new commands
- `src-tauri/src/models.rs` - Add `SemanticEdge`, `NeighborhoodGraph` structs

---

## Phase 2: Local Graph View (Core Feature)

### 2.1 New Component: LocalGraphView

Create `src/components/canvas/LocalGraphView.tsx`:

```typescript
interface LocalGraphViewProps {
  centerAtomId: string;
  depth?: 1 | 2;              // Neighborhood depth (default: 1)
  onAtomClick: (id: string) => void;
  onNavigate: (id: string) => void;  // Re-center on new atom
  onClose: () => void;
}
```

Behavior:
- Centers on the focal atom (visually distinct, larger)
- Shows directly connected atoms in a ring/cluster around it
- Depth 2 shows second-degree connections in an outer ring
- Maximum ~25-30 nodes to prevent clutter
- Click atom to view details; double-click to re-center on it

### 2.2 Layout Algorithm

Use a radial force layout for local graphs:
- Focal atom pinned at center
- Direct connections at radius R1 (~150px)
- Second-degree at radius R2 (~300px)
- Nodes repel within their ring, attract to center

```typescript
// D3 forces for local graph
simulation
  .force('radial', d3.forceRadial(d => d.depth === 1 ? 150 : 300, 0, 0))
  .force('collide', d3.forceCollide(60))
  .force('charge', d3.forceManyBody().strength(-100))
```

### 2.3 Entry Points

Add "View neighborhood" actions throughout the app:

1. **AtomViewer header** - Graph icon button
2. **AtomCard context menu** - "Show in graph" option
3. **TagNode hover** - Graph icon (opens local graph filtered to that tag)
4. **Semantic search results** - "View connections" on each result

**Files to modify:**
- `src/components/atoms/AtomViewer.tsx`
- `src/components/atoms/AtomCard.tsx`
- `src/components/tags/TagNode.tsx`
- `src/components/search/SemanticSearch.tsx`

### 2.4 UI Store Changes

Add to `src/stores/ui.ts`:

```typescript
// Local graph state
localGraph: {
  isOpen: boolean;
  centerAtomId: string | null;
  depth: 1 | 2;
} | null;

// Actions
openLocalGraph: (atomId: string, depth?: 1 | 2) => void;
navigateLocalGraph: (atomId: string) => void;  // Re-center
closeLocalGraph: () => void;
setLocalGraphDepth: (depth: 1 | 2) => void;
```

**Files to create:**
- `src/components/canvas/LocalGraphView.tsx`
- `src/components/canvas/LocalGraphControls.tsx`

---

## Phase 3: Global Graph Improvements

The global view remains available but is improved.

### 3.1 Fix Zoom/Pan UX

Modify TransformWrapper config in `CanvasView.tsx`:

```typescript
<TransformWrapper
  // Enable momentum for native Mac feel
  panning={{
    velocityDisabled: false,
    excluded: ['atom-node']
  }}
  // Smoother zoom
  wheel={{
    smoothStep: 0.001,
    step: 0.15
  }}
  pinch={{
    step: 3  // Lower = smoother pinch
  }}
  // Add momentum
  velocityAnimation={{
    sensitivity: 1,
    animationTime: 300,
    animationType: "easeOut",
    equalToMove: true
  }}
  doubleClick={{ mode: "zoomIn", step: 0.5 }}
>
```

### 3.2 Hybrid Connection Display

Show both tag-based and semantic connections with visual differentiation.

Modify `ConnectionLines.tsx`:

```typescript
interface Connection {
  sourceId: string;
  targetId: string;
  type: 'tag' | 'semantic' | 'both';
  strength: number;  // 0-1, affects opacity and width
}

// Visual styles
const STYLES = {
  tag:      { color: '#666666', dash: 'none' },
  semantic: { color: '#7c3aed', dash: '6,3' },  // Purple dashed
  both:     { color: '#a78bfa', dash: 'none' }, // Light purple solid
};
```

Lower tag threshold to 1+ shared tags, but vary visual weight:
- 3+ tags OR both tag+semantic: thick solid line
- 2 tags: medium line
- 1 tag: thin line
- Semantic only: purple dashed line

### 3.3 Connection Controls

Add toggle controls in `CanvasControls.tsx`:

```typescript
// Connection filters
<div className="space-y-1 text-xs">
  <label><input type="checkbox" /> Tag connections</label>
  <label><input type="checkbox" /> Semantic connections</label>
  <label><input type="range" min="0" max="1" /> Min similarity</label>
</div>
```

### 3.4 Node Visual Improvements

Modify `AtomNode.tsx`:

1. **Color coding by primary tag**: Hash tag name to HSL hue
2. **Size by connection count**: More connected = larger (140-180px)
3. **Border by type**: Different border for highly connected "hub" atoms

```typescript
// Color from tag
function tagToColor(tagName: string): string {
  const hash = tagName.split('').reduce((a, c) => ((a << 5) - a + c.charCodeAt(0)) | 0, 0);
  return `hsl(${Math.abs(hash % 360)}, 55%, 50%)`;
}

// Indicator dot in corner
<div
  className="absolute top-2 left-2 w-3 h-3 rounded-full"
  style={{ backgroundColor: tagToColor(primaryTag) }}
/>
```

**Files to modify:**
- `src/components/canvas/CanvasView.tsx`
- `src/components/canvas/ConnectionLines.tsx`
- `src/components/canvas/CanvasControls.tsx`
- `src/components/canvas/AtomNode.tsx`
- `src/components/canvas/useForceSimulation.ts`

---

## Phase 4: Clustering & Visual Grouping

### 4.1 Community Detection (Backend)

Implement Louvain-style modularity clustering in Rust:

```rust
#[tauri::command]
fn compute_clusters(
    min_cluster_size: i32,  // Ignore tiny clusters
) -> Result<Vec<AtomCluster>, String>;

struct AtomCluster {
    id: u32,
    atom_ids: Vec<String>,
    dominant_tags: Vec<String>,  // Most common tags in cluster
}
```

Store cluster assignments in memory or lightweight cache table:

```sql
CREATE TABLE atom_clusters (
  atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
  cluster_id INTEGER NOT NULL,
  computed_at TEXT NOT NULL
);
```

### 4.2 Cluster Visualization

In global view:
- Subtle background shading per cluster (convex hull or circle)
- Cluster label floating near cluster center (dominant tag names)
- Click cluster label to filter to that cluster

In local view:
- Show which cluster the focal atom belongs to
- Indicate when connections cross cluster boundaries

### 4.3 Hub Node Identification

Use degree centrality (simple connection count) to identify hub nodes:
- Nodes with >10 connections get a subtle glow
- Hub nodes are slightly larger
- Useful for navigation: "Start at a hub, explore outward"

**Files to create:**
- `src-tauri/src/clustering.rs`
- `src/components/canvas/ClusterBackground.tsx`
- `src/components/canvas/ClusterLabel.tsx`

**Files to modify:**
- `src-tauri/src/commands.rs`
- `src/components/canvas/CanvasContent.tsx`

---

## Phase 5: Navigation & Workflow Integration

### 5.1 Breadcrumb Navigation

When navigating local graphs, show a breadcrumb trail:

```
Home > "Neural Networks" > "Backpropagation" > "Gradient Descent"
```

Click any breadcrumb to jump back to that atom's neighborhood.

### 5.2 "Locate on Canvas" Action

From any atom (in viewer, search results, etc.), jump to its position on the global canvas:
- Switch to canvas view
- Animate zoom to center on the atom
- Highlight the atom briefly

Add to UI store:
```typescript
highlightedAtomId: string | null;
locateOnCanvas: (atomId: string) => void;
```

### 5.3 Mini-Preview in Drawer

When viewing an atom in the drawer, show a small (200×150px) interactive preview of its immediate neighborhood below the RelatedAtoms section.

Create `src/components/canvas/MiniGraphPreview.tsx`:
- Simplified LocalGraphView (no controls, depth=1 only)
- Click to expand to full local graph view
- Shows 5-7 closest connections

### 5.4 View Mode Enhancement

Rename/reorganize view modes:
- **Canvas** (global) - renamed to "Overview"
- **Local Graph** - new, the neighborhood view
- **Grid** / **List** - unchanged

Default to Local Graph when opening from an atom; Overview when accessing from sidebar.

**Files to modify:**
- `src/stores/ui.ts`
- `src/components/layout/MainView.tsx`
- `src/components/atoms/AtomViewer.tsx`

**Files to create:**
- `src/components/canvas/MiniGraphPreview.tsx`
- `src/components/canvas/NavigationBreadcrumb.tsx`

---

## Implementation Order

### Sprint 1: Semantic Edges
1. Add `semantic_edges` table migration
2. Implement edge computation in embedding pipeline
3. Add `get_atom_neighborhood` and `get_semantic_edges` commands
4. Add `rebuild_semantic_edges` for existing data
5. Test with existing atoms

### Sprint 2: Local Graph View
1. Create `LocalGraphView` component with radial layout
2. Add UI store state for local graph
3. Add entry points (AtomViewer button, context menus)
4. Implement navigation (re-center on click)

### Sprint 3: Global Graph Improvements
1. Fix zoom/pan configuration
2. Implement hybrid connection display (tag + semantic)
3. Add connection controls (toggles, similarity slider)
4. Add node color coding and sizing

### Sprint 4: Clustering & Polish
1. Implement clustering algorithm
2. Add cluster visualization (backgrounds, labels)
3. Add breadcrumb navigation
4. Add "locate on canvas" functionality
5. Create mini-preview component

### Sprint 5: Integration & Testing
1. End-to-end testing with large datasets
2. Performance optimization if needed
3. Polish animations and transitions
4. Documentation updates

---

## Critical Files Summary

### Backend (Create)
- `src-tauri/src/clustering.rs` - Clustering algorithm

### Backend (Modify)
- `src-tauri/src/db.rs` - Add migrations for semantic_edges, atom_clusters
- `src-tauri/src/embedding.rs` - Hook edge computation after embedding
- `src-tauri/src/commands.rs` - Add new Tauri commands
- `src-tauri/src/models.rs` - Add new structs

### Frontend (Create)
- `src/components/canvas/LocalGraphView.tsx` - Core local graph component
- `src/components/canvas/LocalGraphControls.tsx` - Depth, close controls
- `src/components/canvas/MiniGraphPreview.tsx` - Small preview for drawer
- `src/components/canvas/ClusterBackground.tsx` - Cluster shading
- `src/components/canvas/ClusterLabel.tsx` - Floating cluster labels
- `src/components/canvas/NavigationBreadcrumb.tsx` - Navigation trail

### Frontend (Modify)
- `src/components/canvas/CanvasView.tsx` - Zoom/pan config, data loading
- `src/components/canvas/ConnectionLines.tsx` - Hybrid connection rendering
- `src/components/canvas/AtomNode.tsx` - Color coding, sizing
- `src/components/canvas/CanvasControls.tsx` - Connection toggles
- `src/components/canvas/useForceSimulation.ts` - Lower tag threshold, merge edges
- `src/stores/ui.ts` - Local graph state, highlight state
- `src/components/atoms/AtomViewer.tsx` - Add graph entry point
- `src/components/atoms/AtomCard.tsx` - Add context menu option
- `src/components/tags/TagNode.tsx` - Add graph hover icon
- `src/components/layout/MainView.tsx` - Integrate local graph view
