# Temporary Grill Review — Vault, Projects/Tabs UI/UX, MCP Connector & Cross-Device Transfer

> **TEMPORARY REVIEW DOCUMENT** — Created 2026-11-22 after a 4-round grilling session (Q1–Q33) to allow a fresh AI agent session to independently critique, stress-test, and review all architectural, UX, cryptographic, and sequencing decisions before implementation begins. Delete this file once the review is complete (remember `docs-lint` forbids `docs/design/`).
>
> **Permanent records created alongside this doc:**
> - ADRs: `docs/adr/0125-unified-encrypted-vault-and-projects.md`, `docs/adr/0126-local-mcp-ai-connector.md`, `docs/adr/0127-cross-device-p2p-vault-transfer.md`
> - Specs updated: `docs/specs/desktop.md`, `docs/specs/mobile.md`, `docs/specs/cases-and-privacy.md`
> - Glossary & Roadmap updated: `CONTEXT.md`, `ROADMAP.md`

---

## 1. Context & Problem Audit (From Screenshots + Codebase)

We audited 8 live screenshots (3 macOS desktop, 5 iOS) and the corresponding code (`src/workspace/{sidebar,tab_set,case_page,cases_ui,shell}.rs`, `src/shelf.rs`, `apps/ios/mdoc/Library/{SidebarView,CasesView,DocumentsPage,IntakeSheets,LibraryModel}.swift`, `apps/ios/mdoc/App/{AppModel,Navigator,DocumentStore}.swift`, `crates/mdoc-store/src/cases.rs`, `crates/mdoc-ffi/src/library.rs`).

### Observed Failures
1. **Desktop sidebar spatial instability & hidden members:**
   - Cases disappear from the sidebar whenever no tab is open.
   - Clicking a Case on Home opens a transient `CasePage` and renders the case *above* `Open`; opening its document jumps the entire case section *below* `Open`.
   - Closed member documents of a Case are invisible in the sidebar (only open tabs appear).
   - Home duplicates the Cases list and Recents list alongside the sidebar.
   - `CasePage` immediately dumps 60+ raw identities on screen, crowding out documents and actions.
2. **iOS Case proliferation & broken drill-down:**
   - `IntakeSheets.swift` defaults every Scan / Files / Paste / New import to creating a **New case** named after the document (`existingCase: CaseID? = nil`), resulting in 15+ throwaway cases (`IMG_0089`, `Photo Nov 22…`, `Cloud request … 2/3/4`), most holding `0 documents`.
   - `CasesView.swift` has no Delete Case action, so empty orphan cases accumulate permanently.
   - Tapping a Case calls `app.openCase(id)` which merely sets `library.caseFilter = id` and opens `DocumentsPage`—whose navigation title still says **"Documents"** with only a tiny filter icon and no Project overview (no identities, replies, color, or settings).
   - Sidebar hides all Projects behind a single `Cases 15` row.
3. **Storage fragmentation & partial plaintext at rest:**
   - Desktop currently forbids managed storage (`docs/specs/desktop.md`), leaving `.md` files and originals scattered across `Downloads`/folders referenced by brittle absolute paths in `.case` files.
   - Mobile has a managed `Library` (`cases/`, `documents/`, `originals/`, `documents.json`), but **only `.case` files** are `AES-256-GCM` encrypted; Markdown files (which hold unreplaced originals during review), Source Originals (PDFs/DOCX/scans), and filenames/titles in `documents.json` rely solely on OS filesystem encryption.
   - Desktop (`mdoc_store::cases`) and Mobile (`mdoc_ffi::library`) maintain two diverging storage implementations.
4. **Terminology collision:**
   - `Case` (container of documents) collides with `Case number` (`CASE_1` PII category) and feels overly narrow outside litigation.

---

## 2. Complete Confirmed Decision Log (Q1–Q33)

| # | Topic | Confirmed Decision | Rejected / Notes |
|---|---|---|---|
| **Q1** | Vault scope | Unified encrypted **Vault** across **both** Desktop and Mobile. | Mobile-only or opt-in-only vault. |
| **Q2** | Projects vs. Unfiled | Rename **Case → Project**. Standalone documents live **Unfiled** by default (with document-local mappings); Projects are explicit containers. | Auto-creating a case/project on every import. |
| **Q3** | Encryption scope & unlock | Encrypt **all** vault blobs and metadata index at rest with opaque UUID filenames on disk; unlocked transparently via existing OS Keychain / DPAPI / Secure Enclave (no mandatory master password at launch; optional biometric/PIN lock later). | Partial plaintext (`documents/`, `originals/`) or mandatory launch passphrase. |
| **Q4** | M2 connector protocol | Single local **MCP** contract (`mdoc-mcp` talking to desktop `mdoc` via local authenticated IPC); ship **Claude Desktop `.mcpb`** first, validate **ChatGPT Desktop** on the same contract. | Separate proprietary plugins per AI app. |
| **Q5** | M2 privacy invariant | **Hard invariant:** Ready-only documents, pseudonymized titles/project labels in `list_*`, and **one-way restore** (AI replies restore exclusively inside `mdoc` UI; never return real names in MCP responses). | Returning restored text in MCP tool outputs (would leak real names into AI context). |
| **Q6** | M3 transfer scope | **Explicit, user-initiated** encrypted send/receive of a Project or Document. | Always-on continuous background sync / accounts / cloud servers. |
| **Q7** | M3 transport stack | **Pure-Rust `iroh`** (`iroh` + `iroh-blobs`) delivering Tailcat-style zero-account P2P + relay-fallback UX through our existing Rust/UniFFI pipeline. | Embedding Go `tailcat` (would add Go + `gomobile`/cgo to all 4 platforms and CI). |
| **Q8** | Milestone sequencing | **M1:** Vault + Case→Project/Doc/Tab UI/UX redesign (with M2/M3 schema hooks) → **M2:** Desktop MCP Connector → **M3:** Cross-Device `iroh` Transfer. | Building M2/M3 on top of old loose-file storage. |
| **Q9** | Clean break rename | Rename `Case` → `Project` everywhere (UI, docs, specs, Rust, FFI, Swift, Kotlin). **No legacy `.case` migration code** (early dev, zero production users). | Carrying legacy `.case` migrator baggage. |
| **Q10** | Desktop layout | **Left Sidebar = Persistent Vault Tree** (Search, Home, expandable Projects showing open + closed docs, Unfiled, open External `.md`) + **Top Horizontal Tab Bar** in main pane for active working tabs (Documents, Replies, Project Overview). | Vertical-only `Open` section duplicating Project rows. |
| **Q11** | Mobile layout | Mirror Desktop Vault tree in sidebar (`Home`, `To review`, inline `Projects`, `Unfiled / All Documents`); replace filtered-Documents hack with a true **Project Page**; default intake to **Unfiled** (or current Project when adding inside one). | Keeping `Cases 15` flat list + filtered `DocumentsPage`. |
| **Q12 / Q19** | Project item types | **Exactly 2 first-class item types:** (1) **Documents** (any supported file + Source Original + Markdown/review state, including unconverted/unscanned or freeform Markdown notes) and (2) **AI Replies** (saved pseudonymized/restored responses, promotable to Document). | **Dropped** "Reference Attachments" (3rd arbitrary binary type) as out-of-scope file-cabinet bloat. |
| **Q13** | Storage engine | Pure-Rust **encrypted file-per-object + encrypted `manifest.enc`**, atomic write-temp-fsync-rename + brief advisory lock. | SQLCipher / SQLite (C cross-compile complexity + Apple iOS App Group `0xdead10cc` WAL lock kills in `RestoreExtension`). |
| **Q14 / Q20** | Cryptographic design | **Balanced, pragmatic crypto:** Reuse the single 32-byte OS Keychain/DPAPI key + existing `aes-gcm` crate (zero new crypto crates). Single-shot `AES-256-GCM` (random 12B nonce + AAD) for structured files; simple **1 MiB chunked `AES-256-GCM`** (~40 lines) for `original.enc` to avoid mobile OOM on 256 MiB scans. | HKDF + per-object wrapped DEK hierarchy (over-engineered for single-user local threat model). |
| **Q15** | Document state model | **Single source of truth** per Document: one working editable Markdown + structured per-mention review state (**including Keep & Not-a-name per mention**) + document-local mapping when Unfiled + immutable Source Original + cached **`ready_snapshot`** (SHA-256 + clean pseudonymized Markdown projection). | Dual diverging Markdown files (Original `.md` + Pseudonymized `.md`). |
| **Q16** | Desktop external files | Source formats (PDF, DOCX, XLSX, PPTX, ODT, RTF, EPUB, CSV, images, scans, paste) always **import a copy into the Vault**. Opening loose external `.md` stays lightweight external (badged `External`, explicit save prompts, one-click `Save to Vault`); `Export…` writes `.md` out. Clean break (no legacy session migration). | Keeping loose file references as the primary desktop workflow. |
| **Q17** | M2 IPC & gating | Desktop `mdoc` hosts an **authenticated local IPC listener** (`0600` Unix socket on macOS / user-SID Named Pipe on Windows + per-install random token); thin `mdoc-mcp` shim translates MCP over IPC; explicit per-Project **`Allow AI access`** toggle (off by default) + live activity indicator. | Giving `mdoc-mcp` direct Vault/Keychain access. |
| **Q18** | M3 pairing & merge | One-time ephemeral **QR code + short copyable code** carrying `{node_addr, ephemeral_auth_secret, manifest_hash}`; receiver confirmation preview; **non-destructive merge** on re-sending an existing `ProjectId` (stable alias union + conflict copy if same `DocumentId` diverged). | Blind overwrite on re-transfer or permanent device pairing / sync daemon. |
| **Q21** | Desktop Home & sidebar details | Sidebar never jumps; Home becomes a focused action/triage launcher (drop zone, quick actions, Continue / Needs review) instead of duplicating the full Projects list; tab bar hides on Home when no tabs are open; `⌘W` closes tab without touching Vault. | Duplicating full Projects/Recents lists on Home. |
| **Q22** | Project Overview anatomy | Identical **3-segment layout** on Desktop (tab) and Mobile (screen): Header (color picker, editable title, stats, primary `+ Add document`, `⋯` menu) + segments **`Documents`** *(default)* · **`Replies`** · **`Names`** (searchable, category-grouped identities + Always/Never). | Dumping 60+ identities on top of the Project screen. |
| **Q23** | Move & Delete rules | Moving Unfiled/Project doc into a Project merges matching originals and **rewrites document aliases** to target Project's aliases (undoable toast); deleting a non-empty Project always prompts **"Move documents to Unfiled"** vs **"Delete project and its N documents"**. | Silent deletion or zombie 0-doc projects. |
| **Q24 / Q30** | Crate architecture | New dedicated GUI-free crate **`crates/mdoc-vault`** as the single owner consumed by Desktop GPUI, iOS/Android (`mdoc-ffi`), macOS Shelf, M2 MCP, and M3 Transfer. Retires `mdoc_store::cases` and `mdoc_ffi`'s separate `library/` storage. | Maintaining split desktop & mobile storage implementations. |
| **Q25** | M2 5-tool MCP surface | 5 minimal tools: `list_projects`, `list_documents`, `get_document`, `get_project_aliases`, `deliver_reply`. Support **`stdio`** (primary) + optional **`127.0.0.1` Streamable-HTTP/SSE** transport if ChatGPT Desktop needs HTTP. | Broad filesystem-like MCP tools or unmasked listings. |
| **Q26** | M3 relay policy | Direct LAN/P2P first + `iroh` public relay fallback enabled by default (disclosed in `privacy.md`), with a **"Local network only (never use relay)"** toggle in Settings; relay URL configurable for future self-hosting. | Mandatory self-hosted relay before launch, or strictly LAN-only with no cellular fallback. |
| **Q27** | Desktop autosave | **All Vault documents autosave seamlessly** on desktop (no "Save changes?" prompts; `⌘S` flushes; undo stays in-memory per session). Only loose external `.md` keeps explicit dirty/save prompts. | Manual `⌘S` prompts on managed Vault docs. |
| **Q28** | Vault edge cases | Keep password-protected originals **still locked** inside `original.enc` (#1024); desktop `session.json` stores opaque UUIDs only (zero client names); M1 search stays instant over in-RAM `manifest.enc` (titles + project names; full-text body search Later). | Storing unlocked PDFs/DOCX on disk or building an encrypted FTS index in M1. |
| **Q29** | Mobile & UX polish | iPhone keeps single-active-document nav via Vault drawer (no cramped phone tab strip); Projects sort **Pinned → Recently Updated**, Docs **Recently Updated** (drag-onto-Project to move on desktop); Unfiled docs also support saved **AI Replies** (linked to `doc_id`, auto-migrating if doc moves to a Project). | Phone tab strip; blocking replies on Unfiled docs. |
| **Q31** | M2 operational edges | Closed/locked `mdoc` returns clear actionable MCP error (never auto-launches or touches Keychain headless); **Unfiled docs excluded from MCP** (must be in an AI-enabled Project; all `Ready` docs exposed, optional per-doc exclude switch); `deliver_reply` alerts via non-focus-stealing notification/badge + open Reply tab inside `mdoc`. | Auto-launching GUI or stealing OS window focus mid-chat. |
| **Q32** | M3 payload & expiry | Include **Source Originals by default** with upfront total size + **"Include original files"** toggle on send sheet; double-encrypted chunked stream over `iroh` QUIC; auto-expire listening session after 1 transfer, cancel, or 5 minutes. | Long-lived open listening ports or text-only transfers. |
| **Q33** | Documentation | Record in `ROADMAP.md`, `CONTEXT.md`, `docs/specs/{desktop,mobile,cases-and-privacy}.md`, ADRs `0125`–`0127`, plus this temporary grill review file. | Jumping straight to code without aligned specs. |

---

## 3. Detailed Architecture Specification

### 3.1 `crates/mdoc-vault` On-Disk Layout & Crypto

```text
<vault_root>/                       # App data dir (desktop) / App Group container (iOS) / no_backup (Android)
  .lock                             # Advisory file lock (flock / LockFileEx) held briefly during writes
  manifest.enc                      # Single-shot AES-256-GCM: VaultManifest (projects, docs, replies summaries)
  user-lists.enc                    # Single-shot AES-256-GCM: user-wide Always/Never lists
  projects/
    <project_uuid>.enc              # Single-shot AES-256-GCM: ProjectRecord
  docs/
    <doc_uuid>/
      doc.enc                       # Single-shot AES-256-GCM: DocumentRecord
      original.enc                  # 1 MiB chunked AES-256-GCM: OriginalHeader + raw immutable source bytes
  replies/
    <reply_uuid>.enc                # Single-shot AES-256-GCM: AiReplyRecord
```

#### Wire Formats (Zero new crypto crates — `aes-gcm` 0.11 only)
1. **Single-shot record (`manifest.enc`, `user-lists.enc`, `project.enc`, `doc.enc`, `reply.enc`):**
   - Header: `magic: [u8; 8]` (e.g. `b"MDOCVLT1"`) | `kind: u8` | `nonce: [u8; 12]` (OS CSPRNG) | `ciphertext_and_tag`
   - AAD: `magic || kind || object_id_bytes` (prevents cross-file swap/substitution attacks).
   - Write protocol: serialize JSON/msgpack → encrypt → write to sibling `NamedTempFile` → `sync_all()` → atomic rename over target.
2. **Chunked stream (`original.enc`, up to 256 MiB):**
   - Header (unencrypted prefix authenticated via AAD): `magic: [u8; 8]` (`b"MDOCORG1"`) | `chunk_size: u32` (1 MiB default) | `nonce_prefix: [u8; 8]` (OS CSPRNG)
   - Chunk `i` (`i: u32`, 0-based):
     - Nonce = `nonce_prefix (8B) || i.to_be_bytes() (4B)` (never repeats within file; 64-bit random prefix per file; note: check birthday bound if `original.enc` is immutable write-once — even 10M originals gives $p < 10^{-5}$, OR use a **12-byte random base nonce XOR'd with chunk index**? **Reviewer note:** verify 8B prefix vs 12B random base XOR `u32` — XOR with 12B random nonce has full 96-bit collision resistance! Good detail to verify in fresh review!).
     - AAD = `magic || doc_uuid || chunk_index (u32) || is_final (u8)` (prevents chunk reordering, truncation, or cross-doc transplant).
     - Chunk 0 starts with length-prefixed `OriginalMeta` JSON (original filename, MIME/source kind, page count, `password_protected: bool`, import provenance/left-out note), followed immediately by raw source bytes (still password-protected if locked, per #1024).

#### Core Rust Data Structures (Sketch)
- `ProjectId(Uuid)`, `DocumentId(Uuid)`, `ReplyId(Uuid)` (lowercase hex UUIDv4 or v7; UUIDv7 gives natural time ordering too — reviewer: weigh v4 vs v7!).
- `VaultManifest`:
  - `schema_version: u32`
  - `projects: Vec<ProjectSummary>` (`id`, `name`, `color`, `pinned`, `allow_ai`, `doc_count`, `reply_count`, `identity_count`, `updated_at`, `created_at`)
  - `documents: Vec<DocumentSummary>` (`id`, `project_id: Option<ProjectId>`, `title`, `source_kind`, `origin`, `phase`, `pending_count`, `replaced_count`, `kept_count`, `ai_excluded`, `has_original`, `updated_at`, `opened_at`)
  - `replies: Vec<ReplySummary>` (`id`, `project_id: Option<ProjectId>`, `document_id: Option<DocumentId>`, `title`, `source_app`, `unresolved_count`, `created_at`)
- `ProjectRecord`:
  - `id`, `name`, `color`, `pinned`, `allow_ai`, `created_at`, `updated_at`, `revision: u64`
  - `identities: Vec<StoredIdentity>` (aliases never reused; supports `not_a_name` tombstones)
  - `always: Vec<String>`, `never: Vec<String>`
  - `categories: Option<CategoryOverrides>`
- `DocumentRecord`:
  - `id`, `project_id: Option<ProjectId>`, `title`, `source_kind`, `origin`, `ai_excluded: bool`, `created_at`, `updated_at`, `revision: u64`
  - `markdown: String` (single editable source of truth)
  - `scanned: bool`
  - `review_state`: per-mention decisions (applied spans, pending proposals, **per-mention Keep & Not-a-name** decisions, occurrence overrides)
  - `local_identities: Vec<StoredIdentity>` (populated when `project_id.is_none()`, cleared/reconciled into Project when moved)
  - `ready_snapshot: Option<ReadySnapshot>` (`markdown_for_ai: String`, `sha256: String`, `word_count: usize`, `alias_count: usize`, `computed_at: u64` — populated ONLY when `pending_count == 0 && replaced_count > 0 && no_known_originals_remain`)
  - `original_meta: Option<OriginalSummary>` (filename, byte_len, password_protected, import_warning, left_out)
- `AiReplyRecord`:
  - `id`, `project_id: Option<ProjectId>`, `document_id: Option<DocumentId>` (set for Unfiled docs or optionally source doc), `title`, `source_app: Option<String>` (`"Claude"`, `"ChatGPT"`, `"Pasted"`), `pseudonymized_markdown: String`, `created_at: u64`
  - Restoration happens on read against the Project's (or Unfiled doc's) live `RestoreMap` so identity edits/forgets immediately reflect in Reply view.

#### Concurrency & iOS `RestoreExtension`
- `RestoreExtension` shares the iOS App Group container under ~20–30 MiB RAM limit.
- To restore a shared AI reply: extension reads `manifest.enc` (tiny) + selected `projects/<id>.enc` (tiny), restores in memory, and optionally saves a new `replies/<id>.enc` + updates `manifest.enc` under `.lock`.
- Lock is held **only** during short file write + manifest commit (milliseconds), never across UI interaction or background suspension—eliminating `0xdead10cc`.
- On macOS/Windows desktop, main app + background autosave workers serialize writes through `Vault`'s internal mutex + `.lock`.

---

### 3.2 UI/UX Wireframe & Behavior Summary

#### Desktop (GPUI — macOS & Windows)
```text
┌──────────────────────────┬───────────────────────────────────────────────────────────────┐
│  ● ● ●   [◫]  ‹  ›       │ [● Acme Deal ▾] [MD MSA_v2.md ×] [✦ Claude Risk Memo ×]   [+] │
├──────────────────────────┼───────────────────────────────────────────────────────────────┤
│ [🔍 Search…          ⌘K] │  ● Acme Deal  >  MSA_v2.md       [Source] [Panel] [⋯] [Ready] │
│  ⌂ Home             ⇧⌘H  ├───────────────────────────────────────────────────────────────┤
│                          │                                                               │
│ PROJECTS             [+] │                 Document / Reply / Project View               │
│ ▾ ● Acme Deal         ⋯  │                                                               │
│     DOCX MSA_v2.md    ●  │   (Project Overview tab uses 3 clean segments:                │
│     PDF  SOW.pdf     12  │      [ Documents ]   [ Replies ]   [ Names ]  )               │
│ ▸ ● Vendor Dispute    ⋯  │                                                               │
│                          │                                                               │
│ UNFILED                  │                                                               │
│   PDF  CV_draft.md    ●  │                                                               │
│                          │                                                               │
│ EXTERNAL                 │                                                               │
│   MD   notes.md     ext  │                                                               │
│                          │                                                               │
│ ⚙ Settings        ◑   ?  │                                                               │
└──────────────────────────┴───────────────────────────────────────────────────────────────┘
```
- **Zero spatial jumping:** `Projects` stays permanently in place regardless of which tabs are open or closed.
- **Click Project row** → opens/focuses Project Overview tab; **click `▸`/`▾`** → expands/collapses all Project documents inline.
- **Drag document row** onto any Project or `Unfiled` → moves document + reconciles aliases with undoable toast.
- **Home** (`⇧⌘H`): Focused action launcher (Drop zone, Open files, Paste text, Paste AI reply, Pseudonymize clipboard) + compact *"Continue / Needs review"* strip—no longer duplicates the full Projects list.
- **Tab Bar:** Holds open Documents, AI Replies, and Project Overview tabs; hides cleanly on Home when 0 tabs are open. `⌘W` closes tab without removing anything from the Vault.

#### Mobile (iOS SwiftUI & Android Compose)
- **Sidebar (iPhone drawer / wide iPad column):**
  - Search (`Search documents and projects`)
  - `Home` (Start: `Continue` card + `Scan` / `Files` / `Paste`)
  - `To review` (only when non-empty, orange pending count pill)
  - **`Projects`** (top 5 Pinned/Recent directly tappable with color dot + doc count, `All Projects` when >5, `+ New Project`)
  - **`Unfiled`** / **`All Documents`**
  - Floating bottom bar: Settings, Theme, `+ New`
- **True Project Page** (tapping any Project):
  - Navigation bar displays **Project color dot + Project name** (never `"Documents"`!), primary `+` button (adds directly into this Project), and `⋯` menu (Rename, Color, Categories, Always/Never, Send to Device [M3], Delete Project with Safe Move-to-Unfiled vs Delete-All confirmation).
  - 3 clean segments: **`Documents`** *(default)* · **`Replies`** · **`Names`**.
- **Intake default:** Scan / Files / Paste / New from Home or Sidebar defaults to **Unfiled** (with optional Project picker); triggering `+` from *inside* a Project defaults to that Project.
- **iPhone navigation:** Keeps fast single-active-document switching via the drawer (no tab bar wasting vertical space).

---

### 3.3 Milestone 2 — Local MCP AI Connector

#### Runtime Topology
```text
Claude Desktop (.mcpb) / ChatGPT Desktop
        │  MCP JSON-RPC (stdio primary; optional 127.0.0.1 Streamable-HTTP/SSE)
        ▼
   [ mdoc-mcp ]   (thin stateless shim; zero Keychain / vault file access)
        │  Authenticated Local IPC
        │  - macOS: ~/Library/Application Support/mdoc/mcp.sock (0600)
        │  - Windows: \\.\pipe\mdoc-mcp-<user-sid> (owner-only DACL)
        │  - Auth: per-install 256-bit bearer token (in app-private 0600 file or Keychain)
        ▼
  [ Desktop mdoc ]  (owns unlocked Vault, enforces Ready-only + Allow-AI + UI alerts)
```

#### Exact 5-Tool Contract
1. `list_projects() -> [{ project_id, label_pseudonymized, ready_document_count }]`
   - Filters strictly to `project.allow_ai == true`.
   - `label_pseudonymized` masks any known originals in the project name (same masker as `src/shelf.rs`).
2. `list_documents(project_id?: string) -> [{ doc_id, project_id, title_pseudonymized, word_count, alias_count, updated_at }]`
   - Filters strictly to `project.allow_ai == true && !doc.ai_excluded && doc.ready_snapshot.is_some()`.
   - Never returns Unfiled docs, unscanned docs, or docs with pending proposals / unreplaced known names.
3. `get_document(doc_id: string) -> { doc_id, project_id, title_pseudonymized, markdown, alias_instructions }`
   - Serves `ready_snapshot.markdown_for_ai` + standard alias-preservation instruction.
   - Emits a live activity indicator in `mdoc` (e.g. subtle pill/toast: `"Claude read MSA_v2"`).
4. `get_project_aliases(project_id: string) -> { project_id, aliases: [{ alias, category }] }`
   - Returns ONLY alias tokens and categories (`PERSON_1: Person`, `ORG_1: Organization`). **Never originals.**
5. `deliver_reply(project_id: string, title: string, pseudonymized_markdown: string, source_app?: string) -> { reply_id, status: "restored_in_mdoc", restored_count, unresolved_tokens: [string] }`
   - Saves `AiReplyRecord` to Vault under `project_id`, computes local restoration inside `mdoc`, opens/highlights the restored Reply tab inside `mdoc`, and posts a **non-focus-stealing** OS notification / in-app banner.
   - **NEVER** returns restored plaintext or real names in the MCP tool response.

#### Closed / Locked Behavior
- If `mdoc` is not running or its Vault is locked, `mdoc-mcp` returns an MCP tool error: `"mdoc is not running or its vault is locked. Open mdoc on your desktop to allow AI access."` Never auto-launches `mdoc`, never reads Keychain headless.

---

### 3.4 Milestone 3 — Cross-Device P2P Vault Transfer (`iroh`)

#### Why `iroh` over `tailcat`
- `tailcat` (`tailscale/tailcat`) is written in **Go**, which would add Go + `gomobile`/cgo toolchains to macOS, Windows, iOS, Android, and CI.
- `iroh` (`iroh` + `iroh-blobs`) is **pure Rust**, compiles directly into `crates/mdoc-vault` / `crates/mdoc-ffi` via UniFFI, and delivers the exact Tailcat UX: zero accounts, direct LAN/UDP hole-punching first, relay fallback when behind strict NAT.

#### Security & Wire Flow
1. **Sender initiates** "Send Project / Document to Device":
   - Send sheet shows item summary, total size, **"Include original files"** toggle (ON by default), and a **"Local network only"** indicator if enabled in Settings.
   - Generates a one-time ephemeral 256-bit `transfer_secret`, packages a chunked bundle manifest, starts an ephemeral `iroh` endpoint, and displays a **QR code + short copyable ticket** (`mdoc-xfer://v1/...`) containing `{node_addr, transfer_secret, manifest_hash}`.
   - Session auto-expires immediately after **1 completed transfer, cancel, or 5 minutes**.
2. **Receiver scans QR / pastes code**:
   - Connects over `iroh` QUIC (mutual TLS) and verifies the `transfer_secret` + `manifest_hash`.
   - Application-level chunked `AES-256-GCM` encryption using `transfer_secret` ensures any relay only sees double-encrypted ciphertext.
   - Receiver displays a **confirmation preview**: Project/Document name, counts (`3 documents · 2 replies`), total size (`14.2 MB`), and if `ProjectId` already exists locally: **"Merge into existing Project"** vs **"Import as new copy"**.
3. **Non-destructive Merge Semantics (on existing `ProjectId`)**:
   - **Identities:** Union of identities. Matching originals keep the local alias (rewriting incoming doc/reply aliases if needed); colliding aliases on *different* originals allocate a fresh never-reused alias on the incoming side and rewrite incoming text accordingly; `not_a_name` tombstones preserved.
   - **Documents:** New `DocumentId`s are added. Same `DocumentId` with identical hash is skipped (no-op). Same `DocumentId` edited on both devices imports as a **conflict copy** (`"<Title> (from <Device>)"`, new `DocumentId`) — never silently overwriting local edits.
   - **Replies:** Union by `ReplyId` (replies are immutable).

---

## 4. Suggested Implementation Slice Order

- **Milestone 1 (Vault + UI/UX Redesign):**
  1. `crates/mdoc-vault` core (crypto, manifest, projects, docs, replies, originals chunking, locking, alias reconciliation, unit + concurrency + stress tests).
  2. `mdoc-ffi` migration onto `mdoc-vault` (retiring `library/` file store, clean `Project` naming, updated UniFFI snapshots).
  3. Desktop GPUI redesign (Case→Project rename, persistent Vault sidebar tree, horizontal tab bar, 3-segment Project Overview tab, Reply tab, autosave, opaque `session.json`, streamlined Home, Shelf on `ready_snapshot`).
  4. iOS redesign (Case→Project rename, Vault sidebar drawer, true 3-segment Project page + Delete, Unfiled intake default, `RestoreExtension` on `mdoc-vault`, device check).
  5. Android parity on shared `mdoc-ffi` Vault API.
- **Milestone 2 (Desktop MCP Connector):**
  1. Authenticated local IPC server in desktop `mdoc` + `allow_ai` / `ai_excluded` UI controls + live activity indicator + focus-friendly `deliver_reply` banner/notification.
  2. `mdoc-mcp` binary (5-tool handler, `stdio` + optional `127.0.0.1` HTTP/SSE; evaluate reusing `storytold/craft-mcp-server`).
  3. Claude Desktop `.mcpb` packaging + ChatGPT Desktop connector validation & end-to-end leak-proof tests.
- **Milestone 3 (Cross-Device `iroh` Transfer):**
  1. `crates/mdoc-transfer` (or feature in `mdoc-vault`) on pure-Rust `iroh` (ephemeral ticket, double-encrypted chunked bundle, originals toggle, 5-min expiry, non-destructive merge + unit tests).
  2. Desktop + Mobile Send/Receive QR & short-code UI, Settings `"Local network only"` toggle, `docs/privacy.md` update, real iPhone ↔ Mac LAN + cellular acceptance tests.

---

## 5. Questions for a Fresh Reviewer Agent to Stress-Test

When reviewing this blueprint in a fresh session, please critically examine and challenge:
1. **Crypto nonce construction for `original.enc`:** Is `12B random base nonce XOR chunk_index (u32)` strictly safer than `8B random prefix || chunk_index (u32)` under a single 32-byte Vault key across thousands of originals? Also check chunk size (1 MiB vs 64 KiB) for iOS `RestoreExtension` / PDFKit random-access page reading!
2. **Manifest write amplification & iOS App Group concurrency:** If autosave fires frequently on desktop/mobile, should `manifest.enc` only rewrite when summary fields (title, phase, counts, `updated_at`) actually change, or debounce manifest commits separately from `doc.enc`?
3. **UUIDv4 vs UUIDv7** for `ProjectId` / `DocumentId` / `ReplyId` (is leaking creation timestamp in filenames on disk acceptable under our threat model, or should filenames stay pure random UUIDv4?).
4. **Desktop horizontal tab bar vs macOS traffic lights / GPUI window chrome:** How cleanly does the tab bar sit alongside the existing document header (`breadcrumb`, `Source`, `Panel`, `⋯`, `Pseudonymize`) without vertical chrome bloat on 13" MacBook Air screens?
5. **ChatGPT Desktop MCP reality check:** Verify at implementation time whether ChatGPT Desktop's current macOS/Windows build supports local `stdio` MCP without a remote HTTPS tunnel, or if `127.0.0.1` HTTP/SSE (or Apps SDK) has specific constraints we should account for early.
6. **`iroh` binary size & license audit (`deny.toml`):** Check `iroh` + `iroh-blobs` dependency tree against `deny.toml`'s GPL-3-compatible license allow-list and iOS/Android binary size budget before locking the crate version.
7. **Alias collision edge cases in M3 merge:** Walk through merging two devices that independently created `PERSON_1` for *different* people in the same `ProjectId` (e.g. both devices edited offline after an earlier transfer) — ensure alias rewriting in Markdown + `review_state` + `AiReply` is 100% lossless and never corrupts substring aliases (`PERSON_1` vs `PERSON_10`).
