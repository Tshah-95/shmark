import { useEffect, useRef, useState } from "react";
import { rpc } from "./api";

type Settings = {
  hotkey: string;
  search_roots: string[];
  auto_pin: boolean;
};

type SettingsBundle = {
  settings: Settings;
  effective_search_roots: string[];
  default_roots: string[];
};

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [bundle, setBundle] = useState<SettingsBundle | null>(null);
  const [hotkey, setHotkey] = useState("");
  const [autoPin, setAutoPin] = useState(true);
  const [roots, setRoots] = useState<string[]>([]);
  const [newRoot, setNewRoot] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedNotice, setSavedNotice] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const b = await rpc<SettingsBundle>("settings_get");
        if (cancelled) return;
        setBundle(b);
        setHotkey(b.settings.hotkey);
        setAutoPin(b.settings.auto_pin);
        setRoots(b.settings.search_roots);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function save() {
    setBusy(true);
    setError(null);
    setSavedNotice(false);
    try {
      const next = await rpc<Settings>("settings_set", {
        hotkey,
        search_roots: roots,
        auto_pin: autoPin,
      });
      // Refresh effective roots
      const b = await rpc<SettingsBundle>("settings_get");
      setBundle(b);
      setRoots(next.search_roots);
      setSavedNotice(true);
      window.setTimeout(() => setSavedNotice(false), 2000);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4"
      onClick={onClose}
      data-shmark-modal="settings"
    >
      <div
        className="w-full max-w-2xl rounded-lg border border-zinc-800 bg-zinc-950 p-5 shadow-xl max-h-[90vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-medium">Settings</h2>
          <button
            onClick={onClose}
            className="text-zinc-500 hover:text-zinc-200"
          >
            ✕
          </button>
        </div>

        {!bundle && <div className="text-sm text-zinc-400">loading…</div>}

        {bundle && (
          <div className="space-y-6">
            <Section
              title="Share hotkey"
              hint="Pressing this combo from anywhere opens the share-from-clipboard modal. Leave blank to disable."
            >
              <HotkeyRecorder value={hotkey} onChange={setHotkey} />
            </Section>

            <Section
              title="Project search roots"
              hint="Where shmark looks when you copy a relative path or a basename. Empty means use the built-in defaults."
            >
              <RootsEditor
                roots={roots}
                onChange={setRoots}
                newRoot={newRoot}
                onNewRootChange={setNewRoot}
              />
              {roots.length === 0 && (
                <div className="mt-2 text-xs text-zinc-500">
                  <div className="font-medium mb-0.5">
                    Defaults currently in effect:
                  </div>
                  <ul className="list-disc list-inside text-zinc-500 font-mono space-y-0.5">
                    {bundle.default_roots.length === 0 && (
                      <li>(no defaults exist on this machine)</li>
                    )}
                    {bundle.default_roots.map((r) => (
                      <li key={r}>{r}</li>
                    ))}
                  </ul>
                </div>
              )}
            </Section>

            <Section
              title="Auto-pin received shares"
              hint="When a share arrives from a peer, fetch its bytes immediately so they're available offline. Required for the share to render the first time you open it."
            >
              <label className="inline-flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={autoPin}
                  onChange={(e) => setAutoPin(e.target.checked)}
                  className="accent-zinc-200"
                />
                <span className="text-sm">
                  Auto-fetch incoming shares (recommended)
                </span>
              </label>
            </Section>

            {error && <div className="text-xs text-red-300">{error}</div>}

            <div className="flex justify-end gap-2 pt-2">
              {savedNotice && (
                <span className="text-xs text-emerald-400 self-center">
                  saved
                </span>
              )}
              <button
                onClick={onClose}
                className="rounded px-3 py-1.5 text-sm text-zinc-300 hover:text-zinc-100"
              >
                Cancel
              </button>
              <button
                onClick={() => void save()}
                disabled={busy}
                className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 text-sm font-medium disabled:opacity-50"
              >
                {busy ? "saving…" : "Save"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="text-sm font-medium">{title}</div>
      <div className="text-xs text-zinc-500 mt-0.5 mb-2">{hint}</div>
      {children}
    </div>
  );
}

function HotkeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  function start() {
    setRecording(true);
    setTimeout(() => inputRef.current?.focus(), 0);
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    // Ignore standalone modifier keys — wait for a real key.
    const standalone = ["Shift", "Control", "Alt", "Meta", "Hyper"].includes(
      e.key,
    );
    if (standalone) return;

    const parts: string[] = [];
    // Tauri's accelerator parser: CmdOrCtrl maps to ⌘ on macOS, Ctrl on others.
    if (e.metaKey) parts.push("CmdOrCtrl");
    else if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");

    const code = normalizeKey(e.key, e.code);
    if (!code) return;
    parts.push(code);

    onChange(parts.join("+"));
    setRecording(false);
  }

  return (
    <div className="flex items-center gap-2">
      <code className="font-mono text-sm bg-zinc-900 border border-zinc-800 rounded px-2 py-1 min-w-[200px]">
        {recording ? (
          <span className="text-zinc-500 italic">
            press a key combo (Esc to cancel)
          </span>
        ) : (
          value || <span className="text-zinc-500 italic">(none)</span>
        )}
      </code>
      <button
        type="button"
        onClick={start}
        className="rounded border border-zinc-700 hover:bg-zinc-800 px-2.5 py-1 text-xs"
      >
        {recording ? "recording…" : "Change"}
      </button>
      {value && (
        <button
          type="button"
          onClick={() => onChange("")}
          className="rounded border border-zinc-800 hover:border-zinc-700 px-2.5 py-1 text-xs text-zinc-400"
        >
          Clear
        </button>
      )}
      {recording && (
        <input
          ref={inputRef}
          onKeyDown={handleKeyDown}
          onBlur={() => setRecording(false)}
          className="absolute opacity-0 pointer-events-none w-0 h-0"
        />
      )}
    </div>
  );
}

function normalizeKey(key: string, code: string): string | null {
  // Letters and digits → Tauri uses Code-style names like KeyP / Digit5
  if (/^[a-zA-Z]$/.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (/^F\d+$/.test(key)) return key; // F1..F12
  // Special keys — accept a short list
  const special: Record<string, string> = {
    Escape: "Escape",
    Enter: "Enter",
    Tab: "Tab",
    Space: "Space",
    Backspace: "Backspace",
    ArrowUp: "ArrowUp",
    ArrowDown: "ArrowDown",
    ArrowLeft: "ArrowLeft",
    ArrowRight: "ArrowRight",
  };
  if (special[key]) return special[key];
  // Fall back to event.code (e.g. Comma, Period, Slash)
  if (code) return code;
  return null;
}

function RootsEditor({
  roots,
  onChange,
  newRoot,
  onNewRootChange,
}: {
  roots: string[];
  onChange: (next: string[]) => void;
  newRoot: string;
  onNewRootChange: (v: string) => void;
}) {
  function add() {
    const trimmed = newRoot.trim();
    if (!trimmed) return;
    if (roots.includes(trimmed)) {
      onNewRootChange("");
      return;
    }
    onChange([...roots, trimmed]);
    onNewRootChange("");
  }

  function remove(r: string) {
    onChange(roots.filter((x) => x !== r));
  }

  return (
    <div className="space-y-2">
      {roots.length > 0 && (
        <ul className="space-y-1">
          {roots.map((r) => (
            <li
              key={r}
              className="flex items-center gap-2 text-sm font-mono bg-zinc-900 border border-zinc-800 rounded px-2.5 py-1.5"
            >
              <span className="flex-1 truncate">{r}</span>
              <button
                type="button"
                onClick={() => remove(r)}
                className="text-zinc-500 hover:text-zinc-200 text-xs"
              >
                remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex gap-2">
        <input
          value={newRoot}
          onChange={(e) => onNewRootChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
          placeholder="/Users/you/path-to-project-root"
          className="flex-1 rounded bg-zinc-900 border border-zinc-700 px-2.5 py-1.5 text-sm font-mono focus:outline-none focus:border-zinc-500"
        />
        <button
          type="button"
          onClick={add}
          disabled={!newRoot.trim()}
          className="rounded border border-zinc-700 hover:bg-zinc-800 px-3 py-1.5 text-sm disabled:opacity-50"
        >
          Add
        </button>
      </div>
    </div>
  );
}
