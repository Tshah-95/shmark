// Test harness for spawning shmark-desktop in headless mode and driving
// the running app via the unix-socket dispatch + dev_* RPCs.
//
// Run individual specs with `bun tests/e2e/<name>.spec.ts` from the repo
// root. Vite must be running (port 5179) for the frontend to load — the
// harness fails fast with a helpful message if it isn't.
//
// Each TestApp gets its own tempdir + its own socket via SHMARK_DATA_DIR,
// so concurrent runs don't collide and tests don't pollute the user's
// real ~/Library/Application Support/shmark/.

import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, unlinkSync } from "node:fs";
import * as net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

const REPO_ROOT = new URL("../..", import.meta.url).pathname;
const BIN = join(REPO_ROOT, "target/debug/shmark-desktop");
const VITE_URL = "http://localhost:5179";

export const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export type RpcOk<T = unknown> = T;

export class TestApp {
  readonly dataDir: string;
  readonly socket: string;
  proc: ChildProcess | null = null;

  constructor(opts: { dataDir?: string } = {}) {
    this.dataDir = opts.dataDir ?? mkdtempSync(join(tmpdir(), "shmark-e2e-"));
    this.socket = join(this.dataDir, "shmark.sock");
  }

  async rpc<T = unknown>(method: string, params: unknown = {}): Promise<T> {
    return new Promise((resolve, reject) => {
      const sock = net.createConnection(this.socket);
      let buf = "";
      sock.on("connect", () => {
        sock.write(JSON.stringify({ method, params }) + "\n");
      });
      sock.on("data", (d) => (buf += d.toString()));
      sock.on("end", () => {
        try {
          const r = JSON.parse(buf.trim());
          if ("err" in r)
            reject(new Error(`${r.err.code}: ${r.err.message}`));
          else resolve(r.ok as T);
        } catch (e) {
          reject(e);
        }
      });
      sock.on("error", reject);
      setTimeout(() => sock.end(), 50);
    });
  }

  async start(): Promise<void> {
    if (!existsSync(BIN)) {
      throw new Error(
        `shmark-desktop binary not found at ${BIN}. Run \`cargo build -p shmark-tauri\` first.`,
      );
    }
    if (!(await viteAlive())) {
      throw new Error(
        `Vite dev server not reachable at ${VITE_URL}. Start with \`bun --cwd frontend run dev\` first.`,
      );
    }
    if (existsSync(this.socket)) unlinkSync(this.socket);

    const env = { ...process.env, SHMARK_DATA_DIR: this.dataDir };
    this.proc = spawn(BIN, ["--headless"], {
      stdio: "ignore",
      env,
      detached: false,
    });

    // Wait for the socket and a successful daemon_status call.
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
      if (existsSync(this.socket)) {
        try {
          await this.rpc("daemon_status");
          break;
        } catch {
          // not yet
        }
      }
      await sleep(150);
    }
    if (!existsSync(this.socket)) {
      this.kill();
      throw new Error("daemon didn't come up within 15s");
    }

    // Wait for the frontend to load and __SHMARK_TEST__ to install.
    // NB: Tauri's eval_with_callback JSON-encodes the JS result before
    // sending it back, so the JS we send must NOT pre-encode with
    // JSON.stringify — that would double-encode and ruin equality checks.
    const fdeadline = Date.now() + 15_000;
    while (Date.now() < fdeadline) {
      try {
        const r = await this.rpc<{ value: unknown }>("dev_run_get", {
          js: "typeof window.__SHMARK_TEST__ === 'object' && window.__SHMARK_TEST__ !== null",
        });
        if (r.value === true) return;
      } catch {
        // not yet
      }
      await sleep(150);
    }

    this.kill();
    throw new Error("frontend didn't install __SHMARK_TEST__ within 15s");
  }

  async stop(): Promise<void> {
    if (this.proc) {
      try {
        await this.rpc("daemon_stop");
        await sleep(300);
      } catch {
        // ignore
      }
      this.kill();
    }
    if (this.dataDir.includes("shmark-e2e-")) {
      try {
        rmSync(this.dataDir, { recursive: true, force: true });
      } catch {
        // best effort
      }
    }
  }

  kill() {
    if (this.proc && !this.proc.killed) {
      this.proc.kill();
    }
    this.proc = null;
  }

  // --- High-level test helpers ---

  async activeModal(): Promise<string | null> {
    const r = await this.rpc<{ value: unknown }>("dev_run_get", {
      js: "window.__SHMARK_TEST__?.activeModal() ?? null",
    });
    if (r.value === null || r.value === undefined) return null;
    if (typeof r.value === "string") return r.value;
    return String(r.value);
  }

  async hasTestId(id: string): Promise<boolean> {
    const r = await this.rpc<{ value: unknown }>("dev_run_get", {
      js: `window.__SHMARK_TEST__?.has(${JSON.stringify(id)}) ?? false`,
    });
    return r.value === true;
  }

  async click(id: string): Promise<boolean> {
    const r = await this.rpc<{ value: unknown }>("dev_run_get", {
      js: `window.__SHMARK_TEST__?.click(${JSON.stringify(id)}) ?? false`,
    });
    return r.value === true;
  }

  async type(id: string, value: string): Promise<boolean> {
    const r = await this.rpc<{ value: unknown }>("dev_run_get", {
      js: `window.__SHMARK_TEST__?.typeInto(${JSON.stringify(id)}, ${JSON.stringify(value)}) ?? false`,
    });
    return r.value === true;
  }

  async text(id: string): Promise<string | null> {
    const r = await this.rpc<{ value: unknown }>("dev_run_get", {
      js: `window.__SHMARK_TEST__?.text(${JSON.stringify(id)}) ?? null`,
    });
    if (r.value === null) return null;
    return String(r.value);
  }

  async waitFor<T>(
    check: () => Promise<T | null | undefined | false>,
    timeoutMs: number = 5000,
    intervalMs: number = 100,
  ): Promise<T> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const v = await check();
      if (v !== null && v !== undefined && v !== false) return v as T;
      await sleep(intervalMs);
    }
    throw new Error("waitFor timed out");
  }

  async waitForModal(name: string, timeoutMs: number = 5000): Promise<void> {
    await this.waitFor(async () => {
      const m = await this.activeModal();
      return m === name;
    }, timeoutMs);
  }
}

async function viteAlive(): Promise<boolean> {
  try {
    const r = await fetch(VITE_URL, {
      signal: AbortSignal.timeout(1500),
    });
    return r.ok;
  } catch {
    return false;
  }
}

export function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

export function step(name: string) {
  console.log(`  → ${name}`);
}
