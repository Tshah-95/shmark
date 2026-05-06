type Props = { source: string };

/**
 * Tiny CSV renderer. Parses with naive splitting — not RFC-4180 strict, but
 * fine for the v0 preview. Anything more elaborate gets exported and opened.
 */
export function CsvTable({ source }: Props) {
  const lines = source.replace(/\r\n/g, "\n").split("\n").filter((l) => l.length > 0);
  if (lines.length === 0) {
    return <div className="text-zinc-500 italic">empty csv</div>;
  }
  const rows = lines.map((line) => parseRow(line));
  const header = rows[0]!;
  const body = rows.slice(1);
  return (
    <div className="overflow-auto rounded-lg border border-zinc-800">
      <table className="text-sm w-full">
        <thead className="bg-zinc-900 text-zinc-300">
          <tr>
            {header.map((h, i) => (
              <th key={i} className="px-3 py-2 text-left font-medium border-b border-zinc-800">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.map((r, i) => (
            <tr key={i} className="odd:bg-zinc-950 even:bg-zinc-900/40">
              {r.map((c, j) => (
                <td
                  key={j}
                  className="px-3 py-1.5 border-b border-zinc-900 font-mono text-zinc-200"
                >
                  {c}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function parseRow(line: string): string[] {
  const out: string[] = [];
  let cur = "";
  let inQ = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]!;
    if (ch === '"') {
      if (inQ && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else {
        inQ = !inQ;
      }
    } else if (ch === "," && !inQ) {
      out.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  out.push(cur);
  return out;
}
