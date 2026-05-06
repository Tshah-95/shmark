# shmark — Spec

> Status: living spec. Source of truth for what shmark is and how it's built.
> Last revised: 2026-05-06.

## 1. What shmark is

shmark is a peer-to-peer markdown sharing tool. You hit a global hotkey with a
markdown file path or URL on your clipboard and shmark shares it — beautifully
rendered — with a person or group you've chosen. Recipients get a desktop
notification, open the file, and can download their own copy to do whatever
with.

It exists because sharing markdown with coworkers today is miserable: paste
into Slack and lose the rendering, paste into Notion and fight the editor,
email a `.md` and they double-click into TextEdit. shmark makes the share itself
the smallest possible action and the read experience first-class.

It is local-first, end-to-end encrypted, and open source. There is no shmark
server holding your files. The only infrastructure in the loop is Iroh's
public relay network, which is a dumb encrypted pipe that helps two peers find
each other through NAT — it never sees plaintext.

## 2. Non-negotiables

These are the load-bearing decisions. Everything else is iteration.

- **Local-first.** Your markdown lives on your devices. There is no central
  store of shmark content.
- **End-to-end encrypted.** Anyone in the middle of the wire — including any
  relay — cannot read shares.
- **Open source.** MIT-licensed. Anyone can run it, audit it, fork it.
- **No accounts.** Identity is a keypair. No email signup, no password.
- **Immutable history.** Once shared, an item is read-only. Recipients can
  download a copy and modify their copy; the original entry is permanent.
- **Mac first.** v0 is macOS only. Linux and Windows are not blockers but are
  not v0.
- **Agent ergonomic.** Anything you can do in the UI you can do via CLI.
  Anything you can do in the CLI you can do via the same local API. Agents
  use that surface.

## 3. Glossary

- **Identity** — a person. One Ed25519 keypair, generated locally on first
  launch. The pubkey *is* you. Stored in the OS keychain (or a 0600 file in
  the app data dir for v0; keychain migration is a v1 line item).
- **Device** — a single machine running shmark, with its own Iroh node
  keypair. A device cert links the device's node pubkey to an identity, signed
  by the identity key. One identity can own many devices.
- **Contact** — someone else's identity that you've added to your local
  address book.
- **Group** — a named container shared between identities. Implemented as an
  `iroh-docs` document. DM-style, not channel-style. "Just me" is a built-in
  group containing only your own devices.
- **Share** — an entry in a group containing 1+ items, with a name and
  optional description. Immutable.
- **Item** — one file (or one file inside a folder share), stored as an
  `iroh-blobs` content-addressed blob plus a relative path.
- **Routing note** — free-text note attached to a group or contact, stored
  locally only. Read by agents to decide where things should go.

## 4. Architecture

### 4.1 Identity model

```
identity_keypair         (Ed25519, generated once per human)
  └─ identity_pubkey     ← the stable identity_id
  └─ display_name        (free text, can change, peers see latest)
  └─ devices[]           (device certs signed by identity_keypair)
       └─ device cert    { node_pubkey, identity_pubkey, signature, created_at }
```

A new device starts unpaired — it has its own Iroh node keypair but no
identity. Pairing flow: an existing device prints a 4-word code containing
its node addresses + a one-shot pairing token. The new device dials over Iroh,
proves possession of the token, and the existing device signs a device cert
for the new device's node pubkey + sends back the identity keypair. After
pairing, both devices are equal — there is no "primary device".

When a device authors anything — a share, a group entry — it stamps the
record with `(node_pubkey, identity_pubkey, signature)`. Anyone in the group
can verify the chain: signature over content by node_pubkey, device cert
linking node_pubkey to identity_pubkey signed by identity_pubkey. This is
why two friends see all of "Tejas's three laptops" as a single Tejas.

### 4.2 Daemon model

The Tauri app is the daemon. Quit the app and the daemon goes with it.
Lives as a menu bar / login item by default so it stays running while the
main window is closed.

```
┌──────────────────┐    ┌──────────────────┐    ┌───────────────┐
│  Tauri frontend  │    │   shmark-cli     │    │     agent     │
└────────┬─────────┘    └────────┬─────────┘    └───────┬───────┘
         │                       │                      │
         └───────────────────────┴──────────────────────┘
                              │
                  Unix socket (and loopback HTTP for non-Rust clients)
                              │
                  ┌───────────────────────┐
                  │      shmark-api       │  ← HTTP server, axum
                  └───────────┬───────────┘
                              │
                  ┌───────────────────────┐
                  │     shmark-core       │  ← Iroh endpoint, docs, blobs,
                  │                       │    identity, groups, shares
                  └───────────────────────┘
```

There is exactly one process that owns the Iroh data dir at a time. Three
clients (frontend, CLI, agents) talk to it over the same API. The Tauri app
embeds `shmark-api` directly; the CLI and agents go over the socket.

Closest precedents: Tailscale, Syncthing, Docker Desktop, the iroh CLI
itself.

#### Socket and auth

- **Primary**: Unix domain socket at `~/Library/Application Support/shmark/shmark.sock`.
  File permissions enforce auth — only the owning user can connect.
- **Secondary**: loopback HTTP at `127.0.0.1:<random>` with a token. The
  port + token live in `~/Library/Application Support/shmark/api.json` with
  0600 perms. For agents and any non-Rust client.

#### Auto-launch

If the CLI runs and the daemon isn't up:

1. Look for the socket file. If absent, fork-exec the Tauri app in headless
   mode (`shmark-tauri --headless`).
2. Wait up to ~3s for the socket to appear (poll).
3. Dispatch the command.

This matches the `code .` pattern. Users never have to think about whether
the daemon is running.

### 4.3 Network

Iroh handles the wire. Concretely:

- **Endpoint**: `iroh::Endpoint` per device. Uses QUIC over UDP. Hole-punches
  via STUN, falls back to n0's public relay network when direct fails.
- **Documents**: `iroh-docs` for groups. CRDT key-value store replicated
  among peers in the namespace.
- **Blobs**: `iroh-blobs` for file content. BLAKE3 content addressing. Fetched
  on demand from any peer known to have it.
- **Discovery**: peers in a group know each other's node addresses through
  the doc itself; new joiners get them from the bootstrap ticket.

We do not run any infrastructure. n0's relays are free, public, and
encrypted-pipe-only. v1 may add an "always-on personal cache" pattern — see
§9 — but that's a deployment choice the user makes, not infrastructure we
operate.

## 5. Data model

### 5.1 Synced (lives in iroh-docs and iroh-blobs)

**Group doc**

| Key | Value |
|-----|-------|
| `meta/name` | string (the canonical group name; per-user override is local) |
| `meta/created_by` | identity_pubkey of creator |
| `meta/created_at` | timestamp |
| `members/<identity_pubkey>` | `{ display_name, devices[], joined_at }` |
| `shares/<share_id>` | `{ name, description, items[], author_identity, author_node, signature, created_at }` |
| `tombstones/<member>` | leave events |

`share_id` = ULID. `items[]` = `[{ path: string \| null, blob_hash: BLAKE3 }]`.
Single-file shares: one item, `path: null`. Folder shares: one item per file
with relative paths.

Shares are append-only. Edits = a new share. Deletes = a tombstone keyed by
share_id; clients hide tombstoned shares but the audit trail remains.

### 5.2 Local only (lives in a sqlite file per device)

| Table | Purpose |
|-------|---------|
| `identity` | identity keypair, identity pubkey, display name |
| `devices` | device cert chain (this device + paired devices) |
| `contacts` | { identity_pubkey, display_name, notes, added_at } |
| `groups_local` | { group_id, local_alias, last_read_at } |
| `routing_notes` | { scope: 'group'\|'contact', target_id, body } |
| `shares_status` | per-share, per-recipient state cache (Sent/Synced/Downloaded) |
| `settings` | hotkey, auto-pin policy, notification prefs |

Routing notes are explicitly local-only. They are *your* notes about other
people, never published.

## 6. Surfaces

### 6.1 CLI (`shmark`)

```
shmark identity show
shmark identity rename <new-display-name>

shmark devices list
shmark devices pair                       # prints code
shmark devices pair <code>                # joins as new device
shmark devices remove <node-pubkey>       # local revoke (cosmetic in v0)

shmark contacts list
shmark contacts add <code>                # accept a contact-share code
shmark contacts share-code                # print my contact code
shmark contacts note <name> "..."         # set/replace routing note
shmark contacts note <name> --clear

shmark groups list
shmark groups new <name> [--members <name>,<name>...]
shmark groups join <code>
shmark groups share-code <name> [--read-only]
shmark groups rename <name> <new-local-alias>
shmark groups note <name> "..."
shmark groups leave <name>

shmark share <path> --to <group-or-contact> [--name "..."] [--description "..."]
shmark shares list [--group <g>] [--from <name>] [--unread]
shmark shares status <share-id>
shmark download <share-id> [<dest>]       # default: ./<share-name>/
shmark open <share-id>                    # open in app render view

shmark context dump                       # markdown blob of routing notes for agents

shmark daemon start
shmark daemon stop
shmark daemon status
shmark daemon logs [-f]
```

`<group-or-contact>` resolution: name lookup against local groups + contacts.
Ambiguous names return exit code 2 with the candidate list — agents and
humans both pick one and re-run with a more specific argument.

### 6.2 Local API

`shmark-api` exposes the same operations over HTTP. Routes mirror the CLI
verbs (`POST /shares`, `GET /shares`, `POST /groups`, ...). Full schema
generated from the same Rust types the CLI uses.

### 6.3 Tauri UI

Surfaces, in order of build:

1. Group list (sidebar). "Just me" pinned at top.
2. Share list per group, reverse-chronological.
3. Share detail: rendered preview + metadata + download button.
4. Settings: identity, devices, hotkey, auto-pin, notifications.
5. Onboarding: pair this device, or generate identity.

Render view is a webview iframe with raw HTML disabled and a strict CSP
(see §11).

## 7. Sharing flows

### 7.1 Hotkey share (the core flow)

1. User copies a markdown file path or URL.
2. Hits the configured hotkey (default `cmd+shift+P` — avoiding `cmd+P` to
   skip the system Print collision).
3. `shortcuts.rs` reads clipboard via `arboard`.
4. Resolver classifies the clipboard:
   - Local path → read bytes from disk.
   - `file://` → strip and read.
   - `http(s)://` ending in a markdown extension or with `Content-Type:
     text/markdown` → fetch.
   - Anything else → toast "shmark didn't recognize this", no-op.
5. UI prompt: "Share `foo.md` to..." with most-recent groups/contacts.
   User picks; hits enter.
6. API call: identical to `shmark share <path> --to <pick>`.
7. Toast confirms; share appears in the group locally; gossip propagates.

### 7.2 Manual share

`shmark share <path> --to <name>` from a terminal, or the in-app "share"
button on a group view.

### 7.3 Agent share

Agent (Claude, ChatGPT, whatever) hits the API. Recommended flow:

1. Agent reads `GET /context` to see routing notes.
2. Agent calls `POST /shares` with the path and target.
3. If 409 ambiguous, agent re-prompts the user with the candidate list.

The same routing-note surface that helps a human pick the right group helps
the agent pick the right group.

## 8. Sync semantics

### 8.1 What "share" means in CRDT terms

When you share, you append a share entry to the group's iroh-docs document
**locally**. There is no synchronous "send" RPC. The CRDT propagates to
peers as they become reachable, in any order, with no central sequencer.

This has consequences worth being explicit about:

- "Sending" always succeeds locally. There is no failure case at the API
  boundary for "the recipient is offline".
- The doc entry — name, hash, sender, timestamp — is tiny and propagates
  trivially as soon as one peer connects.
- The blob content is fetched on demand by recipients. If only the sender
  has the bytes and the sender is offline, the recipient cannot fetch yet.
  Once any peer has fetched, they're also a source.

### 8.2 Per-recipient state

For each share × recipient pair, the daemon tracks:

| State | Meaning |
|-------|---------|
| `Sent` | Doc entry created locally. |
| `Synced` | Recipient's node has the doc entry (gossip ack). |
| `Downloaded` | Recipient has fetched the blob (strongest "delivered"). |
| `Pending` | Recipient hasn't been online since you shared. |

Surfaced in `shmark shares status <id>` and in the UI as small per-recipient
indicators.

### 8.3 Auto-pin

Recipients can configure auto-pin: when a share arrives, the daemon fetches
the blobs immediately so they have a durable local copy without depending
on the sender being online later. **Default: ON for v0.** Users expect
"when I see it, it's mine" semantics from email and Slack. Power users who
want to save disk can turn it off.

### 8.4 Offline UX

If a recipient opens a share whose blobs aren't local and no peer is
reachable to fetch from: render shows
"`<sender>`'s device is offline. The file will download when they're back."
No spinner-of-death.

### 8.5 No retry logic at our layer

Iroh's gossip + on-demand blob fetch *is* the retry queue. We don't write
exponential backoff. We do surface state honestly.

### 8.6 Always-on personal cache (v1)

The multi-device identity model means a user can pair an extra "always-on"
device — a desktop that never sleeps, or a tiny VPS — that acts as a
personal cache. It's another paired device by the same identity, but it
happens to never go offline. This solves the "all my devices are sleeping
and my friend wants to read my share" problem without any central
infrastructure. Open source, user-deployed, optional.

## 9. Multi-device pairing flow

1. Existing device A: `shmark devices pair`.
   - Mints a one-shot pairing token.
   - Prints a 4-word code containing `(node_addr, pairing_token)`.
2. New device B: `shmark devices pair <code>`.
   - Decodes, dials A over Iroh.
   - Sends device B's `node_pubkey`, proves possession of the token.
3. Device A:
   - Verifies token.
   - Signs a device cert for B's node_pubkey.
   - Sends `(identity_keypair, identity_pubkey, full_device_cert_chain)` over
     the encrypted channel.
4. Device B:
   - Stores the identity keypair (in keychain, or 0600 file for v0).
   - Stores its device cert.
   - Pulls the existing groups + contacts from device A.
5. Both devices now have the same identity + the same view of groups.

After pairing, device A and device B publish updates to the device cert
chain into a private "self" doc that's shared only between an identity's
devices. This is how subsequent device pairings propagate.

## 10. Routing notes / agent integration

Two scopes:

- **Group note**: free text. "Engineering team — share infra docs here, no
  customer data. Garrett owns this group."
- **Contact note**: free text. "Garrett prefers high-level summaries over
  deep specs. He's east coast."

Both are local-only. Stored in the local sqlite, never published.

`GET /context` (or `shmark context dump`) returns a single markdown blob:

```markdown
# shmark context

## Groups
### Engineering team
Engineering team — share infra docs here, no customer data. Garrett owns this group.

## Contacts
### Garrett (gxxxxx...)
Garrett prefers high-level summaries over deep specs. He's east coast.
```

Agents prepend this to their system prompt before deciding routing.

## 11. Render

### 11.1 Formats

v0 ships with these. Where the engine is "shiki", we get it for ~free
because shiki is already wired up for markdown code blocks.

| Format | Renderer | v0? |
|--------|----------|-----|
| `.md` | react-markdown + remark-gfm + rehype-raw (raw HTML disabled) + shiki + mermaid | yes |
| `.txt` | `<pre>` monospace | yes |
| Code (`.ts`, `.py`, `.rs`, `.go`, `.java`, ~200 langs) | shiki | yes |
| `.json`, `.yaml`, `.toml` | shiki + collapse/expand | yes |
| `.csv` | papaparse → table | yes |
| Images (`.png`, `.jpg`, `.svg`, `.webp`, `.gif`) | webview native | yes |
| `.pdf` | PDF.js | v1 |
| `.docx` | mammoth.js | v1 |
| `.xlsx`, `.pptx`, native binary | — | not planned |

### 11.2 Sandboxing

Render runs in the Tauri webview. Policies:

- **Raw HTML in markdown is disabled.** `rehype-raw` is omitted; any `<...>`
  in markdown is rendered as text.
- **Strict CSP**: `default-src 'self'; script-src 'self'; style-src 'self'
  'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self';`. No
  external fetches from rendered content.
- **Mermaid scripts are bundled, not fetched.**
- v1 may revisit raw HTML inside an iframe sandbox; v0 keeps it simple.

## 12. Open questions / explicitly deferred

- **Windows / Linux ports.** Out of scope for v0.
- **RBAC.** No roles in v0. Group creator is stamped in metadata for
  display. Anyone with a write ticket can publish; anyone with a read
  ticket can read. People leave on their own.
- **Revocation.** Not supported. Once shared, content cannot be unshared
  from someone who already synced. Documented in onboarding.
- **`.pdf` / `.docx` previews.** v1.
- **Structured routing rules** (e.g., "never share `*.env` to <group>").
  v0 = free text. Revisit if free text proves insufficient.
- **Keychain storage for identity key.** v0 uses a 0600 file in app data
  dir. Migrate to OS keychain in v1.
- **Auto-pin default.** v0 = ON. Revisit after first dogfooding pass if
  disk usage becomes a complaint.
- **Edits to shares.** v0 = immutable, edit = new share. Revisit only if
  there's strong demand.

## 13. Tech stack

| Layer | Choice |
|-------|--------|
| Daemon language | Rust |
| P2P | iroh, iroh-docs, iroh-blobs |
| HTTP server | axum |
| CLI parser | clap |
| Local store | sqlite via sqlx |
| Clipboard | arboard |
| Global shortcuts | tauri-plugin-global-shortcut |
| Notifications | tauri-plugin-notification |
| Frontend | React 19 + Vite |
| Markdown render | react-markdown + remark-gfm |
| Syntax highlight | shiki |
| Diagrams | mermaid.js |
| Tabular | papaparse |
| Build | cargo workspace + tauri 2 |
| License | MIT |

## 14. Repo layout (planned)

```
shmark/
├── Cargo.toml                  # workspace
├── crates/
│   ├── shmark-core/            # Iroh node, identity, groups, shares, blobs
│   ├── shmark-api/             # axum server, Unix socket + loopback HTTP
│   ├── shmark-cli/             # clap CLI, thin client over shmark-api
│   └── shmark-tauri/           # Tauri app, embeds shmark-api
├── frontend/                   # React + Vite, Tauri's UI
├── docs/
│   └── architecture/           # deeper design docs as they emerge
├── SPEC.md                     # this doc
├── README.md
└── LICENSE
```

## 15. Build order

Each step ships something runnable. Steps 1–2 give us a working P2P spine
in CLI form before any UI exists, which is the cheapest way to find
protocol-level surprises.

1. **Foundations.** Cargo workspace, identity keypair + device cert,
   `shmark-api` over Unix socket, `shmark-cli` with `identity show` and
   `daemon start|stop|status`. Two laptops can run a daemon and prove
   stable identities.
2. **Groups + shares (CLI only).** `groups new`, `groups join <code>`,
   `share <path> --to <group>`, `shares list`, `download`. Two laptops
   share a markdown file end-to-end via 4-word code.
3. **Tauri shell + render.** Wrap the same API in a Tauri app. Group list,
   share list, render view (md + txt + shiki + mermaid + json/yaml/csv +
   images).
4. **Hotkey + clipboard.** Global shortcut → clipboard → API. Same call
   the CLI uses.
5. **Multi-device pairing.** `devices pair`, identity-key signing,
   "Just me" group, friends see your devices as one identity.
6. **Notifications + sync status.** Doc subscriptions fire native
   notifications on incoming shares. Per-recipient state surfaced.
7. **Routing notes + agent surface.** Group/contact notes, `context dump`,
   polished `--to` resolution.

Dogfooding: from step 2 onward, use shmark itself to share spec updates,
research notes, and follow-up specs between devices. If we can't bear to
use it, that's signal.
