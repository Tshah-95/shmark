// E2E: the new Inbox view is the default and surfaces all shares.

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { TestApp, assert, step } from "./harness";

function pbcopy(s: string) {
  spawnSync("pbcopy", [], { input: s });
}

const FIXTURE = join(tmpdir(), `shmark-inbox-${Date.now()}.md`);
writeFileSync(FIXTURE, "# Inbox e2e test\n\nbody");

const app = new TestApp();
let exitCode = 0;

try {
  step("starting headless app");
  await app.start();

  step("inbox is the default view");
  const initialModal = await app.activeModal();
  assert(initialModal === null, "no modal should be open at startup");
  const inboxBtn = await app.hasTestId("sidebar-inbox");
  assert(inboxBtn, "Inbox button is in the sidebar");

  step("creating a group + share");
  await app.rpc("groups_new", { alias: "inbox-test" });
  await app.waitFor(async () => app.hasTestId("group-inbox-test"), 6000);
  pbcopy(FIXTURE);
  await app.rpc("dev_emit", { event: "shmark://hotkey/share" });
  await app.waitForModal("share-from-clipboard", 5000);
  await app.waitFor(async () => {
    if (!(await app.hasTestId("share-submit"))) return false;
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "!document.querySelector('[data-testid=share-submit]').disabled",
    });
    return r.value === true;
  }, 5000);
  await app.click("share-submit");
  await app.waitFor(async () => {
    const list = await app.rpc<unknown[]>("shares_list", {
      group: "inbox-test",
    });
    return list.length > 0;
  }, 8000);

  step("inbox view shows the new share");
  // Navigate back to inbox (we're in the share-from-clipboard modal close
  // → group view; click sidebar inbox).
  await app.click("sidebar-inbox");
  await app.waitFor(async () => {
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "Array.from(document.querySelectorAll('[data-testid^=inbox-share-]')).length",
    });
    return typeof r.value === "number" && r.value > 0;
  }, 5000);

  step("clicking the share from inbox opens it");
  await app.rpc("dev_run", {
    js: "document.querySelector('[data-testid^=inbox-share-]')?.click()",
  });
  await app.waitFor(async () => {
    // ShareView should render — back button appears
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "Array.from(document.querySelectorAll('button')).some(b => b.textContent?.includes('back'))",
    });
    return r.value === true;
  }, 5000);

  step("sidebar +Share clipboard opens the modal");
  // Click back to leave the share view first
  await app.rpc("dev_run", {
    js: "Array.from(document.querySelectorAll('button')).find(b => b.textContent?.includes('back'))?.click()",
  });
  await new Promise((r) => setTimeout(r, 300));

  await app.click("sidebar-share-clipboard");
  await app.waitForModal("share-from-clipboard", 3000);

  console.log("✓ inbox E2E passed");
} catch (e) {
  console.error("✗ inbox E2E failed:");
  console.error(e instanceof Error ? e.stack ?? e.message : e);
  exitCode = 1;
} finally {
  await app.stop();
}

process.exit(exitCode);
