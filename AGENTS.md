# Atomic - Note-Taking Desktop Application

## Project Overview
Atomic is a Tauri v2 desktop application for note-taking with a React frontend. It features markdown editing, hierarchical tagging, and AI-powered semantic search using local embeddings.

## Current Status: Phase 2.1 Complete
Phase 2.1 (Real sqlite-lembed Integration) is complete with:
- Real 384-dimensional embeddings using sqlite-lembed and all-MiniLM-L6-v2 model
- sqlite-lembed extension loaded at runtime for each database connection
- Model registered in temp.lembed_models for embedding generation
- Semantic search uses real embeddings for query and content matching

Phase 2 (Embedding Pipeline) features:
- Async embedding generation when atoms are created or updated
- Content chunking algorithm for optimal embedding
- Semantic search using sqlite-vec for vector similarity
- Related atoms discovery based on content similarity
- Embedding status indicators on atom cards
- Real-time status updates via Tauri events

Phase 1 (Foundation + Data Layer) features:
- Full UI layout with left panel, main view, and right drawer
- SQLite database with sqlite-vec extension
- Complete CRUD operations for atoms and tags
- Markdown editing with CodeMirror and rendering with react-markdown
- Hierarchical tag navigation with context menus
- Grid and list view modes for atoms
- Dark theme (Obsidian-inspired)

## Tech Stack
- **Desktop Framework**: Tauri v2 (Rust backend)
- **Frontend**: React 18+ with TypeScript
- **Build Tool**: Vite 6
- **Styling**: Tailwind CSS v4 (using `@tailwindcss/vite` plugin)
- **State Management**: Zustand 5
- **Database**: SQLite with sqlite-vec and sqlite-lembed extensions (via rusqlite)
- **Embeddings**: Real 384-dimensional vectors via sqlite-lembed + all-MiniLM-L6-v2 GGUF model
- **Markdown Editor**: CodeMirror 6 (`@uiw/react-codemirror`)
- **Markdown Rendering**: react-markdown with remark-gfm

## Project Structure
```
/src-tauri
  /src
    main.rs           # Tauri entry point
    lib.rs            # App setup, command registration, resource path resolution
    db.rs             # SQLite setup, migrations, sqlite-lembed loading, model registration
    commands.rs       # All Tauri command implementations
    models.rs         # Rust structs for data
    chunking.rs       # Content chunking algorithm
    embedding.rs      # Embedding generation using sqlite-lembed
  /resources
    all-MiniLM-L6-v2.q8_0.gguf  # Bundled embedding model (~24MB, Q8_0 quantization)
    lembed0.so                   # sqlite-lembed extension (Linux x86_64, ~2.4MB)
  Cargo.toml
  tauri.conf.json

/src
  /components
    /layout           # LeftPanel, MainView, RightDrawer, Layout
    /atoms            # AtomCard, AtomEditor, AtomViewer, AtomGrid, AtomList, RelatedAtoms
    /tags             # TagTree, TagNode, TagChip, TagSelector
    /search           # SemanticSearch
    /ui               # Button, Input, Modal, FAB, ContextMenu
  /stores             # Zustand stores (atoms.ts, tags.ts, ui.ts)
  /hooks              # Custom hooks (useClickOutside, useKeyboard, useEmbeddingEvents)
  /lib                # Utilities (tauri.ts, markdown.ts, date.ts)
  App.tsx
  main.tsx
  index.css           # Tailwind imports + custom animations

/index.html
/vite.config.ts
/package.json
```

## Common Commands

### Development
```bash
# Install dependencies
npm install

# Run development server (frontend only)
npm run dev

# Run development server (frontend + Tauri)
npm run tauri dev

# Build for production
npm run tauri build

# Type check
npm run build
```

### Rust Backend
```bash
# Check Rust code
cd src-tauri && cargo check

# Build Rust code
cd src-tauri && cargo build

# Run tests (including chunking tests)
cd src-tauri && cargo test
```

## Database

### Location
The SQLite database is stored in the Tauri app data directory:
- macOS: `~/Library/Application Support/com.atomic.app/atomic.db`
- Linux: `~/.local/share/com.atomic.app/atomic.db`
- Windows: `%APPDATA%/com.atomic.app/atomic.db`

### Schema
```sql
-- Core content units
CREATE TABLE atoms (
  id TEXT PRIMARY KEY,  -- UUID
  content TEXT NOT NULL,
  source_url TEXT,
  created_at TEXT NOT NULL,  -- ISO 8601
  updated_at TEXT NOT NULL,
  embedding_status TEXT DEFAULT 'pending'  -- 'pending', 'processing', 'complete', 'failed'
);

-- Hierarchical tags
CREATE TABLE tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL
);

-- Many-to-many relationship
CREATE TABLE atom_tags (
  atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
  tag_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (atom_id, tag_id)
);

-- Chunked content with embeddings
CREATE TABLE atom_chunks (
  id TEXT PRIMARY KEY,
  atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  content TEXT NOT NULL,
  embedding BLOB  -- 384-dimensional float vector from sqlite-lembed
);

-- Vector similarity search (sqlite-vec virtual table)
CREATE VIRTUAL TABLE vec_chunks USING vec0(
  chunk_id TEXT PRIMARY KEY,
  embedding float[384]
);

-- Temporary table for sqlite-lembed model registration (per-connection)
-- temp.lembed_models(name TEXT, model BLOB)
```

## Tauri Commands (API)

### Atom Operations
- `get_all_atoms()` → `Vec<AtomWithTags>`
- `get_atom(id)` → `AtomWithTags`
- `create_atom(content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding)
- `update_atom(id, content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding)
- `delete_atom(id)` → `()`
- `get_atoms_by_tag(tag_id)` → `Vec<AtomWithTags>`

### Tag Operations
- `get_all_tags()` → `Vec<TagWithCount>` (hierarchical tree)
- `create_tag(name, parent_id?)` → `Tag`
- `update_tag(id, name, parent_id?)` → `Tag`
- `delete_tag(id)` → `()`

### Embedding Operations
- `find_similar_atoms(atom_id, limit, threshold)` → `Vec<SimilarAtomResult>`
- `search_atoms_semantic(query, limit, threshold)` → `Vec<SemanticSearchResult>`
- `retry_embedding(atom_id)` → `()` (retriggers embedding for failed atoms)
- `process_pending_embeddings()` → `i32` (processes all pending atoms, returns count)
- `get_embedding_status(atom_id)` → `String`

### Utility
- `check_sqlite_vec()` → `String` (version check)

## Tauri Events

### embedding-complete
Emitted when an atom's embedding generation completes (success or failure).

Payload:
```typescript
{
  atom_id: string;
  status: 'complete' | 'failed';
  error?: string;
}
```

## Chunking Algorithm

Content is chunked for optimal embedding generation:
1. Split by double newlines (paragraphs)
2. For paragraphs > 1500 chars, split by sentence boundaries (`. `, `! `, `? `)
3. Merge chunks < 100 chars with previous chunk
4. Skip final chunks < 50 chars
5. Cap chunks at 2000 chars max

## Key Dependencies

### Rust (Cargo.toml)
- `tauri` = "2"
- `tauri-plugin-opener` = "2"
- `rusqlite` = { version = "0.32", features = ["bundled", "load_extension"] }
- `sqlite-vec` = "0.1.6"
- `serde` = { version = "1", features = ["derive"] }
- `serde_json` = "1"
- `uuid` = { version = "1", features = ["v4"] }
- `chrono` = { version = "0.4", features = ["serde"] }
- `zerocopy` = { version = "0.8", features = ["derive"] }
- `tokio` = { version = "1", features = ["full"] }

### Frontend (package.json)
- `@tauri-apps/api` = "^2.0.0"
- `react` = "^18.3.1"
- `zustand` = "^5.0.0"
- `@uiw/react-codemirror` = "^4.25.3"
- `@codemirror/lang-markdown` = "^6.5.0"
- `@codemirror/theme-one-dark` = "^6.1.3"
- `react-markdown` = "^10.1.0"
- `remark-gfm` = "^4.0.1"
- `tailwindcss` = "^4.0.0"
- `@tailwindcss/vite` = "^4.0.0"
- `@tailwindcss/typography` = "^0.5.19"

## Design System (Dark Theme - Obsidian-inspired)

### Colors
- Background: `#1e1e1e` (main), `#252525` (panels), `#2d2d2d` (cards/elevated)
- Text: `#dcddde` (primary), `#888888` (secondary/muted), `#666666` (tertiary)
- Borders: `#3d3d3d`
- Accent: `#7c3aed` (purple), `#a78bfa` (light purple for tags)
- Status: `amber-500` (pending/processing), `red-500` (failed)

### Layout
- Left Panel: 250px fixed width
- Main View: Flexible, fills remaining space
- Right Drawer: 500px max or 40% of screen, slides from right as overlay

### Animations
- Drawer slide: 200ms ease-out
- Modal fade/zoom: 200ms
- Hover transitions: 150ms
- Embedding status pulse: CSS `animate-pulse`

## State Management (Zustand Stores)

### atoms.ts
- `atoms: AtomWithTags[]` - All loaded atoms
- `isLoading: boolean` - Loading state
- `error: string | null` - Error message
- `semanticSearchQuery: string` - Current semantic search query
- `semanticSearchResults: SemanticSearchResult[] | null` - Search results (null = not searching)
- `isSearching: boolean` - Semantic search loading state
- Actions: `fetchAtoms`, `fetchAtomsByTag`, `createAtom`, `updateAtom`, `deleteAtom`, `updateAtomStatus`, `searchSemantic`, `clearSemanticSearch`, `retryEmbedding`

### tags.ts
- `tags: TagWithCount[]` - Hierarchical tag tree
- `isLoading: boolean`
- `error: string | null`
- Actions: `fetchTags`, `createTag`, `updateTag`, `deleteTag`

### ui.ts
- `selectedTagId: string | null` - Currently selected tag filter
- `drawerState: { isOpen, mode, atomId }` - Drawer state
- `viewMode: 'grid' | 'list'` - Atom display mode
- `searchQuery: string` - Text search filter
- Actions: `setSelectedTag`, `openDrawer`, `closeDrawer`, `setViewMode`, `setSearchQuery`

## sqlite-lembed Integration

### How It Works
1. **Extension Loading**: On database initialization, sqlite-lembed is loaded via `conn.load_extension()` with the `load_extension` feature enabled in rusqlite
2. **Model Registration**: The all-MiniLM-L6-v2 GGUF model is registered in `temp.lembed_models` for each connection
3. **Embedding Generation**: Content chunks are embedded using `SELECT lembed('all-MiniLM-L6-v2', ?1)`
4. **Query Embedding**: Search queries are embedded the same way for semantic matching

### Resource Files
- `lembed0.so` - sqlite-lembed extension binary (Linux x86_64, v0.0.1-alpha.8)
- `all-MiniLM-L6-v2.q8_0.gguf` - Embedding model (Q8_0 quantization, ~24MB)

### Platform Support
Currently bundled for Linux x86_64. For other platforms:
- macOS aarch64: Download `sqlite-lembed-0.0.1-alpha.8-loadable-macos-aarch64.tar.gz`
- macOS x86_64: Download `sqlite-lembed-0.0.1-alpha.8-loadable-macos-x86_64.tar.gz`
- Windows: Not yet available in pre-built form

### Similarity Calculation
- sqlite-vec returns Euclidean distance (lower = more similar)
- For normalized vectors, convert to similarity: `1.0 - (distance / 2.0)`
- Default threshold: 0.7 for related atoms, 0.3 for semantic search

## Future Phases

### Phase 3: Wiki Integration
- Wikipedia article fetching and display
- Wiki viewer in right drawer
- Link atoms to Wikipedia articles

