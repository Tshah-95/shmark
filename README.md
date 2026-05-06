# shmark

Peer-to-peer markdown sharing. Hit a hotkey with a markdown path or URL on your
clipboard and shmark shares it — beautifully rendered — with the people you
choose. Local-first, end-to-end encrypted, open source.

> **Status:** pre-alpha. Step 1 of the [build order](./SPEC.md#15-build-order)
> works — the daemon boots a stable identity, the CLI talks to it over a Unix
> socket. No groups, no shares, no UI yet. See [`SPEC.md`](./SPEC.md) for the
> full plan.

## Try it

Requires Rust 1.95+ (the repo's `.mise.toml` pins this). On macOS:

```bash
cargo build
./target/debug/shmark daemon start
./target/debug/shmark identity show
./target/debug/shmark daemon stop
```

State lives in `~/Library/Application Support/shmark/`. The identity keypair
persists, so every restart shows the same `identity_pubkey`.

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
