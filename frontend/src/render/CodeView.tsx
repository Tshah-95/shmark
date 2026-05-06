import { useEffect, useState } from "react";
import { codeToHtml } from "shiki";

type Props = { source: string; lang: string };

/**
 * Code-file view (single language, full content). shiki-highlighted with the
 * github-dark theme; falls back to plain pre/code if the language isn't
 * supported.
 *
 * The HTML is rendered via dangerouslySetInnerHTML because shiki emits its
 * own sanitized markup from the input string. We do not echo HTML from the
 * source.
 */
export function CodeView({ source, lang }: Props) {
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const out = await codeToHtml(source, { lang, theme: "github-dark" });
        if (!cancelled) setHtml(out);
      } catch {
        if (!cancelled) setHtml(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [source, lang]);

  if (html) {
    return (
      <div
        className="rounded-lg overflow-x-auto text-sm border border-zinc-800"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }
  return (
    <pre className="rounded-lg border border-zinc-800 bg-zinc-900 p-4 text-sm overflow-x-auto">
      <code>{source}</code>
    </pre>
  );
}
