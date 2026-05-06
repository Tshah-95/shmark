// E2E: share a folder via clipboard → multi-item share lands.

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TestApp, assert, step } from "./harness";

function pbcopy(s: string) {
  spawnSync("pbcopy", [], { input: s });
}

const fixtureDir = mkdtempSync(join(tmpdir(), "shmark-folder-fixture-"));
writeFileSync(join(fixtureDir, "a.md"), "# A");
writeFileSync(join(fixtureDir, "b.md"), "# B\nbody");
mkdirSync(join(fixtureDir, "sub"));
writeFileSync(join(fixtureDir, "sub", "c.md"), "nested");

const app = new TestApp();
let exitCode = 0;

try {
  step("starting headless app");
  await app.start();

  step("creating a group");
  await app.rpc("groups_new", { alias: "folder-test" });
  await app.waitFor(async () => app.hasTestId("group-folder-test"), 6000);

  step(`priming clipboard with directory path: ${fixtureDir}`);
  pbcopy(fixtureDir);

  step("emitting hotkey");
  await app.rpc("dev_emit", { event: "shmark://hotkey/share" });

  step("waiting for modal + form");
  await app.waitForModal("share-from-clipboard", 5000);
  await app.waitFor(async () => {
    if (!(await app.hasTestId("share-submit"))) return false;
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "!document.querySelector('[data-testid=share-submit]').disabled",
    });
    return r.value === true;
  }, 5000);

  step("clicking Share");
  await app.click("share-submit");

  step("verifying multi-item share lands");
  const shares = await app.waitFor(async () => {
    const list = await app.rpc<Array<{ share: { items: unknown[] } }>>(
      "shares_list",
      { group: "folder-test" },
    );
    return list.length > 0 ? list : null;
  }, 8000);

  const items = shares[0]?.share.items ?? [];
  assert(items.length === 3, `expected 3 items, got ${items.length}`);
  const paths = items.map((i: any) => i.path).sort();
  assert(
    JSON.stringify(paths) === JSON.stringify(["a.md", "b.md", "sub/c.md"]),
    `paths mismatch: ${JSON.stringify(paths)}`,
  );

  console.log("✓ folder-share E2E passed");
} catch (e) {
  console.error("✗ folder-share E2E failed:");
  console.error(e instanceof Error ? e.stack ?? e.message : e);
  exitCode = 1;
} finally {
  await app.stop();
}

process.exit(exitCode);
