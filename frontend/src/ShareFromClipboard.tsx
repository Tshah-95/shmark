import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useEffect, useState } from "react";
import { rpc, type LocalGroup, type ShareRecord } from "./api";

type Detected =
  | { kind: "path"; path: string }
  | { kind: "url"; url: string }
  | { kind: "unsupported"; raw: string }
  | { kind: "empty" };

type Props = {
  groups: LocalGroup[];
  onClose: () => void;
  onShared: (record: ShareRecord) => void;
};

export function ShareFromClipboard({ groups, onClose, onShared }: Props) {
  const [detected, setDetected] = useState<Detected | null>(null);
  const [target, setTarget] = useState<string | "">("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const raw = (await readText()).trim();
        if (cancelled) return;
        if (!raw) {
          setDetected({ kind: "empty" });
          return;
        }
        const d = classify(raw);
        setDetected(d);
        if (d.kind === "path") {
          const last = d.path.split("/").pop() ?? "";
          setName(last);
        }
      } catch (e) {
        if (!cancelled)
          setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!target && groups.length > 0) {
      setTarget(groups[0]!.local_alias);
    }
  }, [groups, target]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!detected || detected.kind !== "path" || !target) return;
    setBusy(true);
    setError(null);
    try {
      const record = await rpc<ShareRecord>("share_create", {
        group: target,
        path: detected.path,
        name: name.trim() || null,
        description: description.trim() || null,
      });
      onShared(record);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md rounded-lg border border-zinc-800 bg-zinc-950 p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-medium">Share from clipboard</h2>
          <button
            onClick={onClose}
            className="text-zinc-500 hover:text-zinc-200 text-sm"
          >
            ✕
          </button>
        </div>

        {detected === null && (
          <div className="text-sm text-zinc-400">reading clipboard…</div>
        )}

        {detected?.kind === "empty" && (
          <div className="text-sm text-zinc-400">
            Clipboard is empty. Copy a markdown file path, then press the
            hotkey again.
          </div>
        )}

        {detected?.kind === "unsupported" && (
          <div className="space-y-2">
            <div className="text-sm text-zinc-400">
              shmark didn't recognize this clipboard content as a markdown
              file.
            </div>
            <pre className="text-xs font-mono text-zinc-500 max-h-32 overflow-auto whitespace-pre-wrap p-2 rounded bg-zinc-900">
              {detected.raw.slice(0, 300)}
              {detected.raw.length > 300 ? "…" : ""}
            </pre>
            <div className="text-xs text-zinc-500">
              Supported (v0): a local file path ending in <code>.md</code>,{" "}
              <code>.txt</code>, or another previewable extension.
            </div>
          </div>
        )}

        {detected?.kind === "url" && (
          <div className="space-y-2">
            <div className="text-sm text-zinc-300">
              URL detected:
              <span className="font-mono text-xs block truncate text-zinc-400 mt-0.5">
                {detected.url}
              </span>
            </div>
            <div className="text-xs text-zinc-500">
              URL fetching isn't wired up yet (v1). For now, save the file
              locally and re-copy the path.
            </div>
          </div>
        )}

        {detected?.kind === "path" && (
          <form onSubmit={submit} className="space-y-3">
            <div className="text-xs text-zinc-500 font-mono break-all">
              {detected.path}
            </div>

            <label className="block">
              <span className="text-xs text-zinc-400">Share to</span>
              <select
                value={target}
                onChange={(e) => setTarget(e.target.value)}
                className="mt-1 w-full rounded bg-zinc-900 border border-zinc-700 px-3 py-2 text-sm focus:outline-none focus:border-zinc-500"
              >
                {groups.length === 0 ? (
                  <option value="">— no groups yet —</option>
                ) : (
                  groups.map((g) => (
                    <option key={g.namespace_id} value={g.local_alias}>
                      {g.local_alias}
                    </option>
                  ))
                )}
              </select>
            </label>

            <label className="block">
              <span className="text-xs text-zinc-400">Display name</span>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="mt-1 w-full rounded bg-zinc-900 border border-zinc-700 px-3 py-2 text-sm focus:outline-none focus:border-zinc-500"
              />
            </label>

            <label className="block">
              <span className="text-xs text-zinc-400">Description (optional)</span>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                className="mt-1 w-full rounded bg-zinc-900 border border-zinc-700 px-3 py-2 text-sm focus:outline-none focus:border-zinc-500"
              />
            </label>

            {error && <div className="text-xs text-red-300">{error}</div>}

            <div className="flex justify-end gap-2 pt-1">
              <button
                type="button"
                onClick={onClose}
                className="rounded px-3 py-1.5 text-sm text-zinc-300 hover:text-zinc-100"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={busy || groups.length === 0 || !target}
                className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 text-sm font-medium disabled:opacity-50"
              >
                {busy ? "sharing…" : "Share"}
              </button>
            </div>
          </form>
        )}

        {error && detected?.kind !== "path" && (
          <div className="mt-3 text-xs text-red-300">{error}</div>
        )}
      </div>
    </div>
  );
}

function classify(raw: string): Detected {
  // file:// URL
  if (raw.startsWith("file://")) {
    return { kind: "path", path: raw.replace(/^file:\/\//, "") };
  }
  // http(s) URL
  if (/^https?:\/\//.test(raw)) {
    return { kind: "url", url: raw };
  }
  // Absolute or home-relative local path
  if (raw.startsWith("/") || raw.startsWith("~")) {
    return { kind: "path", path: raw };
  }
  // Looks like a path with an extension we know about
  if (/[A-Za-z0-9_\-]\.[A-Za-z0-9]{1,8}$/.test(raw) && !raw.includes("\n")) {
    return { kind: "path", path: raw };
  }
  return { kind: "unsupported", raw };
}
