import { useEffect, useId, useRef, useState } from "react";

type Props = { source: string };

/**
 * Mermaid block renderer. Lazy-imports mermaid so the bundle stays small for
 * shares that don't use diagrams.
 *
 * mermaid.render returns SVG produced by mermaid's own renderer — with
 * securityLevel "strict" it sanitizes inputs and refuses to embed scripts.
 * That's why we set innerHTML rather than building React elements.
 */
export function Mermaid({ source }: Props) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "_");
  const ref = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const mermaidModule = await import("mermaid");
        const mermaid = mermaidModule.default;
        mermaid.initialize({
          startOnLoad: false,
          theme: "dark",
          securityLevel: "strict",
        });
        const { svg } = await mermaid.render(`mermaid-${id}`, source);
        if (!cancelled && ref.current) {
          ref.current.innerHTML = svg;
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [id, source]);

  if (error) {
    return (
      <div className="rounded-lg border border-red-700 bg-red-950/30 p-3 my-3 text-sm text-red-200">
        <div className="font-medium mb-1">Mermaid render failed</div>
        <pre className="text-xs whitespace-pre-wrap">{error}</pre>
      </div>
    );
  }
  return (
    <div
      ref={ref}
      className="my-3 flex justify-center [&_svg]:max-w-full [&_svg]:h-auto"
    />
  );
}
