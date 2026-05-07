import type { Identity, ListedShare, ShareRecord } from "../api";
import { formatBytes, formatRelativeTime, shortHex } from "../util";

type Props = {
  identity: Identity | null;
  shares: ListedShare[];
  onOpenShare: (alias: string, share: ShareRecord) => void;
  onShareClipboard: () => void;
};

export function InboxView({
  identity,
  shares,
  onOpenShare,
  onShareClipboard,
}: Props) {
  const sorted = [...shares].sort(
    (a, b) => b.share.created_at - a.share.created_at,
  );

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <header className="border-b border-zinc-800 px-6 py-4 flex items-center justify-between">
        <div>
          <div className="text-base font-medium">Inbox</div>
          <div className="text-xs text-zinc-500">
            {sorted.length} share{sorted.length === 1 ? "" : "s"} across all groups
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-auto">
        {sorted.length === 0 ? (
          <EmptyInbox onShareClipboard={onShareClipboard} />
        ) : (
          <ul className="divide-y divide-zinc-900">
            {sorted.map((s) => {
              const isMine =
                identity?.identity_pubkey === s.share.author_identity;
              return (
                <li key={s.share.share_id}>
                  <button
                    onClick={() => onOpenShare(s.group, s.share)}
                    className="w-full text-left px-6 py-3 hover:bg-zinc-900/60 transition-colors"
                    data-testid={`inbox-share-${s.share.share_id}`}
                  >
                    <div className="flex items-baseline justify-between gap-3">
                      <div className="font-medium truncate flex items-center gap-2">
                        {!isMine && !s.all_local && (
                          <span
                            className="w-1.5 h-1.5 rounded-full bg-blue-400 inline-block"
                            title="syncing"
                          />
                        )}
                        {!isMine && s.all_local && (
                          <span
                            className="w-1.5 h-1.5 rounded-full bg-emerald-400 inline-block"
                            title="downloaded"
                          />
                        )}
                        <span>{s.share.name}</span>
                        <span className="text-xs text-zinc-500 font-normal">
                          · {s.group}
                        </span>
                      </div>
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
                      by {isMine ? "you" : shortHex(s.share.author_identity, 6)}
                      {" · "}
                      {formatBytes(s.share.items[0]?.size_bytes ?? 0)}
                      {s.share.items.length > 1 && ` (${s.share.items.length} items)`}
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}

function EmptyInbox({ onShareClipboard }: { onShareClipboard: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center h-full text-center px-10 py-16">
      <div className="text-3xl mb-2">📥</div>
      <div className="text-base font-medium text-zinc-200 mb-1.5">
        Inbox is empty
      </div>
      <div className="text-sm text-zinc-500 max-w-md mb-5">
        Nothing has been shared with you yet — and you haven't shared anything
        out. Drop a markdown file's path on your clipboard and hit{" "}
        <kbd className="px-1.5 py-0.5 rounded bg-zinc-800 border border-zinc-700 text-xs font-mono">
          ⌘⇧P
        </kbd>
        , or use the button below.
      </div>
      <button
        onClick={onShareClipboard}
        className="rounded bg-zinc-100 text-zinc-900 hover:bg-white px-4 py-2 text-sm font-medium"
      >
        + Share clipboard
      </button>
    </div>
  );
}
