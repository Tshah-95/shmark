# shmark

Peer-to-peer markdown sharing. Hit a hotkey with a markdown path or URL on your
clipboard and shmark shares it — beautifully rendered — with the people you
choose. Local-first, end-to-end encrypted, open source.

> **Status:** pre-alpha. Steps 1–4 of the [build order](./SPEC.md#15-build-order)
> work — stable identity, CLI control plane, peer-to-peer file sharing, a
> Tauri desktop app with a markdown-rendering UI (shiki + mermaid), and a
> global hotkey that opens a share-from-clipboard modal. No notifications,
> no multi-device pairing, no routing notes yet. See
> [`SPEC.md`](./SPEC.md) for what's next.

## Try it

Requires Rust 1.95+ (`.mise.toml` pins this) and bun for the frontend. On
macOS or Linux:

### CLI only

```bash
cargo build

# device A
./target/debug/shmark daemon start
./target/debug/shmark groups new my-group
./target/debug/shmark groups share-code my-group     # copy the "code" field
./target/debug/shmark share path/to/file.md --to my-group

# device B
./target/debug/shmark daemon start
./target/debug/shmark groups join '<code>' --alias from-A
./target/debug/shmark shares list                    # see device A's share
./target/debug/shmark shares download <share_id> --group from-A --dest ./out
```

### Desktop app

```bash
# stop any standalone daemon — the desktop app embeds its own
./target/debug/shmark daemon stop

# install frontend deps once
bun --cwd frontend install

# in one terminal: vite dev server
bun --cwd frontend run dev

# in another: launch the Tauri app (it embeds the daemon + serves the
# unix socket, so the CLI keeps working against the same process)
cargo run -p shmark-tauri
```

The Tauri app and the standalone CLI daemon both speak the same JSON RPC,
so `shmark groups list` etc. will hit whichever is currently the socket
owner. They cannot run at the same time.

Identity, device key, group state, blobs, and doc replicas all persist in
`~/Library/Application Support/shmark/` (macOS) or `~/.local/share/shmark/`
(Linux).

## What's working

- **Stable identity per human** — Ed25519 identity keypair, separate from each
  device's network key. Devices carry signed certs linking node → identity.
- **Daemon + thin clients** — Tauri-app-as-daemon (the desktop binary embeds
  the daemon) or standalone CLI daemon. Both expose the same JSON RPC over
  `~/Library/Application Support/shmark/shmark.sock`. CLI, frontend, and
  future agents all hit one dispatch function.
- **Groups** — DM-style containers for shares. Each is an `iroh-docs` document
  replicated peer-to-peer.
- **Shares** — files added to `iroh-blobs`, metadata written into the group's
  doc as JSON. Immutable (edit = new share). Recipients download a copy and
  own it.
- **Cross-device sync** — verified between Mac (NYC) and a box in HEL1-DC4
  (Helsinki); SHA-256 byte-identical in both directions.
- **Live sync resume** — daemons reconnect to known group peers on restart.
- **Desktop UI** — React 19 + Tailwind v4. Sidebar of groups, share list,
  rendered preview. Markdown renders via `react-markdown` + `remark-gfm` +
  `shiki` (code highlighting) + `mermaid` (diagrams). Code files (`.ts`,
  `.py`, etc.), JSON, YAML, CSV, and images all preview natively. Raw HTML
  in markdown is disabled.
- **Global hotkey** — `Cmd+Shift+P` (rebindable later). Reads the clipboard,
  detects a markdown file path, and opens a "share to" modal with a group
  picker. Backed by `tauri-plugin-global-shortcut`; first launch on macOS
  prompts for accessibility permission.

## What's not built yet

- Tauri app, markdown rendering, notifications.
- Global hotkey + clipboard intake.
- Multi-device pairing for one identity (so two of your laptops show as one
  "you" in shared groups).
- Routing notes / `context dump` for agents.
- Folder shares (single-file only for now).
- Pretty short share codes (current codes are raw `iroh-docs` `DocTicket`
  base32 strings, ~400 chars; the "4-word code" UX needs a rendezvous service
  and is deferred to v1).

## Why

Sharing markdown with coworkers today loses the rendering (Slack), fights an
editor (Notion), or opens in TextEdit (email). shmark makes the share itself a
single keystroke and the read experience first-class, without a server in the
middle holding your files.

## Design at a glance

- **Identity is a keypair.** No accounts, no email signup.
- **Groups are DM-style** — share with one person or a small named group.
- **Shares are immutable.** Once shared, a file is read-only; recipients can
  download a copy and do whatever with their copy.
- **No central server.** Content lives on devices, transferred peer-to-peer
  via [Iroh](https://www.iroh.computer). The only infrastructure is Iroh's
  public relay network, which is an encrypted pipe — it never sees plaintext.
- **One daemon, three clients.** A Rust daemon owns the network state; the
  Tauri app, the CLI, and any agent talk to it over the same local API.

Read [`SPEC.md`](./SPEC.md) for the full architecture, data model, and build
order.

## License

MIT — see [`LICENSE`](./LICENSE).
