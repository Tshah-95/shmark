// Test driver hook — installed only in dev builds. Production builds tree-
// shake the import via the import.meta.env.DEV branch.
//
// Used by tests/e2e scripts that drive the running headless app via the
// dev_run RPC. Each helper is intentionally tiny so the JS we send through
// the eval channel is small and reliable.

declare global {
  interface Window {
    __SHMARK_TEST__?: {
      /**
       * Returns the value of the data-shmark-modal attribute of any modal
       * currently open, or null if no modal is showing.
       */
      activeModal(): string | null;

      /**
       * True iff at least one element with data-testid=<id> exists.
       */
      has(testid: string): boolean;

      /**
       * Click the element with data-testid=<id>. Returns true if clicked,
       * false if not found.
       */
      click(testid: string): boolean;

      /**
       * textContent of the element with data-testid=<id>, or null.
       */
      text(testid: string): string | null;

      /**
       * innerHTML of the element with data-testid=<id>, or null. Useful for
       * snapshotting rendered markdown / shiki output.
       */
      html(testid: string): string | null;

      /**
       * Type text into the element with data-testid=<id>. Returns true if
       * found.
       */
      typeInto(testid: string, value: string): boolean;
    };
  }
}

export function installTestHooks() {
  if (!import.meta.env.DEV) return;
  window.__SHMARK_TEST__ = {
    activeModal: () => {
      const m = document.querySelector("[data-shmark-modal]");
      return m?.getAttribute("data-shmark-modal") ?? null;
    },
    has: (testid) => !!document.querySelector(`[data-testid="${testid}"]`),
    click: (testid) => {
      const el = document.querySelector(`[data-testid="${testid}"]`) as
        | HTMLElement
        | null;
      if (!el) return false;
      el.click();
      return true;
    },
    text: (testid) => {
      const el = document.querySelector(`[data-testid="${testid}"]`);
      return el?.textContent ?? null;
    },
    html: (testid) => {
      const el = document.querySelector(`[data-testid="${testid}"]`);
      return el ? el.innerHTML : null;
    },
    typeInto: (testid, value) => {
      const el = document.querySelector(`[data-testid="${testid}"]`) as
        | HTMLInputElement
        | HTMLTextAreaElement
        | null;
      if (!el) return false;
      const proto =
        el instanceof HTMLTextAreaElement
          ? HTMLTextAreaElement.prototype
          : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
      setter?.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    },
  };
}
