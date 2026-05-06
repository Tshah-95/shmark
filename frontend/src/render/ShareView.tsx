import { useEffect, useState } from "react";
import { rpc, type ShareRecord } from "../api";
import {
  decodeBase64ToBlobUrl,
  decodeBase64ToText,
  detectFormat,
  formatBytes,
  formatRelativeTime,
  shortHex,
} from "../util";
import { CodeView } from "./CodeView";
import { CsvTable } from "./CsvTable";
import { Markdown } from "./Markdown";

type Props = {
  groupAlias: string;
  share: ShareRecord;
  onBack: () => void;
};

export function ShareView({ groupAlias, share, onBack }: Props) {
  const [text, setText] = useState<string | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const item = share.items[0]!;
  const fmt = detectFormat(share.name);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setText(null);
    setImageUrl(null);

    (async () => {
      try {
        const { bytes_b64 } = await rpc<{ bytes_b64: string; len: number }>(
          "share_get_bytes",
          { group: groupAlias, share_id: share.share_id, item: 0 },
        );
        if (cancelled) return;
        if (fmt.kind === "image") {
          const mime = mimeForExt(share.name);
          setImageUrl(decodeBase64ToBlobUrl(bytes_b64, mime));
        } else {
          setText(decodeBase64ToText(bytes_b64));
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
      if (imageUrl) URL.revokeObjectURL(imageUrl);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groupAlias, share.share_id]);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <header className="border-b border-zinc-800 px-6 py-4 flex items-start gap-4">
        <button
          onClick={onBack}
          className="text-zinc-400 hover:text-zinc-100 text-sm font-medium"
        >
          ← back
        </button>
        <div className="flex-1 min-w-0">
          <div className="text-base font-medium truncate">{share.name}</div>
          {share.description && (
            <div className="text-sm text-zinc-400 truncate">{share.description}</div>
          )}
          <div className="text-xs text-zinc-500 mt-1 flex flex-wrap gap-x-3 gap-y-1 font-mono">
            <span>by {shortHex(share.author_identity)}</span>
            <span>{formatRelativeTime(share.created_at)}</span>
            <span>{formatBytes(item.size_bytes)}</span>
            <span>blob {shortHex(item.blob_hash)}</span>
          </div>
        </div>
      </header>

      <div className="flex-1 overflow-auto px-6 py-5">
        {loading && (
          <div className="text-zinc-500 text-sm italic">fetching content…</div>
        )}
        {error && (
          <div className="rounded-lg border border-red-800 bg-red-950/30 p-3 text-sm text-red-200">
            <div className="font-medium mb-1">Render failed</div>
            <pre className="text-xs whitespace-pre-wrap">{error}</pre>
          </div>
        )}
        {!loading && !error && text !== null && (
          <RenderText text={text} kind={fmt.kind} lang={fmt.lang} />
        )}
        {!loading && !error && imageUrl && (
          <img src={imageUrl} alt={share.name} className="max-w-full rounded-lg" />
        )}
      </div>
    </div>
  );
}

function RenderText({
  text,
  kind,
  lang,
}: {
  text: string;
  kind: ReturnType<typeof detectFormat>["kind"];
  lang?: string;
}) {
  if (kind === "markdown") return <Markdown source={text} />;
  if (kind === "csv") return <CsvTable source={text} />;
  if (kind === "code" && lang) return <CodeView source={text} lang={lang} />;
  if (kind === "json") return <CodeView source={text} lang="json" />;
  if (kind === "yaml") return <CodeView source={text} lang="yaml" />;
  return (
    <pre className="font-mono text-sm whitespace-pre-wrap text-zinc-200">{text}</pre>
  );
}

function mimeForExt(name: string): string {
  const lower = name.toLowerCase();
  if (lower.endsWith(".png")) return "image/png";
  if (lower.endsWith(".jpg") || lower.endsWith(".jpeg")) return "image/jpeg";
  if (lower.endsWith(".gif")) return "image/gif";
  if (lower.endsWith(".webp")) return "image/webp";
  if (lower.endsWith(".svg")) return "image/svg+xml";
  return "application/octet-stream";
}
