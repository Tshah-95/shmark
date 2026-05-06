import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writeText as clipboardWriteText } from "@tauri-apps/plugin-clipboard-manager";
import { useCallback, useEffect, useState } from "react";
import {
  rpc,
  type Identity,
  type ListedShare,
  type LocalGroup,
  type ShareCode,
  type ShareRecord,
} from "./api";
import { ShareFromClipboard } from "./ShareFromClipboard";
import { ShareView } from "./render/ShareView";
import { formatRelativeTime, shortHex } from "./util";

type View =
  | { kind: "group"; alias: string }
  | { kind: "share"; alias: string; share: ShareRecord };

export function App() {
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [groups, setGroups] = useState<LocalGroup[]>([]);
  const [shares, setShares] = useState<ListedShare[]>([]);
  const [view, setView] = useState<View | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);
  const [showJoin, setShowJoin] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [showShareFromClipboard, setShowShareFromClipboard] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [id, gs, ss] = await Promise.all([
        rpc<Identity>("identity_show"),
        rpc<LocalGroup[]>("groups_list"),
        rpc<ListedShare[]>("shares_list"),
      ]);
      setIdentity(id);
      setGroups(gs);
      setShares(ss);
      setBootError(null);
    } catch (e) {
      setBootError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = window.setInterval(refresh, 3000);
    return () => window.clearInterval(t);
  }, [refresh]);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    (async () => {
      try {
        unlisten = await listen("shmark://hotkey/share", () => {
          setShowShareFromClipboard(true);
        });
      } catch {
        // Tauri event subsystem may not be available in non-Tauri runtimes
        // (e.g. running the frontend in plain Vite for development); fail
        // open silently.
      }
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  if (bootError) {
    return (
      <div className="h-full flex items-center justify-center p-8">
        <div className="max-w-md rounded-lg border border-red-800 bg-red-950/40 p-4">
          <div className="font-medium text-red-100 mb-1">
            Couldn't reach the daemon
          </div>
          <pre className="text-xs whitespace-pre-wrap text-red-200">
            {bootError}
          </pre>
          <button
            onClick={() => void refresh()}
            className="mt-3 px-3 py-1.5 rounded bg-red-700 hover:bg-red-600 text-sm"
          >
            retry
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full grid grid-cols-[240px_minmax(0,1fr)]">
      <aside className="border-r border-zinc-800 bg-zinc-900/40 flex flex-col">
        <div className="px-4 py-4 border-b border-zinc-800">
          <div className="text-sm font-semibold tracking-tight">shmark</div>
          {identity && (
            <div className="text-xs text-zinc-500 mt-0.5 font-mono">
              {identity.display_name} · {shortHex(identity.identity_pubkey, 6)}
            </div>
          )}
        </div>
        <div className="px-3 py-2 flex items-center justify-between">
          <span className="text-xs uppercase text-zinc-500 font-medium tracking-wider">
            Groups
          </span>
          <div className="flex gap-1">
            <button
              onClick={() => setShowCreate(true)}
              className="text-xs text-zinc-400 hover:text-zinc-100"
              title="new group"
            >
              + new
            </button>
            <button
              onClick={() => setShowJoin(true)}
              className="text-xs text-zinc-400 hover:text-zinc-100"
              title="join via code"
            >
              join
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-1.5 pb-2">
          {groups.length === 0 && (
            <div className="text-xs text-zinc-500 italic px-2 py-1.5">
              no groups yet
            </div>
          )}
          {groups.map((g) => {
            const shareCount = shares.filter(
              (s) => s.namespace_id === g.namespace_id,
            ).length;
            const active = view?.kind && view.alias === g.local_alias;
            return (
              <button
                key={g.namespace_id}
                onClick={() => setView({ kind: "group", alias: g.local_alias })}
                className={`w-full text-left rounded px-2.5 py-1.5 text-sm transition-colors ${
                  active
                    ? "bg-zinc-800 text-zinc-50"
                    : "text-zinc-300 hover:bg-zinc-800/60"
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate">{g.local_alias}</span>
                  <span className="text-[10px] text-zinc-500 tabular-nums">
                    {shareCount}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </aside>

      <main className="overflow-hidden flex flex-col">
        {view === null && (
          <Welcome
            hasGroups={groups.length > 0}
            onCreate={() => setShowCreate(true)}
            onJoin={() => setShowJoin(true)}
          />
        )}
        {view?.kind === "group" && (
          <GroupView
            alias={view.alias}
            shares={shares.filter(
              (s) =>
                s.group === view.alias ||
                groups.find((g) => g.local_alias === view.alias)?.namespace_id ===
                  s.namespace_id,
            )}
            onOpenShare={(share) =>
              setView({ kind: "share", alias: view.alias, share })
            }
            onCopiedShareCode={() => void 0}
          />
        )}
        {view?.kind === "share" && (
          <ShareView
            groupAlias={view.alias}
            share={view.share}
            onBack={() => setView({ kind: "group", alias: view.alias })}
          />
        )}
      </main>

      {showCreate && (
        <CreateGroupModal
          onClose={() => setShowCreate(false)}
          onCreated={async () => {
            setShowCreate(false);
            await refresh();
          }}
        />
      )}
      {showJoin && (
        <JoinGroupModal
          onClose={() => setShowJoin(false)}
          onJoined={async () => {
            setShowJoin(false);
            await refresh();
          }}
        />
      )}
      {showShareFromClipboard && (
        <ShareFromClipboard
          groups={groups}
          onClose={() => setShowShareFromClipboard(false)}
          onShared={async () => {
            setShowShareFromClipboard(false);
            await refresh();
          }}
        />
      )}
    </div>
  );
}

function Welcome({
  hasGroups,
  onCreate,
  onJoin,
}: {
  hasGroups: boolean;
  onCreate: () => void;
  onJoin: () => void;
}) {
  return (
    <div className="h-full flex items-center justify-center p-10">
      <div className="max-w-md text-center">
        <div className="text-4xl font-semibold tracking-tight mb-2">shmark</div>
        <div className="text-zinc-400 mb-6">
          {hasGroups
            ? "Pick a group on the left to see shared markdown."
            : "Create a group or join one with a share code."}
        </div>
        <div className="flex gap-2 justify-center">
          <button
            onClick={onCreate}
            className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-4 py-2 text-sm font-medium"
          >
            New group
          </button>
          <button
            onClick={onJoin}
            className="rounded border border-zinc-700 text-zinc-200 hover:bg-zinc-800 px-4 py-2 text-sm font-medium"
          >
            Join with code
          </button>
        </div>
      </div>
    </div>
  );
}

function GroupView({
  alias,
  shares,
  onOpenShare,
  onCopiedShareCode,
}: {
  alias: string;
  shares: ListedShare[];
  onOpenShare: (share: ShareRecord) => void;
  onCopiedShareCode: () => void;
}) {
  const [codeNotice, setCodeNotice] = useState<string | null>(null);

  async function copyShareCode() {
    try {
      const result = await rpc<ShareCode>("groups_share_code", {
        name_or_id: alias,
        read_only: false,
      });
      // Use the Tauri clipboard plugin instead of navigator.clipboard so we
      // bypass the webview's secure-context restrictions in dev mode.
      await clipboardWriteText(result.code);
      setCodeNotice("share code copied to clipboard");
      onCopiedShareCode();
      window.setTimeout(() => setCodeNotice(null), 2500);
    } catch (e) {
      setCodeNotice(`failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <header className="border-b border-zinc-800 px-6 py-4 flex items-center justify-between">
        <div>
          <div className="text-base font-medium">{alias}</div>
          <div className="text-xs text-zinc-500">
            {shares.length} share{shares.length === 1 ? "" : "s"}
          </div>
        </div>
        <div className="flex items-center gap-2">
          {codeNotice && (
            <span className="text-xs text-zinc-400">{codeNotice}</span>
          )}
          <button
            onClick={() => void copyShareCode()}
            className="text-sm rounded border border-zinc-700 hover:bg-zinc-800 px-3 py-1.5"
          >
            Copy share code
          </button>
        </div>
      </header>
      <div className="flex-1 overflow-auto">
        {shares.length === 0 ? (
          <div className="text-zinc-500 italic px-6 py-8 text-sm">
            No shares yet. Drop a markdown file in via the CLI:{" "}
            <code className="px-1 rounded bg-zinc-900 text-zinc-300">
              shmark share path/to/file.md --to {alias}
            </code>
          </div>
        ) : (
          <ul className="divide-y divide-zinc-900">
            {shares.map((s) => (
              <li key={s.share.share_id}>
                <button
                  onClick={() => onOpenShare(s.share)}
                  className="w-full text-left px-6 py-3 hover:bg-zinc-900/60 transition-colors"
                >
                  <div className="flex items-baseline justify-between gap-3">
                    <div className="font-medium truncate">{s.share.name}</div>
                    <div className="text-xs text-zinc-500 tabular-nums whitespace-nowrap">
                      {formatRelativeTime(s.share.created_at)}
                    </div>
                  </div>
                  {s.share.description && (
                    <div className="text-sm text-zinc-400 truncate mt-0.5">
                      {s.share.description}
                    </div>
                  )}
                  <div className="text-xs text-zinc-500 mt-1 font-mono">
                    by {shortHex(s.share.author_identity, 6)} · blob{" "}
                    {shortHex(s.share.items[0]?.blob_hash ?? "", 6)}
                  </div>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function CreateGroupModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => Promise<void> | void;
}) {
  const [alias, setAlias] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!alias.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await rpc<LocalGroup>("groups_new", { alias: alias.trim() });
      await onCreated();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New group" onClose={onClose}>
      <form onSubmit={submit} className="space-y-3">
        <label className="block">
          <span className="text-xs text-zinc-400">Local alias</span>
          <input
            autoFocus
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder="e.g. design-reviews"
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
            disabled={busy || !alias.trim()}
            className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 text-sm font-medium disabled:opacity-50"
          >
            {busy ? "creating…" : "Create"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function JoinGroupModal({
  onClose,
  onJoined,
}: {
  onClose: () => void;
  onJoined: () => Promise<void> | void;
}) {
  const [code, setCode] = useState("");
  const [alias, setAlias] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!code.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await rpc<LocalGroup>("groups_join", {
        code: code.trim(),
        alias: alias.trim() || null,
      });
      await onJoined();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Join a group" onClose={onClose}>
      <form onSubmit={submit} className="space-y-3">
        <label className="block">
          <span className="text-xs text-zinc-400">Share code</span>
          <textarea
            autoFocus
            rows={4}
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="paste a doc ticket here"
            className="mt-1 w-full rounded bg-zinc-900 border border-zinc-700 px-3 py-2 text-xs font-mono focus:outline-none focus:border-zinc-500"
          />
        </label>
        <label className="block">
          <span className="text-xs text-zinc-400">Local alias (optional)</span>
          <input
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder="leave blank for auto"
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
            disabled={busy || !code.trim()}
            className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 text-sm font-medium disabled:opacity-50"
          >
            {busy ? "joining…" : "Join"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function Modal({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
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
          <h2 className="text-base font-medium">{title}</h2>
          <button
            onClick={onClose}
            className="text-zinc-500 hover:text-zinc-200 text-sm"
          >
            ✕
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
