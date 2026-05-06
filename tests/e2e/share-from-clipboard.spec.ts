// E2E smoke: hotkey emit → modal opens → submit → share lands.
//
// Run with: bun tests/e2e/share-from-clipboard.spec.ts
//
// The clipboard is primed via macOS pbcopy because dev_run is
// fire-and-forget — async clipboard writes from inside the webview can't be
// reliably awaited from the test side.

import { spawnSync, writeFileSync } from "node:fs";
import { spawnSync as spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TestApp, assert, step } from "./harness";

const FIXTURE = join(tmpdir(), `shmark-e2e-fixture-${Date.now()}.md`);
writeFileSync(
  FIXTURE,
  "# Hello from E2E\n\nThis was added via the test driver.\n",
);

function pbcopy(s: string) {
  const r = spawn("pbcopy", [], { input: s });
  if (r.status !== 0) {
    throw new Error(`pbcopy failed: ${r.stderr.toString()}`);
  }
}

const app = new TestApp();
let exitCode = 0;

try {
  step("starting headless app");
  await app.start();

  step("creating a group");
  const group = await app.rpc<{ local_alias: string }>("groups_new", {
    alias: "e2e-grp",
  });
  assert(group.local_alias === "e2e-grp", "group alias should match");

  step("waiting for App to pick up the new group (sidebar refresh)");
  await app.waitFor(async () => app.hasTestId("group-e2e-grp"), 6000);

  step("priming the clipboard via pbcopy");
  pbcopy(FIXTURE);

  step("emitting the share hotkey event");
  await app.rpc("dev_emit", { event: "shmark://hotkey/share" });

  step("waiting for share-from-clipboard modal");
  await app.waitForModal("share-from-clipboard", 5000);

  step("waiting for resolve to populate the form + submit to enable");
  await app.waitFor(async () => {
    if (!(await app.hasTestId("share-submit"))) return false;
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "!document.querySelector('[data-testid=share-submit]').disabled",
    });
    return r.value === true;
  }, 5000);

  step("clicking Share");
  const clicked = await app.click("share-submit");
  assert(clicked, "share-submit click should succeed");

  step("waiting for share to appear in shares_list");
  const shares = await app.waitFor(async () => {
    const list = await app.rpc<unknown[]>("shares_list", { group: "e2e-grp" });
    return list.length > 0 ? list : null;
  }, 8000);
  assert(Array.isArray(shares), "shares_list should be array");
  assert(shares.length >= 1, "at least one share present");

  console.log("✓ share-from-clipboard E2E passed");
} catch (e) {
  console.error("✗ share-from-clipboard E2E failed:");
  console.error(e instanceof Error ? e.stack ?? e.message : e);
  exitCode = 1;
} finally {
  await app.stop();
}

process.exit(exitCode);
