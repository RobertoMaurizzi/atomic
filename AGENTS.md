# Atomic - Note-Taking Desktop Application

## Project Overview
Atomic is a Tauri v2 desktop application for note-taking with a React frontend. It features markdown editing, hierarchical tagging, AI-powered semantic search using embeddings, automatic tag extraction, wiki article synthesis, agentic chat with RAG, and an interactive canvas view for spatial atom visualization. The core business logic lives in the `atomic-core` Rust crate, which is independent of Tauri and can be used as a standalone library. A standalone `atomic-server` binary provides full REST API + WebSocket access to the knowledge base without Tauri.

## Tech Stack
- **Desktop Framework**: Tauri v2 (Rust backend)
- **Core Library**: `atomic-core` Rust crate (standalone, no Tauri dependency)
- **Frontend**: React 18+ with TypeScript
- **Build Tool**: Vite 6
- **Styling**: Tailwind CSS v4 (using `@tailwindcss/vite` plugin)
- **State Management**: Zustand 5 (with persist middleware for UI preferences)
- **Database**: SQLite with sqlite-vec extension (via rusqlite)
- **AI Providers**: Pluggable — OpenRouter and Ollama fully supported
  - **Embeddings**: OpenRouter (default: openai/text-embedding-3-small) or Ollama (default: nomic-embed-text)
  - **LLM**: OpenRouter (default: openai/gpt-4o-mini for tagging) or Ollama (default: llama3.2)
- **HTTP Server**: actix-web (embedded in Tauri for browser extension / MCP; standalone `atomic-server` binary for headless access)
- **MCP Server**: Model Context Protocol server via rmcp (integrated + standalone binary)
- **HTTP Client**: reqwest (Rust)
- **Markdown Editor**: CodeMirror 6 (`@uiw/react-codemirror`)
- **Markdown Rendering**: react-markdown with remark-gfm
- **Canvas Visualization**: d3-force (simulation), react-zoom-pan-pinch (zoom/pan)
- **List Virtualization**: @tanstack/react-virtual

## Architecture

The project is a **Cargo workspace** with multiple crates:

```
Cargo.toml (workspace root)
├── src-tauri/              → Tauri desktop app (thin wrappers around atomic-core)
├── crates/atomic-core/     → Standalone core library (all business logic, no Tauri dependency)
├── crates/atomic-server/   → Standalone HTTP server binary (REST API + WebSocket, no Tauri)
├── crates/atomic-mcp/      → Standalone MCP server binary
└── crates/mcp-bridge/      → HTTP-to-stdio bridge for MCP protocol
```

**`atomic-core`** is the key architectural piece — it provides the `AtomicCore` facade with all CRUD, search, embedding, wiki, chat, agent, and clustering operations. It uses callback-based event systems (`Fn(EmbeddingEvent)`, `Fn(ChatEvent)`) rather than Tauri events, making it usable from any Rust context.

The Tauri app (`src-tauri`) is a thin wrapper that:
1. Manages the `AppHandle` for Tauri events
2. Delegates to `atomic-core` for all business logic (including chat and agent)
3. Bridges `ChatEvent` / `EmbeddingEvent` callbacks → Tauri `app_handle.emit()` events
4. Runs an HTTP server (actix-web on port 44380) for browser extension and MCP

The standalone server (`atomic-server`) wraps `atomic-core` with:
1. Full REST API (~47 endpoints) covering all `AtomicCore` operations
2. WebSocket endpoint for push events (embedding progress, chat streaming)
3. Named, revocable API token authentication (SHA-256 hashed, DB-backed)
4. No Tauri dependency — runs headless for browser extensions, web clients, mobile apps

## Project Structure
```
/Cargo.toml             # Workspace definition

/crates
  /atomic-core          # Standalone core library
    /src
      lib.rs            # AtomicCore facade with all high-level operations
      db.rs             # SQLite setup, migrations, connection management
      models.rs         # Shared Rust structs (Atom, Tag, WikiArticle, etc.)
      error.rs          # AtomicCoreError enum
      chunking.rs       # Markdown-aware content chunking
      embedding.rs      # Embedding generation + tag extraction pipeline
      extraction.rs     # Tag extraction logic using provider abstraction
      search.rs         # Unified search (semantic, keyword, hybrid)
      wiki.rs           # Wiki article generation and update logic
      settings.rs       # Settings CRUD with defaults and migration
      chat.rs           # Conversation CRUD (create, get, update, delete, scope management)
      agent.rs          # Agentic chat loop with tool calling, ChatEvent callback system
      clustering.rs     # Atom clustering algorithms
      compaction.rs     # Tag compaction using LLM
      tokens.rs         # Named API token CRUD (create, verify, revoke, migrate)
      /import
        mod.rs          # Import module exports
        obsidian.rs     # Obsidian vault import logic
      /providers        # Pluggable AI provider abstraction
        mod.rs          # ProviderConfig, factory functions, provider cache
        types.rs        # Message, ToolCall, CompletionResponse, StreamDelta
        error.rs        # ProviderError enum with retry support
        traits.rs       # EmbeddingProvider, LlmProvider, StreamingLlmProvider traits
        models.rs       # AvailableModel, capability caching
        /openrouter     # OpenRouter provider implementation
          mod.rs        # OpenRouterProvider combining embedding + LLM
          embedding.rs  # Embedding API calls
          llm.rs        # Chat completion + streaming
        /ollama         # Ollama provider implementation (full support)
          mod.rs        # OllamaProvider combining embedding + LLM
          embedding.rs  # Embedding API calls with batch processing
          llm.rs        # Chat completion + streaming with tool calling
    Cargo.toml
  /atomic-server          # Standalone HTTP server (no Tauri dependency)
    /src
      main.rs             # CLI entry point with subcommands (serve, token create/list/revoke)
      config.rs           # Cli struct with Serve/Token subcommands (clap derive)
      state.rs            # AppState (AtomicCore + broadcast channel), ServerEvent enum
      auth.rs             # Named API token auth middleware (DB-backed, verify + last_used update)
      error.rs            # ApiError -> HttpResponse mapping
      ws.rs               # WebSocket upgrade handler, broadcast subscriber loop
      event_bridge.rs     # EmbeddingEvent/ChatEvent callback -> ServerEvent broadcast bridge
      /routes
        mod.rs            # configure_routes() registering all route groups
        atoms.rs          # Atom + Tag CRUD
        search.rs         # Unified search (semantic/keyword/hybrid) + find_similar
        wiki.rs           # Wiki CRUD + generate + update
        settings.rs       # Get/set settings, test connection, list models
        embedding.rs      # Process pending, retry, reset stuck, get status
        canvas.rs         # Positions CRUD, atoms-with-embeddings
        graph.rs          # Semantic edges, neighborhood, rebuild
        clustering.rs     # Compute, get, connection counts
        chat.rs           # Conversations CRUD, scope management, send message
        ollama.rs         # Test, list models, verify provider
        auth.rs           # API token CRUD (create, list, revoke)
        utils.rs          # sqlite-vec check, compact tags
    Cargo.toml
  /atomic-mcp             # Standalone MCP server binary
  /mcp-bridge             # HTTP-to-stdio bridge for MCP

/src-tauri
  /src
    main.rs             # Tauri entry point
    lib.rs              # App setup, command registration, HTTP server launch
    db.rs               # Tauri-specific Database wrapper around atomic-core
    models.rs           # Tauri-specific request/response structs
    http_server.rs      # actix-web HTTP server (port 44380) for browser extension + MCP
    chat.rs             # Thin wrappers delegating to atomic-core chat CRUD
    agent.rs            # Thin wrapper bridging ChatEvent → Tauri events
    obsidian.rs         # Obsidian import Tauri integration
    /mcp                # MCP server integration
      mod.rs            # Module exports
      server.rs         # AtomicMcpServer with tool handlers (semantic_search, read_atom, etc.)
      types.rs          # MCP-specific types
    /commands            # Tauri command implementations (thin wrappers)
      mod.rs            # Re-exports all commands
      helpers.rs        # Shared helper functions
      atoms.rs          # Atom CRUD (delegates to atomic-core)
      tags.rs           # Tag CRUD
      embedding.rs      # Embedding and search commands
      settings.rs       # Settings and model discovery
      wiki.rs           # Wiki article operations
      canvas.rs         # Canvas position management
      graph.rs          # Semantic graph operations
      clustering.rs     # Clustering commands
      ollama.rs         # Ollama-specific commands
      import.rs         # Obsidian import command
      utils.rs          # Utility commands (sqlite-vec check, tag compaction)
    /providers          # Re-exports from atomic-core
      mod.rs            # Thin re-export layer
  Cargo.toml
  tauri.conf.json

/scripts
  import-wikipedia.js   # Bulk import Wikipedia articles
  import-rss.js         # RSS feed import
  import/obsidian.js    # Obsidian vault import (Node.js alternative)
  build-mcp-bridge.js   # Build standalone MCP bridge binary
  build-release.js      # Versioned release builds
  reset-database.js     # Database reset utilities
  reset-tags.js         # Tag reset utilities
  reset-chunks.js       # Chunk reset utilities
  drop-database.js      # Database drop utility

/src
  /components
    /layout             # LeftPanel, MainView, RightDrawer, Layout
    /atoms              # AtomCard, AtomEditor, AtomViewer, AtomGrid, AtomList, RelatedAtoms
    /canvas             # CanvasView, CanvasContent, AtomNode, ConnectionLines, CanvasControls, useForceSimulation
    /tags               # TagTree, TagNode, TagChip, TagSelector
    /wiki               # WikiViewer, WikiArticleContent, WikiHeader, WikiEmptyState, WikiGenerating, CitationLink, CitationPopover
    /chat               # ChatViewer, ConversationsList, ConversationCard, ChatView, ChatHeader, ChatMessage, ChatInput, ScopeEditor
    /command-palette     # CommandPalette, CommandInput, CommandList, CommandItem, SearchResults, TagResults, fuzzySearch
    /search             # SemanticSearch
    /settings           # SettingsModal, SettingsButton
    /ui                 # Button, Input, Modal, FAB, ContextMenu
  /stores               # Zustand stores (atoms.ts, tags.ts, ui.ts, settings.ts, wiki.ts, chat.ts)
  /hooks                # Custom hooks (useClickOutside, useKeyboard, useEmbeddingEvents, useChatEvents)
  /lib                  # Utilities (tauri.ts, markdown.ts, date.ts, similarity.ts)
  App.tsx
  main.tsx
  index.css             # Tailwind imports + custom animations

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
# Check all workspace crates
cargo check

# Build all workspace crates
cargo build

# Run tests (all crates including atomic-core unit tests)
cargo test

# Check/build specific crate
cargo check -p atomic-core
cargo test -p atomic-core
cargo check -p atomic-server
cargo test -p atomic-server

# Run standalone server
cargo run -p atomic-server -- --db-path /path/to/atomic.db serve --port 8080

# Token management CLI
cargo run -p atomic-server -- --db-path /path/to/atomic.db token create --name "my-laptop"
cargo run -p atomic-server -- --db-path /path/to/atomic.db token list
cargo run -p atomic-server -- --db-path /path/to/atomic.db token revoke <token-id>
```

### Utility Scripts
```bash
# Import Wikipedia articles for stress testing (requires app to be run once first)
npm run import:wikipedia        # Import 500 articles (default)
npm run import:wikipedia 1000   # Import custom number of articles

# Import RSS feeds
npm run import:rss

# Build standalone MCP bridge binary
npm run build:mcp-bridge

# Run Tauri with a fresh database (for testing)
npm run tauri:fresh

# Database management
npm run db:reset               # Reset database (interactive)
npm run db:reset:force         # Force reset with backup
npm run db:reset-tags:force    # Reset tags only
npm run db:reset-chunks:force  # Reset chunks only

# Release builds
npm run release:patch          # Bump patch version and build
npm run release:minor          # Bump minor version and build
```

## Database

### Location
The SQLite database is stored in the Tauri app data directory:
- macOS: `~/Library/Application Support/com.atomic.app/atomic.db`
- Linux: `~/.local/share/com.atomic.app/atomic.db`
- Windows: `%APPDATA%/com.atomic.app/atomic.db`

### Settings Keys

**Provider selection:**
- `provider`: "openrouter" or "ollama" (default: "openrouter")

**OpenRouter settings:**
- `openrouter_api_key`: User's OpenRouter API key for LLM and embedding access
- `embedding_model`: Model for embeddings (default: "openai/text-embedding-3-small")
  - Supported: `openai/text-embedding-3-small` (1536 dim), `openai/text-embedding-3-large` (3072 dim)
  - Changing dimension requires re-embedding all atoms (handled automatically)
- `tagging_model`: Model for tag extraction (default: "openai/gpt-4o-mini")
- `wiki_model`: Model for wiki generation (default: "anthropic/claude-sonnet-4.5")
- `chat_model`: Model for chat (default: "anthropic/claude-sonnet-4.5")

**Ollama settings:**
- `ollama_host`: Ollama server URL (default: "http://127.0.0.1:11434")
- `ollama_embedding_model`: Embedding model name (default: "nomic-embed-text")
- `ollama_llm_model`: LLM model name (default: "llama3.2")

**General:**
- `auto_tagging_enabled`: "true" or "false" (default: "true")

Note: When using OpenRouter, LLM models are restricted to those supporting structured outputs. Ollama models are auto-discovered from the running server.

## Tauri Commands (API)

### Atom Operations
- `get_all_atoms()` → `Vec<AtomWithTags>`
- `get_atom(id)` → `AtomWithTags`
- `create_atom(content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding + tag extraction)
- `update_atom(id, content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding + tag extraction)
- `delete_atom(id)` → `()`
- `get_atoms_by_tag(tag_id)` → `Vec<AtomWithTags>`

### Tag Operations
- `get_all_tags()` → `Vec<TagWithCount>` (hierarchical tree)
- `create_tag(name, parent_id?)` → `Tag`
- `update_tag(id, name, parent_id?)` → `Tag`
- `delete_tag(id)` → `()`

### Embedding & Search Operations
- `find_similar_atoms(atom_id, limit, threshold)` → `Vec<SimilarAtomResult>`
- `search_atoms_semantic(query, limit, threshold)` → `Vec<SemanticSearchResult>` (vector similarity)
- `search_atoms_keyword(query, limit)` → `Vec<SemanticSearchResult>` (FTS5/BM25)
- `search_atoms_hybrid(query, limit, threshold)` → `Vec<SemanticSearchResult>` (combined BM25 + vector)
- `retry_embedding(atom_id)` → `()` (retriggers embedding for failed atoms)
- `reset_stuck_processing()` → `i32` (reset atoms stuck in 'processing' state)
- `process_pending_embeddings()` → `i32` (processes all pending atoms, returns count)
- `process_pending_tagging()` → `i32` (processes pending tag extraction)
- `get_embedding_status(atom_id)` → `String`

### Wiki Operations
- `get_wiki_article(tag_id)` → `Option<WikiArticleWithCitations>` (returns article with citations if exists)
- `get_wiki_article_status(tag_id)` → `WikiArticleStatus` (quick check: has_article, atom counts, updated_at)
- `generate_wiki_article(tag_id, tag_name)` → `WikiArticleWithCitations` (generates new article from scratch)
- `update_wiki_article(tag_id, tag_name)` → `WikiArticleWithCitations` (incrementally updates with new atoms)
- `delete_wiki_article(tag_id)` → `()` (deletes article and citations)

### Settings Operations
- `get_settings()` → `HashMap<String, String>` (all settings)
- `set_setting(key, value)` → `()` (upsert a setting)
- `test_openrouter_connection(apiKey)` → `Result<bool, String>` (validates API key)
- `get_available_llm_models()` → `Vec<AvailableModel>` (fetch models supporting structured outputs)

### Canvas Operations
- `get_atom_positions()` → `Vec<AtomPosition>` (returns all stored canvas positions)
- `save_atom_positions(positions)` → `()` (bulk save/update positions after simulation)
- `get_atoms_with_embeddings()` → `Vec<AtomWithEmbedding>` (atoms with average embedding vectors)

### Chat Operations
- `create_conversation(tag_ids, title?)` → `ConversationWithTags` (creates new conversation with optional tag scope)
- `get_conversations(filter_tag_id?, limit, offset)` → `Vec<ConversationWithTags>` (list conversations)
- `get_conversation(id)` → `Option<ConversationWithMessages>` (single conversation with full message history)
- `update_conversation(id, title?, is_archived?)` → `Conversation` (update metadata)
- `delete_conversation(id)` → `()` (delete conversation and all messages)
- `set_conversation_scope(conversation_id, tag_ids)` → `ConversationWithTags` (replace all scope tags)
- `add_tag_to_scope(conversation_id, tag_id)` → `ConversationWithTags` (add single tag to scope)
- `remove_tag_from_scope(conversation_id, tag_id)` → `ConversationWithTags` (remove single tag from scope)
- `send_chat_message(conversation_id, content)` → `ChatMessageWithContext` (send message, triggers agent loop)

### Semantic Graph Operations
- `get_semantic_edges(min_similarity)` → `Vec<SemanticEdge>` (all edges above threshold)
- `get_atom_neighborhood(atom_id, depth, min_similarity)` → `NeighborhoodGraph` (local graph view)
- `rebuild_semantic_edges()` → `i32` (rebuild all edges, for migrations)

### Clustering Operations
- `compute_clusters(min_similarity?, min_cluster_size?)` → `Vec<AtomCluster>` (compute and cache clusters)
- `get_clusters()` → `Vec<AtomCluster>` (get cached clusters or compute if missing)
- `get_connection_counts(min_similarity?)` → `HashMap<String, i32>` (hub identification)

### Ollama Operations
- `test_ollama(host)` → `bool` (test connection to Ollama server)
- `get_ollama_models(host)` → `Vec<OllamaModel>` (all models with categorization)
- `get_ollama_embedding_models_cmd(host)` → `Vec<AvailableModel>` (embedding models only)
- `get_ollama_llm_models_cmd(host)` → `Vec<AvailableModel>` (LLM models only)
- `verify_provider_configured()` → `bool` (check if provider has required settings)

### Import Operations
- `import_obsidian_vault(vault_path, max_notes?)` → `ImportResult` (import from Obsidian vault)

### MCP Operations
- `get_mcp_bridge_path()` → `String` (path to standalone MCP bridge binary)
- `get_mcp_config()` → `serde_json::Value` (MCP configuration for Claude integration)

### Utility Operations
- `check_sqlite_vec()` → `String` (version check)
- `compact_tags()` → `CompactionResult` (LLM-assisted tag categorization and merging)
- `get_all_wiki_articles()` → `Vec<WikiArticleWithCitations>` (all wiki articles)

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

### tagging-complete
Emitted when tag extraction completes (separate from embedding).

Payload:
```typescript
{
  atom_id: string;
  status: 'complete' | 'failed' | 'skipped';
  error?: string;
  tags_extracted: string[];      // IDs of all tags applied
  new_tags_created: string[];    // IDs of newly created tags
}
```

### atom-created
Emitted when an atom is created via the HTTP API (browser extension). Triggers UI refresh.

### embeddings-reset
Emitted when the embedding model/dimension changes and all atoms need re-embedding.

### Chat Events
Events emitted during chat agent loop:

**chat-stream-delta**: Streaming content from assistant
```typescript
{ conversation_id: string; content: string; }
```

**chat-tool-start**: Tool execution started
```typescript
{ conversation_id: string; tool_call_id: string; tool_name: string; tool_input: unknown; }
```

**chat-tool-complete**: Tool execution completed
```typescript
{ conversation_id: string; tool_call_id: string; results_count: number; }
```

**chat-complete**: Full message completed
```typescript
{ conversation_id: string; message: ChatMessageWithContext; }
```

**chat-error**: Error during chat
```typescript
{ conversation_id: string; error: string; }
```

## Wiki Synthesis

### How It Works
1. User clicks the article icon next to a tag in the left panel
2. Right drawer opens in wiki mode for that tag
3. If no article exists, shows empty state with "Generate Article" button
4. Generation fetches relevant chunks from atoms with that tag
5. Chunks are ranked by embedding similarity to tag name
6. Top chunks are sent to configured LLM provider (OpenRouter or Ollama) with generation prompt
7. LLM returns markdown article with [N] citations
8. Citations are extracted and mapped to source atoms/chunks
9. Article and citations are saved to database

### Incremental Updates
When new atoms are added after article generation:
1. Status check shows "X new atoms available" banner
2. Clicking "Update Article" fetches only new atoms' chunks
3. Existing article and new sources are sent to LLM with update prompt
4. LLM integrates new information, continuing citation numbering
5. Updated article replaces existing content

### Citation Interaction
- Citations appear as clickable [N] links inline in text
- Clicking opens a popover positioned near the citation
- Popover shows excerpt text (~300 chars max)
- "View full atom →" link opens atom in viewer mode
- Popover closes on click outside or Escape key

### Structured Outputs
Wiki generation uses structured outputs (via OpenRouter or Ollama JSON mode):
- Schema: `article_content` (string) and `citations_used` (array of integers)
- Temperature: 0.3 for consistent output
- Max tokens: 4000 for longer articles

## Automatic Tag Extraction

### How It Works
1. When an atom is created/updated, the embedding pipeline runs
2. If auto-tagging is enabled and API key is set, tag extraction runs in parallel with embedding
3. Each content chunk is sent to the configured provider (OpenRouter or Ollama) using the tagging model with the existing tag hierarchy
4. The LLM identifies existing tags that apply and suggests new tags if needed
5. Results from all chunks are merged and deduplicated
6. Existing tags are linked to the atom; new tags are created with proper hierarchy
7. The `embedding-complete` event includes tag information for UI updates

### Configurable Model
The tagging model can be configured in Settings:
- Default: `openai/gpt-4o-mini` (cheaper/faster, good for bulk imports)
- Alternative: `anthropic/claude-sonnet-4.5` (higher quality, more expensive)
- Any OpenRouter model ID that supports structured outputs can be used

### Structured Outputs
Tag extraction uses structured outputs to guarantee valid JSON responses:
- Schema enforces the exact structure: `existing_tag_ids` (array of strings) and `new_tags` (array of objects with name, parent_id, suggested_category)
- OpenRouter uses `response_format.type: "json_schema"` with strict validation
- Ollama uses JSON mode with schema in the prompt

### Tag Categories
New tags are automatically placed under category tags:
- **Locations**: Geographic places
- **People**: Named individuals
- **Organizations**: Companies, institutions, groups
- **Topics**: Subject matter, concepts
- **Events**: Historical or current events
- **Other**: Miscellaneous

### Error Handling
- API errors are retried up to 3 times with exponential backoff
- Extraction failures don't break the embedding pipeline
- Missing API key or disabled auto-tagging gracefully skips extraction

## Canvas View

### Architecture
The canvas view provides a spatial visualization of atoms using:
- **react-zoom-pan-pinch**: Handles zoom/pan interactions via TransformWrapper and TransformComponent
- **d3-force**: Calculates atom positions using force simulation (no D3 rendering)
- **React components**: Renders atom cards and SVG connection lines

### Force Simulation
The simulation uses multiple forces to position atoms:
- `forceManyBody()`: Repulsion between atoms (strength: -200)
- `forceCollide()`: Collision detection (radius: 100px)
- `forceLink()`: Attraction between atoms sharing tags
- `forceCenter()`: Centers graph at (2500, 2500) on 5000x5000 canvas
- Custom `similarityForce`: Attraction based on embedding cosine similarity (threshold: 0.7)

### Position Persistence
- Positions are saved to `atom_positions` table after simulation completes
- On subsequent loads, stored positions are used (no re-simulation)
- New atoms trigger incremental simulation with existing atoms fixed initially

### Visual Design
- **Atom nodes**: 160px wide compact cards with truncated content
- **Connection lines**: SVG lines between atoms sharing tags (opacity: 0.15)
- **Fading**: Non-matching atoms fade to 20% opacity when filtering by tag or search
- **Canvas controls**: Zoom in/out/reset buttons in bottom-right corner

### Components
- `CanvasView`: Main container, handles data loading and simulation orchestration
- `CanvasContent`: Inner content layer that gets transformed by zoom/pan
- `AtomNode`: Compact card component for individual atoms (memoized)
- `ConnectionLines`: SVG layer rendering lines between connected atoms
- `CanvasControls`: Zoom control buttons using react-zoom-pan-pinch hooks
- `useForceSimulation`: Custom hook managing D3 force simulation

## Chunking Algorithm

Content is chunked using a markdown-aware, overlapping strategy optimized for RAG:
1. Parse markdown structure (code blocks, headers, lists, paragraphs)
2. Never split code blocks (kept atomic even if exceeding max size)
3. Headers create natural chunk boundaries
4. Split at paragraph boundaries, then sentence boundaries if needed
5. Target chunk size: 2500 tokens (~10,000 chars)
6. Overlap: 200 tokens from next chunk appended to each chunk
7. Minimum chunk: 100 tokens (smaller merged with adjacent)
8. Maximum chunk: 3000 tokens (hard limit, except code blocks)
9. Token counting via tiktoken (cl100k_base encoding, matches OpenAI models)

## Key Dependencies

### Rust — atomic-core (Cargo.toml)
- `rusqlite` = { version = "0.32", features = ["bundled", "load_extension"] }
- `sqlite-vec` = "0.1.6"
- `tokio` = { version = "1", features = ["rt-multi-thread", "sync", "time", "macros"] }
- `reqwest` = { version = "0.12", features = ["json", "stream"] }
- `serde` = { version = "1", features = ["derive"] }
- `serde_json` = "1"
- `uuid` = { version = "1", features = ["v4"] }
- `chrono` = { version = "0.4", features = ["serde"] }
- `tiktoken-rs` = "0.6"
- `thiserror` = "1"
- `async-trait` = "0.1"
- `glob` = "0.3" (obsidian import)
- `yaml-rust2` = "0.9" (obsidian import)

### Rust — atomic-server (Cargo.toml)
- `atomic-core` = { path = "../atomic-core" }
- `actix-web` = "4.9"
- `actix-cors` = "0.7"
- `actix-ws` = "0.3"
- `tokio` = { version = "1", features = ["rt-multi-thread", "sync", "time", "macros", "signal"] }
- `clap` = { version = "4", features = ["derive"] }
- `reqwest` = { version = "0.12", features = ["json"] }
- `rusqlite` = { version = "0.32", features = ["bundled"] }

### Rust — src-tauri (Cargo.toml)
- `tauri` = "2"
- `atomic-core` = { path = "../crates/atomic-core" }
- `actix-web` = "4.9"
- `actix-cors` = "0.7"
- `rmcp` = { version = "0.10", features = ["server", "macros"] }
- `rmcp-actix-web` = "0.8"
- `schemars` = "1.0"
- `tauri-plugin-dialog` = "2"
- `tauri-plugin-shell` = "2"

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
- `@tanstack/react-virtual` = "^3.13.18"
- `d3-force` = "^3.0.0"
- `react-zoom-pan-pinch` = "^3.7.0"
- `@types/d3-force` = "^3.0.10" (dev)
- `better-sqlite3` = "^11.5.0" (dev, for import scripts)

## HTTP Servers & MCP

### Embedded HTTP Server (Tauri, actix-web)
An HTTP server runs alongside the Tauri app on `http://127.0.0.1:44380` for external integrations:
- `GET /health` — Health check with version info
- `POST /atoms` — Create atom (used by browser extension)
- `/mcp/*` — MCP (Model Context Protocol) endpoint for Claude integration

The embedded server uses `AppState` containing `SharedDatabase` and `AppHandle`, delegating to `atomic-core` for business logic.

### Standalone Server (`atomic-server`)
A standalone HTTP server (`crates/atomic-server/`) wrapping `atomic-core` with a full REST API. No Tauri dependency — runs headless.

**Auth**: All `/api/*` routes require `Authorization: Bearer <token>`. Tokens are named, revocable API tokens stored as SHA-256 hashes in the `api_tokens` table. A default token is auto-created on first run and printed to stdout. Manage tokens via CLI (`atomic-server token create/list/revoke`) or REST API (`/api/auth/tokens`). `/health` and `/ws` skip auth (WebSocket uses `?token=xxx` query param).

**CLI**: `cargo run -p atomic-server -- --db-path <path> serve --port 8080 --bind 127.0.0.1`
Token management: `cargo run -p atomic-server -- --db-path <path> token create --name "name"`, `token list`, `token revoke <id>`

**REST API** (~47 endpoints):

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check (no auth) |
| GET | `/ws?token=xxx` | WebSocket for push events |
| **Atoms** | | |
| GET | `/api/atoms` | List atoms (optional `?tag_id=x` filter) |
| GET | `/api/atoms/:id` | Get atom |
| POST | `/api/atoms` | Create atom |
| PUT | `/api/atoms/:id` | Update atom |
| DELETE | `/api/atoms/:id` | Delete atom |
| **Tags** | | |
| GET | `/api/tags` | List tags |
| POST | `/api/tags` | Create tag |
| PUT | `/api/tags/:id` | Update tag |
| DELETE | `/api/tags/:id` | Delete tag |
| **Search** | | |
| POST | `/api/search` | Search (body: `{ query, mode, limit?, threshold? }`) |
| GET | `/api/atoms/:id/similar` | Find similar atoms |
| **Wiki** | | |
| GET | `/api/wiki` | List all wiki articles |
| GET | `/api/wiki/:tag_id` | Get wiki article |
| GET | `/api/wiki/:tag_id/status` | Get article status |
| POST | `/api/wiki/:tag_id/generate` | Generate wiki article |
| POST | `/api/wiki/:tag_id/update` | Update wiki article |
| DELETE | `/api/wiki/:tag_id` | Delete wiki article |
| **Settings** | | |
| GET | `/api/settings` | Get all settings |
| PUT | `/api/settings/:key` | Set a setting |
| POST | `/api/settings/test-openrouter` | Test OpenRouter connection |
| GET | `/api/settings/models` | List available LLM models |
| **Embedding** | | |
| POST | `/api/embeddings/process-pending` | Process pending embeddings |
| POST | `/api/embeddings/process-tagging` | Process pending tagging |
| POST | `/api/embeddings/retry/:atom_id` | Retry embedding |
| POST | `/api/embeddings/reset-stuck` | Reset stuck processing |
| GET | `/api/atoms/:id/embedding-status` | Get embedding status |
| **Canvas** | | |
| GET | `/api/canvas/positions` | Get atom positions |
| PUT | `/api/canvas/positions` | Save atom positions |
| GET | `/api/canvas/atoms-with-embeddings` | Get atoms with embeddings |
| **Graph** | | |
| GET | `/api/graph/edges` | Get semantic edges |
| GET | `/api/graph/neighborhood/:atom_id` | Get atom neighborhood |
| POST | `/api/graph/rebuild-edges` | Rebuild semantic edges |
| **Clustering** | | |
| POST | `/api/clustering/compute` | Compute clusters |
| GET | `/api/clustering` | Get clusters |
| GET | `/api/clustering/connection-counts` | Get connection counts |
| **Chat** | | |
| POST | `/api/conversations` | Create conversation |
| GET | `/api/conversations` | List conversations |
| GET | `/api/conversations/:id` | Get conversation with messages |
| PUT | `/api/conversations/:id` | Update conversation |
| DELETE | `/api/conversations/:id` | Delete conversation |
| PUT | `/api/conversations/:id/scope` | Set conversation scope |
| POST | `/api/conversations/:id/scope/tags` | Add tag to scope |
| DELETE | `/api/conversations/:id/scope/tags/:tag_id` | Remove tag from scope |
| POST | `/api/conversations/:id/messages` | Send message (triggers agent loop) |
| **Ollama** | | |
| POST | `/api/ollama/test` | Test Ollama connection |
| GET | `/api/ollama/models` | List all Ollama models |
| GET | `/api/ollama/embedding-models` | List embedding models |
| GET | `/api/ollama/llm-models` | List LLM models |
| GET | `/api/provider/verify` | Verify provider configured |
| **Auth / Tokens** | | |
| POST | `/api/auth/tokens` | Create named API token (returns raw token once) |
| GET | `/api/auth/tokens` | List all tokens (metadata only) |
| DELETE | `/api/auth/tokens/:id` | Revoke a token |
| **Utils** | | |
| GET | `/api/utils/sqlite-vec` | Check sqlite-vec version |
| POST | `/api/utils/compact-tags` | Compact tags via LLM |

**WebSocket Events**: Clients receive `ServerEvent` JSON messages covering embedding pipeline events (`EmbeddingStarted`, `EmbeddingComplete`, `EmbeddingFailed`, `TaggingComplete`, `TaggingFailed`, `TaggingSkipped`) and chat streaming events (`ChatStreamDelta`, `ChatToolStart`, `ChatToolComplete`, `ChatComplete`, `ChatError`). All events include a `type` field for discrimination.

### MCP Server
The MCP integration provides tools for Claude and other MCP clients:
- `semantic_search(query, limit?, threshold?)` — Search atoms by semantic similarity
- `read_atom(atom_id, limit?, offset?)` — Read atom content

Available as:
1. **Integrated** — Runs within the Tauri app's HTTP server at `/mcp`
2. **Standalone** — `atomic-mcp` binary for use without the desktop app
3. **Bridge** — `mcp-bridge` binary for HTTP-to-stdio protocol bridging

## Design System (Dark Theme - Obsidian-inspired)

### Colors
- Background: `#1e1e1e` (main), `#252525` (panels), `#2d2d2d` (cards/elevated)
- Text: `#dcddde` (primary), `#888888` (secondary/muted), `#666666` (tertiary)
- Borders: `#3d3d3d`
- Accent: `#7c3aed` (purple), `#a78bfa` (light purple for tags)
- Status: `amber-500` (pending/processing), `red-500` (failed), `green-500` (success)

### Layout
- Left Panel: 250px fixed width
- Main View: Flexible, fills remaining space
- Right Drawer: 75vw width, slides from right as overlay

### Tag Display
- Tags are collapsed by default in AtomViewer and TagSelector
- Maximum 5 tags shown initially
- "+N more" button expands to show all tags
- "Show less" button collapses back to 5 tags

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
- `drawerState: { isOpen, mode, atomId, tagId, tagName, conversationId }` - Drawer state
- `viewMode: 'canvas' | 'grid' | 'list'` - Atom display mode (default: 'canvas', persisted to localStorage)
- `searchQuery: string` - Text search filter
- Drawer modes: `'editor' | 'viewer' | 'wiki' | 'chat'`
- Actions: `setSelectedTag`, `openDrawer`, `openWikiDrawer`, `openChatDrawer`, `closeDrawer`, `setViewMode`, `setSearchQuery`

### settings.ts
- `settings: Record<string, string>` - All settings as key-value pairs
- `isLoading: boolean`
- `error: string | null`
- Actions: `fetchSettings`, `setSetting`, `testOpenRouterConnection`

### wiki.ts
- `currentArticle: WikiArticleWithCitations | null` - Current wiki article
- `articleStatus: WikiArticleStatus | null` - Article status info
- `isLoading: boolean` - Loading state
- `isGenerating: boolean` - Generation in progress
- `isUpdating: boolean` - Update in progress
- `error: string | null` - Error message
- Actions: `fetchArticle`, `fetchArticleStatus`, `generateArticle`, `updateArticle`, `deleteArticle`, `clearArticle`, `clearError`

### chat.ts
- `view: 'list' | 'conversation'` - Current chat view
- `currentConversation: ConversationWithTags | null` - Active conversation
- `messages: ChatMessageWithContext[]` - Messages in current conversation
- `conversations: ConversationWithTags[]` - List of all conversations
- `listFilterTagId: string | null` - Filter for conversations list
- `isLoading: boolean` - Loading state
- `isStreaming: boolean` - Streaming response in progress
- `streamingContent: string` - Content being streamed
- `retrievalSteps: RetrievalStep[]` - Tool calls for transparency
- `error: string | null` - Error message
- Actions: `showList`, `openConversation`, `goBack`, `fetchConversations`, `createConversation`, `deleteConversation`, `updateConversationTitle`, `setScope`, `addTagToScope`, `removeTagFromScope`, `sendMessage`, `cancelResponse`, `appendStreamContent`, `addRetrievalStep`, `completeMessage`, `setStreamingError`, `clearError`, `reset`

### Similarity Calculation
- sqlite-vec returns Euclidean distance (lower = more similar)
- For normalized vectors, convert to similarity: `1.0 - (distance / 2.0)`
- Default threshold: 0.7 for related atoms, 0.3 for semantic search, 0.3 for wiki chunk selection
