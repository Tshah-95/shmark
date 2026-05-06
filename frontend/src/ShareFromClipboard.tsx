import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useEffect, useState } from "react";
import { rpc, type LocalGroup, type ShareRecord } from "./api";
import { formatBytes, formatRelativeTime } from "./util";

type Candidate = {
  path: string;
  parent_dir: string;
  size_bytes: number;
  mtime_secs: number;
};

type Resolution =
  | { kind: "empty" }
  | { kind: "unsupported"; raw: string }
  | { kind: "url"; url: string }
  | { kind: "path"; candidate: Candidate }
  | { kind: "candidates"; candidates: Candidate[] }
  | { kind: "not_found"; query: string; roots: string[] };

type Props = {
  groups: LocalGroup[];
  onClose: () => void;
  onShared: (record: ShareRecord) => void;
};

export function ShareFromClipboard({ groups, onClose, onShared }: Props) {
  const [resolution, setResolution] = useState<Resolution | null>(null);
  const [chosen, setChosen] = useState<Candidate | null>(null);
  const [target, setTarget] = useState<string>("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const raw = (await readText()) ?? "";
        const r = await rpc<Resolution>("paths_resolve", { raw });
        if (cancelled) return;
        setResolution(r);
        if (r.kind === "path") {
          setChosen(r.candidate);
          setName(basename(r.candidate.path));
        } else if (r.kind === "url") {
          setName(urlBasename(r.url) ?? "shared.md");
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
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

  function pickCandidate(c: Candidate) {
    setChosen(c);
    setName(basename(c.path));
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!target || !resolution) return;
    setBusy(true);
    setError(null);
    try {
      const sourcePath =
        resolution.kind === "url"
          ? resolution.url
          : (chosen?.path ?? null);
      if (!sourcePath) throw new Error("nothing to share");
      const record = await rpc<ShareRecord>("share_create", {
        group: target,
        path: sourcePath,
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
        className="w-full max-w-lg rounded-lg border border-zinc-800 bg-zinc-950 p-5 shadow-xl"
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

        {resolution === null && (
          <div className="text-sm text-zinc-400 italic">resolving…</div>
        )}

        {resolution?.kind === "empty" && (
          <div className="text-sm text-zinc-400">
            Clipboard is empty. Copy a file path or URL, then press the
            hotkey again.
          </div>
        )}

        {resolution?.kind === "unsupported" && (
          <div className="space-y-2">
            <div className="text-sm text-zinc-400">
              shmark didn't recognize this clipboard content as a file path or
              URL.
            </div>
            <pre className="text-xs font-mono text-zinc-500 max-h-32 overflow-auto whitespace-pre-wrap p-2 rounded bg-zinc-900">
              {resolution.raw.slice(0, 300)}
              {resolution.raw.length > 300 ? "…" : ""}
            </pre>
          </div>
        )}

        {resolution?.kind === "not_found" && (
          <div className="space-y-2">
            <div className="text-sm text-zinc-300">
              Couldn't find a file matching{" "}
              <code className="px-1 rounded bg-zinc-900 text-xs">
                {resolution.query}
              </code>
              .
            </div>
            <div className="text-xs text-zinc-500">
              Searched: {resolution.roots.join(", ") || "(no roots)"}
            </div>
            <div className="text-xs text-zinc-500">
              Tip: copy the absolute path (e.g. with{" "}
              <code className="px-1 rounded bg-zinc-900">realpath</code>) or
              add the project's parent dir to the search roots in Settings.
            </div>
          </div>
        )}

        {resolution?.kind === "candidates" && !chosen && (
          <div className="space-y-2">
            <div className="text-xs text-zinc-400 uppercase tracking-wider">
              Multiple matches — pick one
            </div>
            <ul className="divide-y divide-zinc-900 rounded border border-zinc-800">
              {resolution.candidates.map((c) => (
                <li key={c.path}>
                  <button
                    onClick={() => pickCandidate(c)}
                    className="w-full text-left px-3 py-2 hover:bg-zinc-900/60"
                  >
                    <div className="text-sm font-medium">{basename(c.path)}</div>
                    <div className="text-xs text-zinc-500 font-mono truncate">
                      {c.parent_dir}
                    </div>
                    <div className="text-xs text-zinc-500 mt-0.5">
                      {formatBytes(c.size_bytes)} ·{" "}
                      {formatRelativeTime(c.mtime_secs)}
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}

        {(resolution?.kind === "path" ||
          (resolution?.kind === "candidates" && chosen) ||
          resolution?.kind === "url") && (
          <form onSubmit={submit} className="space-y-3">
            <div className="text-xs text-zinc-500 font-mono break-all">
              {resolution.kind === "url"
                ? resolution.url
                : (chosen?.path ?? "")}
            </div>
            {resolution.kind === "candidates" && chosen && (
              <button
                type="button"
                onClick={() => setChosen(null)}
                className="text-xs text-zinc-400 hover:text-zinc-100"
              >
                ← pick a different match
              </button>
            )}

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
              <span className="text-xs text-zinc-400">
                Description (optional)
              </span>
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

        {error && resolution?.kind !== "path" && resolution?.kind !== "url" && (
          <div className="mt-3 text-xs text-red-300">{error}</div>
        )}
      </div>
    </div>
  );
}

function basename(p: string): string {
  return p.split("/").pop() ?? p;
}

function urlBasename(url: string): string | null {
  try {
    const u = new URL(url);
    const last = u.pathname.split("/").filter(Boolean).pop();
    return last ?? null;
  } catch {
    return null;
  }
}
