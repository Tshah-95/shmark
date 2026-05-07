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

type Contact = {
  identity_pubkey: string;
  display_name: string;
  note: string | null;
  added_at: number;
};

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [bundle, setBundle] = useState<SettingsBundle | null>(null);
  const [hotkey, setHotkey] = useState("");
  const [autoPin, setAutoPin] = useState(true);
  const [roots, setRoots] = useState<string[]>([]);
  const [newRoot, setNewRoot] = useState("");
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedNotice, setSavedNotice] = useState(false);

  async function loadContacts() {
    try {
      const cs = await rpc<Contact[]>("contacts_list");
      setContacts(Array.isArray(cs) ? cs : []);
    } catch {
      // ignore
    }
  }

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [b] = await Promise.all([
          rpc<SettingsBundle>("settings_get"),
          loadContacts(),
        ]);
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

            <Section
              title="Contacts"
              hint="Free-text notes per identity, used by the agent when deciding where to share. Local-only — never sent to peers."
            >
              <ContactsEditor
                contacts={contacts}
                onChange={() => void loadContacts()}
              />
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

function ContactsEditor({
  contacts,
  onChange,
}: {
  contacts: Contact[];
  onChange: () => void;
}) {
  const [showAdd, setShowAdd] = useState(false);
  const [draftPubkey, setDraftPubkey] = useState("");
  const [draftName, setDraftName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [editingNote, setEditingNote] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function add() {
    setError(null);
    try {
      await rpc("contacts_upsert", {
        identity_pubkey: draftPubkey.trim(),
        display_name: draftName.trim(),
      });
      setDraftPubkey("");
      setDraftName("");
      setShowAdd(false);
      onChange();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function remove(c: Contact) {
    try {
      await rpc("contacts_remove", { name_or_pubkey: c.identity_pubkey });
      onChange();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function saveNote(c: Contact) {
    try {
      await rpc("contacts_set_note", {
        name_or_pubkey: c.identity_pubkey,
        note: editingNote.trim().length === 0 ? null : editingNote.trim(),
      });
      setEditing(null);
      onChange();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <div className="space-y-2">
      {contacts.length === 0 && (
        <div className="text-xs text-zinc-500 italic">no contacts yet</div>
      )}
      <ul className="space-y-1.5">
        {contacts.map((c) => (
          <li
            key={c.identity_pubkey}
            className="rounded border border-zinc-800 bg-zinc-900 px-3 py-2"
          >
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-sm font-medium truncate">
                  {c.display_name}
                </div>
                <div className="text-[10px] text-zinc-500 font-mono truncate">
                  {c.identity_pubkey}
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  onClick={() => {
                    setEditing(c.identity_pubkey);
                    setEditingNote(c.note ?? "");
                  }}
                  className="text-xs text-zinc-400 hover:text-zinc-100"
                >
                  edit note
                </button>
                <button
                  type="button"
                  onClick={() => void remove(c)}
                  className="text-xs text-zinc-500 hover:text-red-300"
                >
                  remove
                </button>
              </div>
            </div>
            {editing === c.identity_pubkey ? (
              <div className="mt-2 flex gap-2">
                <input
                  autoFocus
                  value={editingNote}
                  onChange={(e) => setEditingNote(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void saveNote(c);
                    if (e.key === "Escape") setEditing(null);
                  }}
                  placeholder="routing note (e.g. 'east coast, prefers summaries')"
                  className="flex-1 rounded bg-zinc-950 border border-zinc-700 px-2 py-1 text-xs focus:outline-none focus:border-zinc-500"
                />
                <button
                  onClick={() => void saveNote(c)}
                  className="text-xs text-zinc-100"
                >
                  save
                </button>
                <button
                  onClick={() => setEditing(null)}
                  className="text-xs text-zinc-500"
                >
                  cancel
                </button>
              </div>
            ) : c.note ? (
              <div className="mt-1.5 text-xs text-zinc-300">{c.note}</div>
            ) : (
              <div className="mt-1.5 text-xs text-zinc-600 italic">
                no note
              </div>
            )}
          </li>
        ))}
      </ul>
      {showAdd ? (
        <div className="rounded border border-zinc-800 bg-zinc-900 p-3 space-y-2">
          <input
            autoFocus
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            placeholder="display name (what you call them)"
            className="w-full rounded bg-zinc-950 border border-zinc-700 px-2.5 py-1.5 text-sm focus:outline-none focus:border-zinc-500"
          />
          <input
            value={draftPubkey}
            onChange={(e) => setDraftPubkey(e.target.value)}
            placeholder="identity_pubkey (64-char hex)"
            className="w-full rounded bg-zinc-950 border border-zinc-700 px-2.5 py-1.5 text-xs font-mono focus:outline-none focus:border-zinc-500"
          />
          {error && <div className="text-xs text-red-300">{error}</div>}
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setShowAdd(false)}
              className="text-xs text-zinc-400"
            >
              cancel
            </button>
            <button
              type="button"
              onClick={() => void add()}
              disabled={!draftName.trim() || !draftPubkey.trim()}
              className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-2.5 py-1 text-xs font-medium disabled:opacity-50"
            >
              Add
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setShowAdd(true)}
          className="rounded border border-zinc-800 hover:bg-zinc-800/60 px-3 py-1.5 text-xs text-zinc-300 w-full"
          data-testid="settings-add-contact"
        >
          + Add contact
        </button>
      )}
    </div>
  );
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
