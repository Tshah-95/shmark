// E2E: clicking "Copy share code" actually writes to the OS clipboard.
// The bug we shipped earlier: navigator.clipboard.writeText fails in the
// dev webview's secure context. The fix is the Tauri clipboard plugin —
// this test proves the fix works.

import { spawnSync } from "node:child_process";
import { TestApp, assert, step } from "./harness";

function pbpaste(): string {
  const r = spawnSync("pbpaste", []);
  if (r.status !== 0) throw new Error("pbpaste failed");
  return r.stdout.toString();
}

function pbcopy(s: string) {
  spawnSync("pbcopy", [], { input: s });
}

const app = new TestApp();
let exitCode = 0;

try {
  step("starting headless app");
  await app.start();

  step("creating a group");
  await app.rpc("groups_new", { alias: "copy-test" });
  await app.waitFor(async () => app.hasTestId("group-copy-test"), 6000);

  step("clicking the group in sidebar to switch to its view");
  await app.click("group-copy-test");

  step("waiting for the Copy share code button");
  await app.waitFor(async () => app.hasTestId("copy-share-code"), 5000);

  step("priming clipboard with sentinel so we can detect the change");
  pbcopy("BEFORE");
  assert(pbpaste() === "BEFORE", "pbpaste should reflect the priming");

  step("clicking Copy share code");
  const clicked = await app.click("copy-share-code");
  assert(clicked, "copy-share-code button should be clickable");

  step("waiting for clipboard to contain the iroh ticket");
  await app.waitFor(async () => {
    const v = pbpaste();
    return v !== "BEFORE" && v.startsWith("doc") ? v : null;
  }, 5000);

  const finalClip = pbpaste();
  assert(
    finalClip.startsWith("doc"),
    `clipboard should hold a DocTicket starting with 'doc', got: ${finalClip.slice(0, 30)}...`,
  );

  console.log("✓ copy-share-code E2E passed");
} catch (e) {
  console.error("✗ copy-share-code E2E failed:");
  console.error(e instanceof Error ? e.stack ?? e.message : e);
  exitCode = 1;
} finally {
  await app.stop();
}

process.exit(exitCode);
