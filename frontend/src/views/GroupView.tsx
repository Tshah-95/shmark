import { useEffect, useState } from "react";
import { writeText as clipboardWriteText } from "@tauri-apps/plugin-clipboard-manager";
import {
  rpc,
  type Identity,
  type ListedShare,
  type ShareCode,
  type ShareRecord,
} from "../api";
import { formatBytes, formatRelativeTime, shortHex } from "../util";

type Props = {
  alias: string;
  identity: Identity | null;
  shares: ListedShare[];
  onOpenShare: (share: ShareRecord) => void;
  onShareClipboard: () => void;
};

export function GroupView({
  alias,
  identity,
  shares,
  onOpenShare,
  onShareClipboard,
}: Props) {
  const [note, setNote] = useState<string | null>(null);
  const [editingNote, setEditingNote] = useState(false);
  const [noteDraft, setNoteDraft] = useState("");
  const [codeNotice, setCodeNotice] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const dump = await rpc<{ markdown: string }>("context_dump", {});
        if (cancelled) return;
        // Crude parse: find the "### <alias>\n\n<note>\n\n" section.
        const md = dump.markdown;
        const re = new RegExp(
          `### ${alias.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\$&")}\\n\\n([\\s\\S]*?)\\n\\n`,
        );
        const m = md.match(re);
        if (m) {
          const body = m[1] ?? "";
          setNote(body === "(no note)" ? null : body);
        }
      } catch {
        // ignore — note is optional
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [alias]);

  async function saveNote() {
    const next = noteDraft.trim();
    await rpc("groups_set_note", {
      group: alias,
      note: next.length === 0 ? null : next,
    });
    setNote(next.length === 0 ? null : next);
    setEditingNote(false);
  }

  async function copyShareCode() {
    try {
      const result = await rpc<ShareCode>("groups_share_code", {
        name_or_id: alias,
        read_only: false,
      });
      await clipboardWriteText(result.code);
      setCodeNotice("share code copied to clipboard");
      window.setTimeout(() => setCodeNotice(null), 2500);
    } catch (e) {
      setCodeNotice(`failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <header className="border-b border-zinc-800 px-6 py-4 space-y-2">
        <div className="flex items-center justify-between gap-4">
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
              onClick={onShareClipboard}
              className="text-sm rounded bg-zinc-100 text-zinc-900 hover:bg-white px-3 py-1.5 font-medium"
              data-testid="group-share-clipboard"
            >
              + Share
            </button>
            <button
              onClick={() => void copyShareCode()}
              className="text-sm rounded border border-zinc-700 hover:bg-zinc-800 px-3 py-1.5"
              data-testid="copy-share-code"
            >
              Copy share code
            </button>
          </div>
        </div>
        {/* Note row */}
        <div className="text-xs flex items-start gap-2">
          {editingNote ? (
            <div className="flex-1 flex gap-2">
              <input
                autoFocus
                value={noteDraft}
                onChange={(e) => setNoteDraft(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void saveNote();
                  if (e.key === "Escape") setEditingNote(false);
                }}
                placeholder="routing note for the agent (e.g. 'engineering — no customer data')"
                className="flex-1 rounded bg-zinc-900 border border-zinc-700 px-2 py-1 text-xs focus:outline-none focus:border-zinc-500"
              />
              <button
                onClick={() => void saveNote()}
                className="text-xs text-zinc-200 hover:text-zinc-50"
              >
                save
              </button>
              <button
                onClick={() => setEditingNote(false)}
                className="text-xs text-zinc-500 hover:text-zinc-300"
              >
                cancel
              </button>
            </div>
          ) : (
            <button
              onClick={() => {
                setNoteDraft(note ?? "");
                setEditingNote(true);
              }}
              className="text-left flex-1 text-zinc-400 hover:text-zinc-100"
              data-testid="group-edit-note"
            >
              {note ? (
                <span>
                  <span className="text-zinc-500">note:</span> {note}
                </span>
              ) : (
                <span className="text-zinc-600 italic">+ add a routing note</span>
              )}
            </button>
          )}
        </div>
      </header>

      <div className="flex-1 overflow-auto">
        {shares.length === 0 ? (
          <div className="text-zinc-500 italic px-6 py-8 text-sm">
            No shares yet.{" "}
            <button
              onClick={onShareClipboard}
              className="text-zinc-300 underline hover:text-zinc-100"
            >
              Share something
            </button>
            {" "}or drop a markdown path on your clipboard and hit ⌘⇧P.
          </div>
        ) : (
          <ul className="divide-y divide-zinc-900">
            {shares.map((s) => {
              const isMine =
                identity?.identity_pubkey === s.share.author_identity;
              return (
                <li key={s.share.share_id}>
                  <button
                    onClick={() => onOpenShare(s.share)}
                    className="w-full text-left px-6 py-3 hover:bg-zinc-900/60 transition-colors"
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
                    <div className="text-xs text-zinc-500 mt-1 font-mono flex items-center gap-2">
                      <span>by {isMine ? "you" : shortHex(s.share.author_identity, 6)}</span>
                      <span>· {formatBytes(s.share.items[0]?.size_bytes ?? 0)}</span>
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
