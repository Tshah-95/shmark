# Agent-driven Tauri 2 testing — recommended stack

> Question: how does an AI agent (or any non-human harness) verify the
> shmark desktop app's UI flows end-to-end without opening a visible
> window on the user's screen? Compare the obvious options and recommend
> a stack we can build first.

## Key findings (read these first)

1. **`tauri-driver` is unsupported on macOS.** From the official Tauri 2
   docs: *"On desktop, only Windows and Linux are supported due to macOS
   not having a WKWebView driver tool available."*
   ([source](https://v2.tauri.app/develop/tests/webdriver/)). Since
   shmark targets macOS first, the canonical Tauri E2E story doesn't
   apply to us. **This is the surprise that reshapes the answer.**

2. **Tauri 2 ships an official IPC mocking layer for JS unit tests** —
   `mockIPC`, `mockWindows`, `clearMocks` from `@tauri-apps/api/mocks`,
   with `shouldMockEvents: true` available since v2.7.0
   ([source](https://v2.tauri.app/develop/tests/mocking/)). Works with
   Vitest + jsdom. **Replaces** the Rust runtime — does not test it.

3. **Rust ↔ frontend remote control is straightforward.** The Rust-side
   `Webview` exposes `eval(js)` and `eval_with_callback(js, cb)` (the
   first is fire-and-forget; the second returns a JSON string)
   ([docs.rs](https://docs.rs/tauri/2.11.0/tauri/webview/struct.Webview.html)).
   The frontend exposes `WebviewWindow.{listen, emit, show, hide,
   isVisible, setPosition}`
   ([source](https://v2.tauri.app/reference/javascript/api/namespacewebviewwindow/)).
   Together these primitives let a Rust-driven test harness inject any
   JS into the webview and observe state — no WebDriver needed.

4. **macOS OS-chrome automation (cliclick / AppleScript) needs an
   active GUI session and accessibility permission.** cliclick *"is not
   possible to use … before a user logs in"*
   ([source](https://github.com/BlueM/cliclick)) and AppleScript
   System Events keystrokes go to the frontmost process, with no
   reliable way to target a non-foreground app
   ([Apple Discussions](https://discussions.apple.com/thread/8581123),
   [Daniela Baron blog](https://danielabaron.me/blog/automate-keyboard-mac/)).
   Workable for one-machine smoke tests; flaky / unsuitable for CI or
   over SSH.

5. **shmark already has the right spine for this.** A unix-socket JSON
   RPC where every domain verb (groups, shares, settings, paths_resolve)
   is callable. Adding a few **dev-only** RPCs that wrap the Rust-side
   webview script-evaluation and window-state queries gives an
   in-process test driver that doesn't require an external WebDriver
   server.

## Current state (shmark, today)

Files referenced are at `/Users/tejas/Github/shmark/`.

- `crates/shmark-api/src/dispatch.rs` — central dispatch fn used by
  both the unix socket server and the in-process Tauri command. All
  domain operations live here; **no UI-specific RPCs yet.**
- `crates/shmark-api/src/server.rs` — unix-socket server, line-
  delimited JSON.
- `crates/shmark-tauri/src/main.rs` — embeds `AppState`, runs the unix
  socket, exposes a `rpc(method, params)` Tauri command, registers
  the global hotkey, builds the tray.
- `frontend/src/App.tsx` — listens for `shmark://hotkey/share` and
  shows the share-from-clipboard modal.
- `frontend/src/Settings.tsx` — `HotkeyRecorder` captures key combos
  via React `keydown`. **The recorder is purely DOM-driven** — perfect
  for component-level Vitest tests.
- `crates/shmark-core/src/resolve.rs` — already has 9 unit tests; **the
  in-codebase precedent for this kind of testing is good.**
- `frontend/src/render/{Markdown,CodeView,CsvTable,Mermaid}.tsx` —
  pure React components, no Tauri imports inside. Ideal for snapshot
  tests (no mocking needed at all for these).

What's NOT yet present:
- `tests/e2e/` directory or any frontend test harness.
- Vitest config in `frontend/`.
- `--headless` flag on `shmark-desktop`.
- Any Rust→JS script-injection plumbing.
- `dev_*` RPCs.

## Options compared

| # | Option | Setup cost | Coverage | User disruption (Mac dev box) | macOS support |
|---|--------|-----------|----------|------|-----|
| 1 | tauri-driver / WebDriver | medium | full E2E inc. real renders | low (hidden window) | **no — Linux/Win only** |
| 2 | Headless Tauri (visible:false) + RPC-driven dev commands | medium | UI flow + state, not pixels | none if window stays hidden | yes |
| 3 | Vitest + Testing Library + `mockIPC` (component tests) | low | component logic, no real Rust | none | yes |
| 4 | Snapshot/HTML output diff for shiki/mermaid renderers | very low | rendering correctness only | none | yes |
| 5 | AppleScript / cliclick for OS-chrome | low | OS-level events | flashes / activates app | yes (with caveats) |
| 6 | CLI parity ("everything in app is also in CLI") | low | domain logic only; cannot test UI | none | yes |

Crucially, option 1 is *off the table on Mac in 2026*. Option 2 is the
closest functional substitute we can run locally.

### Per-option detail

**(1) tauri-driver.** Out for our case. If we ever ship Linux/Windows
builds we could revisit; the toolchain is `cargo install tauri-driver
--locked` + WebDriverIO/Selenium harness against a binary spawned on a
specific port.

**(2) Headless Tauri + dev RPCs.** The pattern: `shmark-desktop
--headless` calls `webview.hide()` after `setup` and skips the tray
build. Adds a small set of RPCs only compiled in `cfg(debug_assertions)`:

- `dev_run(js: String)` — wraps `webview.eval(js)`. Lets the agent fire
  any JS inside the running webview
  (`document.querySelector(...).click()`,
  `(window as any).__SHMARK_TEST__.openModal('share')`, etc.).
- `dev_run_get(js: String)` — uses `eval_with_callback`, blocks on a
  oneshot, returns the result string.
- `dev_window_state()` — `{ visible, focused, label }`.
- `dev_active_modal()` — reads from a frontend-exposed atom.
- `dev_emit(name, payload)` — re-exports `app.emit` so a test can
  inject events the frontend listens for (e.g.
  `shmark://hotkey/share` to simulate the hotkey without going
  through the OS).

The frontend cooperates by exposing test hooks behind a `__SHMARK_TEST__`
global object (`__SHMARK_TEST__.openModal('share')`,
`__SHMARK_TEST__.getRenderedHtml()`). Gated by a `dev` flag so it's
absent in release builds.

This gives us **flow-level E2E without ever showing a window** and
**without clicking anything from outside the app**. The agent drives via
RPC; the app drives itself via the test hooks. No WebDriver server, no
new harness binary.

**(3) Vitest + Testing Library + `mockIPC`.** The right tool for
component-level logic: "given props X, the modal renders Y," "clicking
the submit button calls `invoke('share_create', ...)` with these
args." Doesn't exercise real Rust code. Fast.

```ts
// example — verify Settings hotkey recorder builds the right accelerator
import { mockIPC } from '@tauri-apps/api/mocks';
mockIPC((cmd, args) => {
  if (cmd === 'rpc' && args.method === 'settings_get') {
    return { settings: { hotkey: 'CmdOrCtrl+Shift+P', search_roots: [], auto_pin: true },
             effective_search_roots: [], default_roots: [] };
  }
});
```

**(4) Snapshot tests for renderers.** `Markdown.tsx`, `CodeView.tsx`,
and `CsvTable.tsx` are pure: text in, HTML out. Render once with a
fixed input, assert with `toMatchInlineSnapshot`. Catches shiki/mermaid
upgrades that change output. **Lowest-cost, highest-signal-per-line
test we could write.**

**(5) AppleScript / cliclick.** The only realistic way to verify the
*OS-level* experience: tray icon visible, global hotkey actually
firing through the OS. cliclick example:

```bash
osascript -e 'tell application "shmark" to activate'
cliclick kd:cmd,shift kp:p ku:cmd,shift
```

Limits: needs a logged-in GUI session, accessibility permission
granted to the parent process (Terminal / iTerm / VS Code), and the
keystroke goes to whichever app is foreground (so script must
explicitly activate the target app first). Acceptable for occasional
verification on the dev box; not useful in CI or over SSH.

**(6) CLI parity.** Already mostly in place. Insufficient on its own —
"the modal opens" is not a CLI verb — but **necessary** as the foundation
because all the dev RPCs in option 2 are just more CLI verbs.

## Recommended stack

In layers, cheapest first:

1. **CLI/RPC parity** — done. Keep adding domain verbs as we add
   features. Confirm anything UI-side that calls a backend can also be
   exercised via the socket directly.
2. **Snapshot tests for renderers** — `frontend/tests/render/*.test.tsx`
   using Vitest. Lock in shiki, mermaid, csv table output. Five
   minutes of setup, regression-proof forever.
3. **Component tests with `mockIPC`** — `frontend/tests/components/*`.
   Drive `ShareFromClipboard`, `Settings`, `App` with mocked invoke.
   Verifies the wiring without booting Tauri.
4. **Rust integration tests on `dispatch`** — `crates/shmark-api/tests/
   dispatch.rs`. Spin up a fresh `AppState` in a tempdir, hit each
   verb, assert on returned JSON. Already 9 passing tests for `resolve`;
   extend the same pattern.
5. **Headless Tauri + `dev_*` RPCs** — the meaningful new investment.
   See "Punch list" below. Gated behind `cfg(debug_assertions)` so
   release builds don't ship test backdoors.
6. **AppleScript smoke once before each release** — verify tray and
   global hotkey on a real desktop. Don't try to automate OS-chrome
   in CI.

We deliberately **skip tauri-driver** (off on Mac) and **skip a
separate test-driver binary** (the existing socket + `dev_*` RPCs is
the same idea with less code).

## Punch list — what to build first

In order. Each step ships a working slice.

1. **`frontend/tests/render/*.test.tsx`** — Vitest in `frontend/`.
   - Add `vitest` + `@testing-library/react` + `jsdom` to
     `frontend/package.json` devDeps.
   - `Markdown` with a fixture `.md` file → `toMatchInlineSnapshot`.
   - Same for `CodeView` (typescript + python), `CsvTable` (3-col
     fixture). No Tauri mocking needed; these components are pure.
2. **`crates/shmark-api/tests/dispatch_test.rs`** — Rust integration
   test for dispatch. Boots a fresh `AppState` against a tempdir
   data dir, runs through `groups_new` → `share_create` → `shares_list`
   → `share_get_bytes`, asserts shapes. Establishes pattern for the
   rest.
3. **`frontend/tests/components/Settings.test.tsx`** — `mockIPC` +
   RTL. Verifies `HotkeyRecorder` builds the right accelerator from a
   keydown, that `Save` invokes `rpc('settings_set', ...)` with the
   right payload.
4. **`--headless` flag on `shmark-desktop`** — in
   `crates/shmark-tauri/src/main.rs`. After `setup`, if the CLI args
   contain `--headless`, call `window.hide()` and skip `build_tray`.
   Reuses the existing AppState boot.
5. **`dev_*` RPCs** — added behind `cfg(debug_assertions)`. Initial set:
   `dev_window`, `dev_emit`, `dev_run`, `dev_run_get`. `dev_run` needs
   the `AppHandle` (not just `AppState`), so it lives as a Tauri
   command in shmark-tauri rather than in shmark-api dispatch.
6. **`__SHMARK_TEST__` window global** — frontend exposes a small
   surface: `openModal(name)`, `getRenderedHtml()`, `dispatch(action)`.
   Defined in `src/dev.ts`, imported only when
   `import.meta.env.DEV` so production bundles strip it.
7. **`tests/e2e/share-from-clipboard.test.ts`** — first end-to-end
   smoke. Spawns `shmark-desktop --headless` as a child process, waits
   for the socket, calls `dev_emit("shmark://hotkey/share")`, polls
   `dev_active_modal()` until it reads "share-from-clipboard", calls
   `dev_run` to click the submit button, verifies a new share
   appeared via `shares_list`.

Order rationale: 1–3 are pure JS/Rust unit tests with no orchestration —
they catch regressions cheaply. 4–7 build the headless E2E spine,
each step usable on its own.

## Unknowns

- **Webview script-eval reliability for arbitrary JS**: docs.rs notes
  *"Exception is ignored because of the limitation on Windows. You can
  catch it yourself and return as string as a workaround."* For our
  needs this is fine — wrap the JS in `try/catch` and bounce errors
  back via the callback variant. Confirmed working in production apps
  based on the workaround patterns in
  [tauri#190](https://github.com/tauri-apps/tauri/issues/190).
- **Hidden-window WebKitView render fidelity**: a hidden Tauri window
  on macOS still runs the JS event loop and dispatches events; whether
  layout/measurement APIs return real values when the window is
  unmapped is undocumented. May need to use `setPosition` to put the
  window off-screen instead of `hide()` if measurement is needed.
  Flag for the first headless test.
- **Global-hotkey re-registration**: `settings_set` fires
  `signal_settings_changed`; the watcher unregisters/registers via
  `tauri-plugin-global-shortcut`. Whether the OS observes the new
  binding without a user-side keypress confirmation is something only
  AppleScript-level testing or actual user keystroke can verify.
  Acceptable to leave OS-side as a manual check.
