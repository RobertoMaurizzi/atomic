# Atomic Remote Access — Product Spec

## Vision

Atomic is a personal knowledge base that gives users complete ownership of their data while making it accessible from anywhere. Users should never have to choose between privacy and convenience.

The core thesis: **users want access to their data from any device, but they want complete ownership of that data.** These two goals are traditionally in tension — cloud services offer convenience at the cost of control, while local-only tools offer control at the cost of reach. Atomic bridges this gap.

## What "complete ownership" means

Ownership is not just "you can export a zip file." It means:

- **You choose where it lives** — your machine, your server, your VM, not our SaaS by default
- **You can read it without our software** — SQLite is an open format, not a proprietary database
- **No account required for the core experience** — the app works fully offline without phoning home
- **AI processing is optional and configurable** — run Ollama locally for full privacy, or use OpenRouter if you choose to send data to a cloud provider
- **If the project disappears, your data survives** — no orphaned cloud database, no "sorry we're shutting down"
- **Your data is never commingled with other users' data** — even on managed hosting, each user is fully isolated

## Privacy Model

Atomic achieves credible privacy through **architecture and transparency**, not cryptography:

- **Open source** — the server, the client, the infrastructure-as-code. Users can read every line of code. If it doesn't phone home, doesn't collect analytics, doesn't send data anywhere — that's verifiable, not just promised.
- **SQLite-per-user isolation** — each user is their own process, their own database file. There's no shared database where a bug could leak data across users. This is a structural guarantee, not a policy.
- **Encrypted at rest** — standard disk encryption protects against physical disk theft, decommissioned hardware, and backup leaks.
- **"Deploy to your cloud" as a first-class option** — users can deploy to their own DigitalOcean/Fly/Railway account with one click. We provide the software and manage updates but never have access to their server or data.
- **No data mining, no analytics, no ad targeting** — users pay with money, not data.
- **Data export anytime** — a user's entire knowledge base is one SQLite file they can copy, back up, or take elsewhere.

This approach covers the vast majority of privacy-conscious users without the complexity cost of end-to-end encryption. For users who want maximum privacy, the fully-local desktop mode with Ollama ensures nothing ever leaves their machine.

## User Personas

### The Privacy Maximalist
- Uses Atomic desktop-only, fully local
- Runs Ollama for embeddings and LLM — nothing leaves their machine
- Would never create an account or connect to a server
- Atomic already serves this user today

### The Power User
- Technical enough to run a home server or VPS
- Wants sync across laptop, phone, and browser
- Self-hosts `atomic-server` on their own infrastructure
- Uses the browser extension to capture articles from any device
- May use OpenRouter for convenience, or Ollama on their server

### The Everyday User
- Cares about privacy but isn't going to run Docker or SSH into a server
- Wants the "just works" experience — sign up, install, go
- Willing to trust a managed hosting provider that credibly isolates their data
- This is the largest potential user base

## Deployment Tiers

All tiers run the same software. The only difference is who provisions and manages the infrastructure.

```
Most private                                          Most convenient
◄──────────────────────────────────────────────────────────────────►

Local only       Self-hosted      Your cloud,        Our infra,
(desktop)        (your metal)     our software       your instance

No network.      You run it.      One-click deploy   Sign up and go.
Full Ollama.     Full control.    to YOUR account.   Per-user isolation.
                                  We manage updates.  Encrypted at rest.
                                  We can't access     Open source.
                                  your server.
```

### Tier 1: Local Only (current)
- Fully local SQLite database on the user's machine
- No network, no account, no server
- AI via Ollama (local) or OpenRouter (user's own API key)
- Single-device only
- **Already exists today**

### Tier 2: Self-Hosted Server
- User runs `atomic-server` on their own hardware (bare metal, VPS, NAS, Raspberry Pi)
- Single binary or Docker image — no complex dependencies
- Server is the source of truth; desktop app and browser extension connect to it
- User controls backups, updates, and infrastructure
- Same isolation guarantees as local — it's their machine

### Tier 3: Deploy to Your Cloud
- One-click deploy templates for DigitalOcean, Fly.io, Railway, etc.
- Resources created in the USER's cloud account, on their billing
- We provide the Docker image and deployment config
- Optional managed update service (auto-pull new images)
- We never have credentials to their server or data
- Strongest privacy story for non-technical users who want a server

### Tier 4: Managed Hosting
- We provision and run an isolated `atomic-server` instance per user
- Each user gets their own process and their own SQLite database — no shared infrastructure, no multi-tenancy
- User signs up, pays, and gets a URL. No DevOps required
- Encrypted at rest, per-user process isolation
- Users can export their database at any time, or migrate to self-hosted with zero friction

## Architecture

### Core Principle: One Binary, Many Contexts

The `atomic-core` Rust crate already contains all business logic with zero Tauri dependency. The architecture extends this:

```
┌─────────────────────────────────────────────────────┐
│                    atomic-core                       │
│  Database · Providers · Search · Wiki · Embedding    │
│  Chunking · Clustering · Chat · Tag Extraction       │
│                                                      │
│  Standalone Rust library. No framework dependency.   │
│  Callback-based events. SQLite storage.              │
└──────────┬──────────────────┬───────────────────────┘
           │                  │
  ┌────────▼────────┐  ┌─────▼──────────────────────┐
  │  atomic-desktop  │  │     atomic-server           │
  │  (Tauri app)     │  │  (standalone Rust binary)   │
  │                  │  │                             │
  │  - Local SQLite  │  │  - atomic-core              │
  │  - Optional      │  │  - actix-web (REST + WS)    │
  │    server conn   │  │  - Auth layer               │
  │  - Embedded HTTP │  │  - WebSocket events         │
  │    server (ext)  │  │  - Same SQLite per user     │
  └──────────────────┘  └─────┬──────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
     ┌────────▼──┐   ┌───────▼───┐   ┌───────▼────────┐
     │  Web app   │   │  Browser  │   │  Mobile app    │
     │  (React)   │   │ Extension │   │  (future)      │
     │            │   │           │   │                │
     │ HTTP + WS  │   │ POST to   │   │ HTTP + WS     │
     │ transport  │   │ /atoms    │   │ transport      │
     └────────────┘   └───────────┘   └────────────────┘
```

### Server-Side Processing

The server handles all AI processing — embeddings, tag extraction, wiki synthesis, chat/RAG. Clients are thin; they send content and receive results. This is the same model the desktop app uses locally today, just over HTTP.

```
Thin clients                          Server (the brain)
─────────────                         ──────────────────
Mobile app     ──POST content──►      atomic-server
Browser ext    ──POST content──►        - Stores atom
Web app        ──POST content──►        - Generates embeddings
Desktop (remote mode)──────────►        - Extracts tags
                                        - Indexes for FTS + vector search
               ◄──events (WS)──        - Runs wiki/chat/RAG
                                        - All AI processing via configured provider
```

Benefits of server-side processing:
- **One implementation** — AI pipeline exists once in `atomic-core`, tested once, deployed once
- **Thin clients** — mobile app, browser extension, and web client are just HTTP calls with no AI runtime
- **Provider flexibility** — server can use Ollama (local to server) or OpenRouter, configurable per instance
- **No client capability requirements** — a phone doesn't need to run embedding models

The only client that does its own AI processing is the desktop app in fully-local mode (Tier 1), which already works today.

### AI Provider Per Tier

The server's AI provider is a configuration choice, not an architectural one — the existing `ProviderConfig` already supports OpenRouter and Ollama with a configurable host URL.

- **Local only / Self-hosted**: User runs Ollama on the same machine or network, or uses their own OpenRouter key. Full control.
- **Deploy to your cloud**: Same as self-hosted — user can run Ollama alongside their instance or use OpenRouter.
- **Managed hosting**: Requires a cloud AI provider since we're not running GPUs per user. Users bring their own OpenRouter key, point at their own remote Ollama (`ollama_host` is already configurable to any URL), or we bundle AI into the subscription cost by proxying through a platform-managed API key. The everyday user on managed hosting is already trusting a hosting provider — trusting an AI API provider is a similar level of trust.

### SQLite-Per-User Isolation

Every user — whether self-hosted or managed — gets their own SQLite database file and their own server process. This is a deliberate architectural choice:

- **True isolation**: No shared database, no data commingling, no cross-user bugs
- **Simple security model**: Process-level isolation, no row-level access control needed
- **Portable**: A user's entire knowledge base is one `.db` file they can copy, back up, or migrate
- **Lightweight**: SQLite processes are cheap. Hundreds of users can run on a single host machine, or each user gets a micro-VM (Fly Machine, Firecracker) for stronger isolation
- **No migration complexity**: Same schema whether local, self-hosted, or managed

### Transport Abstraction

The React frontend currently communicates with the Tauri backend via `invoke()` calls and Tauri events. To support both desktop (Tauri) and web (HTTP/WebSocket) contexts, the frontend needs a transport abstraction:

```
Frontend Store (e.g., atoms.ts)
       │
       ▼
  Transport Layer
       │
       ├── TauriTransport    → invoke() + listen()     (desktop app, local mode)
       └── HttpTransport     → fetch() + WebSocket     (web app, mobile, desktop remote mode)
```

Both transports implement the same interface. The data shapes are identical — only the transport mechanism changes. The desktop app already has an HTTP server running (port 44380) that reuses `atomic-core` business logic, proving this works.

### Event System

Real-time events (embedding progress, chat streaming, tag extraction) currently use Tauri's event bus. For web clients, these map to WebSocket messages:

| Tauri Event | WebSocket Message | Purpose |
|---|---|---|
| `embedding-complete` | `{"type": "embedding-complete", ...}` | Embedding finished |
| `tagging-complete` | `{"type": "tagging-complete", ...}` | Tag extraction finished |
| `chat-stream-delta` | `{"type": "chat-stream-delta", ...}` | Streaming chat token |
| `chat-complete` | `{"type": "chat-complete", ...}` | Chat response finished |
| `atom-created` | `{"type": "atom-created", ...}` | New atom (e.g., from extension) |

The `atomic-core` crate already uses callback-based events (`Fn(EmbeddingEvent)`), which map cleanly to either Tauri emit or WebSocket broadcast.

## Components

### atomic-server (new binary)

A standalone Rust binary that serves the full Atomic API over HTTP and WebSocket. It wraps `atomic-core` and handles all AI processing server-side.

**What it does:**
- Wraps `atomic-core` with REST endpoints (one endpoint per Tauri command)
- Handles embedding generation, tag extraction, wiki synthesis, and chat server-side
- WebSocket endpoint for real-time events
- Authentication (token-based initially)
- Serves the web client as static files (optional, can be separate)
- TLS termination delegated to reverse proxy (nginx, Caddy, Cloudflare Tunnel)

**What it doesn't do:**
- No multi-tenancy — one process per user
- No user management — that's the provisioning layer's job (managed tier)
- No payment processing — separate billing service for managed tier

**Deployment:**
- Single binary, no runtime dependencies (SQLite is bundled)
- Docker image for convenience
- Configurable via environment variables or config file
- Data stored in a single directory (SQLite database + any future file attachments)

### Web Client

The existing React frontend, adapted to use HTTP/WebSocket instead of Tauri:

**Approach:**
- Same React components, same Zustand stores, same UI
- Replace `invoke()` calls with HTTP fetch via a transport abstraction
- Replace Tauri `listen()` with WebSocket subscriptions
- Remove Tauri-specific plugins (dialog → HTML file input, opener → window.open)
- Build as a static SPA that can be served by atomic-server or any CDN/static host

**Scope:**
- Full feature parity with the desktop app (read, write, search, wiki, chat, canvas)
- Responsive design for mobile browsers (future consideration)

### Browser Extension

Expands on the existing browser extension support:

**Current state:** The desktop app runs an HTTP server on port 44380 with a `POST /atoms` endpoint.

**Extended for server mode:**
- Extension configured with server URL + auth token (instead of localhost)
- Capture current page content (article text, selection, URL)
- One-click save to knowledge base — server handles embedding and tagging automatically
- Works from any device with a browser — no desktop app needed
- Optional: quick search of existing atoms from extension popup

### Desktop App Changes

The existing Tauri app gains the ability to connect to a remote server:

**New capability:** "Connect to Server" option in settings
- Enter server URL + auth token
- Desktop app switches from local SQLite to HTTP/WebSocket transport
- All operations go through the server (including AI processing)
- Offline mode: queue changes locally, sync when reconnected (future enhancement)

**Local mode preserved:** Users can still use Atomic in fully local mode with no server. This remains the default.

### Mobile App (future)

A thin client that connects to `atomic-server`:

- Likely React Native or a lightweight native shell around the web client
- Read, search, and capture-focused (not full editing)
- All AI processing happens on the server — mobile app is just an interface
- Push notifications for completed embeddings or chat responses
- Camera/share sheet integration for quick capture

## Authentication

Even self-hosted servers need authentication to prevent unauthorized access.

### Current Implementation: Named, Revocable API Tokens
- Server auto-creates a "default" token on first run, printed to stdout
- Tokens are named (e.g. "laptop", "phone", "browser-extension") for identification
- Token format: `at_` prefix + 32 random bytes base64url-encoded (~46 chars)
- Tokens stored as SHA-256 hashes in `api_tokens` table (never stored in plaintext)
- Each token tracks `last_used_at` for visibility into active sessions
- Individual tokens can be revoked without affecting other devices
- CLI management: `atomic-server token create/list/revoke`
- REST API management: `POST/GET/DELETE /api/auth/tokens` (requires valid token)
- Web client: paste token to connect, token management UI in Settings
- 401 response on revoked/invalid token clears client localStorage and redirects to login
- Legacy migration: old `server_auth_token` setting auto-migrated to api_tokens table

### Future: Account-Based Auth
- Email + passphrase for managed tier onboarding
- OAuth for convenience (Sign in with Apple/Google) — optional
- Session tokens with expiry and refresh

## User Experience Flows

### New User — Managed Hosting
1. User visits atomic.app (or similar)
2. Downloads desktop app, or opens web app directly
3. Creates account (email + passphrase)
4. Instance provisioned in background (~5 seconds)
5. Guided setup: install browser extension, configure AI provider (or use defaults)
6. User starts creating atoms — everything syncs automatically
7. Opens phone browser, logs in at their URL — sees all their data

### New User — Deploy to Your Cloud
1. User visits atomic.app, chooses "Host it yourself"
2. Clicks "Deploy to Fly.io" (or DigitalOcean, Railway, etc.)
3. Redirected to cloud provider, authorizes resource creation
4. Instance deployed to user's own cloud account (~2 minutes)
5. Redirected back with server URL
6. Configures desktop app and browser extension with URL + token
7. We never touch their data — it lives in their cloud account

### Existing Desktop User → Adding Server
1. User has been using Atomic locally for months
2. Goes to Settings → "Connect to Server"
3. Options: "Self-host" (shows instructions), "Deploy to your cloud" (one-click), or "Get managed instance" (signup flow)
4. "Import local database" — uploads their existing `.db` file to the server
5. Desktop app switches to server mode
6. Installs browser extension, connects to server
7. Local data is now accessible everywhere

### Self-Hosted Setup
1. User runs `docker run -v atomic-data:/data -p 443:8080 atomic-server`
2. First-run wizard in browser: set passphrase, get API token
3. Configures desktop app with server URL + token
4. Configures browser extension with server URL + token
5. Optionally sets up Ollama on the same machine for fully private AI
6. Optionally points a domain at it with Caddy/Cloudflare for HTTPS

## Phasing

### Phase 1: atomic-server MVP
**Goal:** A working standalone server that the web client and browser extension can talk to.

- New `atomic-server` crate in workspace: `atomic-core` + actix-web + WebSocket
- Full REST API surface (all Tauri commands become HTTP endpoints)
- Server-side AI processing (embeddings, tagging, wiki, chat) — same pipeline as desktop
- WebSocket endpoint for real-time events
- Token-based authentication
- Static file serving for web client
- Docker image and standalone binary releases
- Browser extension updated to support configurable server URL

### Phase 2: Web Client
**Goal:** Full Atomic experience in a browser.

- Frontend transport abstraction (Tauri invoke ↔ HTTP fetch)
- WebSocket event subscriptions replacing Tauri listen
- Remove Tauri-specific plugin dependencies
- Build pipeline producing both Tauri app and static web SPA from same source
- Desktop app gains "connect to server" settings option

### Phase 3: Hosting & Deployment
**Goal:** Non-technical users can get a working instance without managing servers.

- One-click deploy templates for Fly.io, Railway, DigitalOcean (Tier 3)
- Auto-update mechanism for deploy-to-your-cloud instances
- Provisioning service for managed hosting (Tier 4): creates isolated instances per user
- Billing integration (Stripe or similar)
- User dashboard: manage instance, view usage, export data
- Automated backups
- Instance lifecycle: sleep idle instances, wake on request

### Phase 4: Enhanced Sync & Mobile
**Goal:** Polish the multi-device experience.

- Desktop offline mode with change queuing and sync on reconnect
- Mobile app (capture-focused, thin client — all AI on server)
- Database migration tool (import local → server, export server → local)
- Share individual atoms or wiki articles via public links (optional, user-controlled)

## Open Questions

1. **Offline-first vs server-first for the desktop app?** When connected to a server, should the desktop app maintain a local cache/replica for offline use, or treat the server as the sole source of truth? Local cache adds complexity (conflict resolution) but enables offline work.

2. **AI provider management on managed hosting.** Should managed instances include AI (embeddings + LLM) bundled in, or should users always bring their own API key? Bundling is simpler UX but adds cost and means content passes through more services.

3. **Pricing model for managed tier.** Per-user flat rate? Usage-based (storage + AI calls)? Free tier with limits?

4. **Web client as a separate app or same codebase?** The React frontend could be built from the exact same source with a compile-time transport switch, or forked into a separate web-specific app. Same source is cleaner but may accumulate platform-specific conditionals.

5. **Database migration path.** When a user moves from local to server (or vice versa), do we copy the entire SQLite file, or do we export/import through the API? File copy is simpler but skips validation; API import is safer but slower for large databases.

6. **Browser extension authentication UX.** How does the extension get the auth token? QR code scan from the desktop app? Manual paste? OAuth redirect? This is a friction point that matters for non-technical users.
