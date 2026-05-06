import { useEffect, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { codeToHtml } from "shiki";
import { Mermaid } from "./Mermaid";

type Props = { source: string };

/**
 * Markdown view for shmark.
 *
 * Renderer policy (per SPEC §11):
 * - Raw HTML is disabled (no rehype-raw).
 * - GitHub-flavored markdown (tables, task lists, autolinks).
 * - Code blocks → shiki-highlighted HTML, plus a special-case for `mermaid`.
 *
 * Note: the shiki output bypasses React reconciliation via
 * dangerouslySetInnerHTML. This is acceptable because shiki produces sanitized
 * HTML it generates itself from the input string — it does not echo HTML from
 * the markdown source.
 */
export function Markdown({ source }: Props) {
  const components: Components = {
    code({ className, children, ...rest }) {
      const value = String(children ?? "").replace(/\n$/, "");
      const m = /language-([\w-]+)/.exec(className ?? "");
      const lang = m?.[1];

      if (!lang) {
        return <code {...rest}>{children}</code>;
      }
      if (lang === "mermaid") {
        return <Mermaid source={value} />;
      }
      return <ShikiCode source={value} lang={lang} />;
    },
  };

  return (
    <div className="prose-sh max-w-none text-zinc-100">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {source}
      </ReactMarkdown>
    </div>
  );
}

function ShikiCode({ source, lang }: { source: string; lang: string }) {
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
        className="rounded-lg overflow-x-auto text-sm"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }
  return (
    <pre>
      <code>{source}</code>
    </pre>
  );
}
