// E2E: change the hotkey via Settings UI, save, verify settings_get.

import { TestApp, assert, step } from "./harness";

const app = new TestApp();
let exitCode = 0;

try {
  step("starting headless app");
  await app.start();

  step("opening settings");
  await app.click("sidebar-settings");
  await app.waitForModal("settings", 3000);

  step("waiting for settings_get to populate");
  await app.waitFor(async () => {
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "Array.from(document.querySelectorAll('button')).some(b => b.textContent?.trim() === 'Save')",
    });
    return r.value === true;
  }, 5000);

  step("simulating a hotkey rebind to CmdOrCtrl+Shift+L by setting state directly");
  // The HotkeyRecorder uses a hidden input + onKeyDown handler. Driving a
  // real keydown through the dev bridge is fiddly because the input mounts
  // only when "recording" is true. Easier: set the local component state
  // by clicking Change, then dispatching a synthetic keydown on the
  // recorder input.
  const changeClicked = await app.rpc<{ value: unknown }>("dev_run_get", {
    js: "(() => { const btns = Array.from(document.querySelectorAll('button')); const b = btns.find(b => b.textContent?.trim() === 'Change'); if (b) { b.click(); return true; } return false; })()",
  });
  assert(changeClicked.value === true, "Change button should exist + click");

  // Wait for the hidden recorder input to mount
  await app.waitFor(async () => {
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "!!document.querySelector('input.absolute.opacity-0')",
    });
    return r.value === true;
  }, 3000);

  // Dispatch a keydown on it
  await app.rpc("dev_run", {
    js: `(() => {
      const input = document.querySelector('input.absolute.opacity-0');
      if (!input) return;
      const ev = new KeyboardEvent('keydown', { key: 'l', code: 'KeyL', metaKey: true, shiftKey: true, bubbles: true });
      input.dispatchEvent(ev);
    })();`,
  });

  step("verifying displayed hotkey changed");
  await app.waitFor(async () => {
    const r = await app.rpc<{ value: unknown }>("dev_run_get", {
      js: "document.body.innerText.includes('CmdOrCtrl+Shift+KeyL')",
    });
    return r.value === true;
  }, 3000);

  step("clicking Save");
  await app.rpc("dev_run", {
    js: "(() => { const btns = Array.from(document.querySelectorAll('button')); const b = btns.find(b => b.textContent?.trim() === 'Save'); if (b) b.click(); })();",
  });

  step("verifying settings_get reflects the new hotkey");
  await app.waitFor(async () => {
    const s = await app.rpc<{ settings: { hotkey: string } }>(
      "settings_get",
      {},
    );
    return s.settings.hotkey === "CmdOrCtrl+Shift+KeyL";
  }, 5000);

  step("resetting hotkey to default to keep test idempotent");
  await app.rpc("settings_set", { hotkey: "CmdOrCtrl+Shift+P" });

  console.log("✓ settings E2E passed");
} catch (e) {
  console.error("✗ settings E2E failed:");
  console.error(e instanceof Error ? e.stack ?? e.message : e);
  exitCode = 1;
} finally {
  await app.stop();
}

process.exit(exitCode);
