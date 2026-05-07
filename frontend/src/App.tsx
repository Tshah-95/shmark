import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  rpc,
  type Identity,
  type ListedShare,
  type LocalGroup,
  type ShareRecord,
} from "./api";
import { SettingsPanel } from "./Settings";
import { ShareFromClipboard } from "./ShareFromClipboard";
import { ShareView } from "./render/ShareView";
import { formatRelativeTime, shortHex } from "./util";
import { GroupView } from "./views/GroupView";
import { InboxView } from "./views/InboxView";

type View =
  | { kind: "inbox" }
  | { kind: "group"; alias: string }
  | { kind: "share"; alias: string; share: ShareRecord };

export function App() {
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [groups, setGroups] = useState<LocalGroup[]>([]);
  const [shares, setShares] = useState<ListedShare[]>([]);
  const [view, setView] = useState<View>({ kind: "inbox" });
  const [bootError, setBootError] = useState<string | null>(null);
  const [showJoin, setShowJoin] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [showShareFromClipboard, setShowShareFromClipboard] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [reloadedToast, setReloadedToast] = useState<string | null>(null);
  const seenShareIdsRef = useRef<Set<string>>(new Set());
  const notificationsReadyRef = useRef<boolean>(false);
  const firstLoadRef = useRef<boolean>(true);

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

      const newSeen = seenShareIdsRef.current;
      const isFirst = firstLoadRef.current;
      const fresh = ss.filter((s) => !newSeen.has(s.share.share_id));
      for (const s of ss) newSeen.add(s.share.share_id);
      firstLoadRef.current = false;
      if (!isFirst && id) {
        for (const s of fresh) {
          if (s.share.author_identity === id.identity_pubkey) continue;
          void notifyNewShare(s, notificationsReadyRef);
        }
      }
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
    (async () => {
      try {
        const granted = await isPermissionGranted();
        if (granted) {
          notificationsReadyRef.current = true;
          return;
        }
        const result = await requestPermission();
        notificationsReadyRef.current = result === "granted";
      } catch {
        // notification plugin unavailable
      }
    })();
  }, []);

  useEffect(() => {
    let unlistenHotkey: UnlistenFn | null = null;
    let unlistenReload: UnlistenFn | null = null;
    (async () => {
      try {
        unlistenHotkey = await listen("shmark://hotkey/share", () => {
          setShowShareFromClipboard(true);
        });
        unlistenReload = await listen<{
          identity_pubkey: string;
          display_name: string;
        }>("shmark://reloaded", (event) => {
          seenShareIdsRef.current = new Set();
          firstLoadRef.current = true;
          setReloadedToast(
            `✓ Pairing complete — now signed in as ${event.payload.display_name}.`,
          );
          window.setTimeout(() => setReloadedToast(null), 5000);
          void refresh();
        });
      } catch {
        // event subsystem unavailable
      }
    })();
    return () => {
      if (unlistenHotkey) unlistenHotkey();
      if (unlistenReload) unlistenReload();
    };
  }, [refresh]);

  // Open a group: navigate + mark its unread shares as seen.
  async function selectGroup(alias: string) {
    setView({ kind: "group", alias });
    try {
      await rpc("groups_mark_seen", { name_or_id: alias });
    } catch {
      // best-effort — UI still works
    }
  }

  async function openShare(alias: string, share: ShareRecord) {
    setView({ kind: "share", alias, share });
    try {
      await rpc("groups_mark_seen", { name_or_id: alias });
    } catch {
      // ignore
    }
  }

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

  const sharesForView =
    view.kind === "group"
      ? shares.filter(
          (s) =>
            s.group === view.alias ||
            groups.find((g) => g.local_alias === view.alias)?.namespace_id ===
              s.namespace_id,
        )
      : shares;

  const totalUnread = groups.reduce(
    (sum, g) => sum + (g.unread_count ?? 0),
    0,
  );

  return (
    <div className="h-full grid grid-cols-[260px_minmax(0,1fr)]">
      <aside className="border-r border-zinc-800 bg-zinc-900/40 flex flex-col">
        <div className="px-4 py-4 border-b border-zinc-800">
          <div className="text-sm font-semibold tracking-tight">shmark</div>
          {identity && (
            <div className="text-xs text-zinc-500 mt-0.5 font-mono truncate">
              {identity.display_name} · {shortHex(identity.identity_pubkey, 6)}
            </div>
          )}
        </div>

        <div className="px-3 pt-3 pb-2">
          <button
            onClick={() => setShowShareFromClipboard(true)}
            className="w-full rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 text-sm font-medium"
            data-testid="sidebar-share-clipboard"
          >
            + Share clipboard
          </button>
        </div>

        <button
          onClick={() => setView({ kind: "inbox" })}
          className={`mx-1.5 rounded px-2.5 py-1.5 text-sm text-left transition-colors flex items-center justify-between ${
            view.kind === "inbox"
              ? "bg-zinc-800 text-zinc-50"
              : "text-zinc-300 hover:bg-zinc-800/60"
          }`}
          data-testid="sidebar-inbox"
        >
          <span>📥 Inbox</span>
          {totalUnread > 0 && (
            <span className="text-[10px] tabular-nums bg-blue-500/80 text-white rounded-full px-1.5 min-w-[1.25rem] text-center">
              {totalUnread}
            </span>
          )}
        </button>

        <div className="px-3 pt-3 pb-1 flex items-center justify-between">
          <span className="text-xs uppercase text-zinc-500 font-medium tracking-wider">
            Groups
          </span>
          <div className="flex gap-1">
            <button
              onClick={() => setShowCreate(true)}
              className="text-xs text-zinc-400 hover:text-zinc-100"
              title="new group"
              data-testid="sidebar-new-group"
            >
              + new
            </button>
            <button
              onClick={() => setShowJoin(true)}
              className="text-xs text-zinc-400 hover:text-zinc-100"
              title="join via code"
              data-testid="sidebar-join-group"
            >
              join
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-1.5 pb-2 flex flex-col">
          {groups.length === 0 && (
            <div className="text-xs text-zinc-500 italic px-2 py-1.5">
              no groups yet
            </div>
          )}
          <div className="flex-1">
            {groups.map((g) => {
              const active =
                view.kind === "group" && view.alias === g.local_alias;
              const unread = g.unread_count ?? 0;
              const latest = g.latest_share_at ?? 0;
              return (
                <button
                  key={g.namespace_id}
                  onClick={() => void selectGroup(g.local_alias)}
                  className={`w-full text-left rounded px-2.5 py-1.5 text-sm transition-colors ${
                    active
                      ? "bg-zinc-800 text-zinc-50"
                      : "text-zinc-300 hover:bg-zinc-800/60"
                  }`}
                  data-testid={`group-${g.local_alias}`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span
                      className={`truncate ${unread > 0 ? "font-semibold text-zinc-50" : ""}`}
                    >
                      {g.local_alias}
                    </span>
                    {unread > 0 ? (
                      <span className="text-[10px] tabular-nums bg-blue-500/80 text-white rounded-full px-1.5 min-w-[1.25rem] text-center">
                        {unread}
                      </span>
                    ) : null}
                  </div>
                  {latest > 0 && (
                    <div className="text-[10px] text-zinc-500 tabular-nums mt-0.5">
                      {formatRelativeTime(latest)}
                    </div>
                  )}
                </button>
              );
            })}
          </div>
          <button
            onClick={() => setShowSettings(true)}
            className="mt-2 text-left rounded px-2.5 py-1.5 text-xs text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200 border-t border-zinc-800/60 pt-2"
            data-testid="sidebar-settings"
          >
            ⚙ Settings
          </button>
        </div>
      </aside>

      <main className="overflow-hidden flex flex-col">
        {view.kind === "inbox" && (
          <InboxView
            identity={identity}
            shares={shares}
            onOpenShare={(alias, share) => void openShare(alias, share)}
            onShareClipboard={() => setShowShareFromClipboard(true)}
          />
        )}
        {view.kind === "group" && (
          <GroupView
            alias={view.alias}
            identity={identity}
            shares={sharesForView}
            onOpenShare={(share) => void openShare(view.alias, share)}
            onShareClipboard={() => setShowShareFromClipboard(true)}
          />
        )}
        {view.kind === "share" && (
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
          onCreated={async (alias) => {
            setShowCreate(false);
            await refresh();
            setView({ kind: "group", alias });
          }}
        />
      )}
      {showJoin && (
        <JoinGroupModal
          onClose={() => setShowJoin(false)}
          onJoined={async (alias) => {
            setShowJoin(false);
            await refresh();
            if (alias) setView({ kind: "group", alias });
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
      {showSettings && (
        <SettingsPanel onClose={() => setShowSettings(false)} />
      )}
      {reloadedToast && (
        <div className="fixed bottom-4 right-4 z-50 rounded-lg border border-emerald-700 bg-emerald-950/90 backdrop-blur px-4 py-2.5 text-sm text-emerald-100 shadow-lg max-w-sm">
          {reloadedToast}
        </div>
      )}
    </div>
  );
}

async function notifyNewShare(
  s: ListedShare,
  ready: React.MutableRefObject<boolean>,
) {
  if (!ready.current) return;
  try {
    sendNotification({
      title: `New share in ${s.group}`,
      body: `${s.share.name}${s.share.description ? " — " + s.share.description : ""}`,
    });
  } catch {
    // ignore
  }
}

function CreateGroupModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (alias: string) => Promise<void> | void;
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
      await onCreated(alias.trim());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="New group" testid="create-group" onClose={onClose}>
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
  onJoined: (alias: string | null) => Promise<void> | void;
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
      const r = await rpc<LocalGroup>("groups_join", {
        code: code.trim(),
        alias: alias.trim() || null,
      });
      await onJoined(r.local_alias);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Join a group" testid="join-group" onClose={onClose}>
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
  testid,
  onClose,
  children,
}: {
  title: string;
  testid?: string;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4"
      onClick={onClose}
      data-shmark-modal={testid}
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
