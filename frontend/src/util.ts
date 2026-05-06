export function shortHex(s: string, n: number = 8): string {
  if (s.length <= n * 2 + 1) return s;
  return `${s.slice(0, n)}…${s.slice(-n)}`;
}

export function formatRelativeTime(epochSecs: number): string {
  const now = Math.floor(Date.now() / 1000);
  const delta = now - epochSecs;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  if (delta < 86400 * 7) return `${Math.floor(delta / 86400)}d ago`;
  const d = new Date(epochSecs * 1000);
  return d.toLocaleDateString();
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

export function detectFormat(name: string): {
  kind: "markdown" | "code" | "json" | "yaml" | "csv" | "image" | "text";
  lang?: string;
} {
  const lower = name.toLowerCase();
  if (lower.endsWith(".md") || lower.endsWith(".markdown")) return { kind: "markdown" };
  if (lower.endsWith(".json")) return { kind: "json", lang: "json" };
  if (lower.endsWith(".yaml") || lower.endsWith(".yml")) return { kind: "yaml", lang: "yaml" };
  if (lower.endsWith(".csv")) return { kind: "csv" };
  if (
    lower.endsWith(".png") ||
    lower.endsWith(".jpg") ||
    lower.endsWith(".jpeg") ||
    lower.endsWith(".gif") ||
    lower.endsWith(".webp") ||
    lower.endsWith(".svg")
  )
    return { kind: "image" };

  const codeMap: Record<string, string> = {
    ".ts": "ts",
    ".tsx": "tsx",
    ".js": "js",
    ".jsx": "jsx",
    ".py": "python",
    ".rs": "rust",
    ".go": "go",
    ".java": "java",
    ".c": "c",
    ".cpp": "cpp",
    ".h": "c",
    ".hpp": "cpp",
    ".rb": "ruby",
    ".sh": "bash",
    ".sql": "sql",
    ".toml": "toml",
    ".html": "html",
    ".css": "css",
    ".swift": "swift",
    ".kt": "kotlin",
    ".scala": "scala",
    ".php": "php",
    ".lua": "lua",
  };
  for (const [ext, lang] of Object.entries(codeMap)) {
    if (lower.endsWith(ext)) return { kind: "code", lang };
  }
  return { kind: "text" };
}

export function decodeBase64ToText(b64: string): string {
  // atob returns a binary string of bytes; convert to UTF-8.
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
}

export function decodeBase64ToBlobUrl(b64: string, mime: string): string {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return URL.createObjectURL(new Blob([bytes], { type: mime }));
}
