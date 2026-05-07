import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsPanel } from "../../src/Settings";

afterEach(() => {
  clearMocks();
});

const defaultBundle = {
  settings: {
    hotkey: "CmdOrCtrl+Shift+P",
    search_roots: [] as string[],
    auto_pin: true,
  },
  effective_search_roots: ["/Users/tejas/Github"],
  default_roots: ["/Users/tejas/Github"],
};

function installMockIpc(handler?: (method: string, params: unknown) => unknown) {
  mockIPC((cmd, args: any) => {
    if (cmd === "rpc") {
      const m = args?.method as string;
      const p = args?.params;
      if (handler) {
        const r = handler(m, p);
        if (r !== undefined) return r;
      }
      if (m === "settings_get") return defaultBundle;
      if (m === "settings_set") {
        return {
          hotkey: p?.hotkey ?? defaultBundle.settings.hotkey,
          search_roots: p?.search_roots ?? defaultBundle.settings.search_roots,
          auto_pin: p?.auto_pin ?? defaultBundle.settings.auto_pin,
        };
      }
      if (m === "contacts_list") return [];
    }
    return undefined;
  });
}

describe("SettingsPanel", () => {
  it("loads and displays current settings on mount", async () => {
    installMockIpc();
    render(<SettingsPanel onClose={() => {}} />);

    // The hotkey value should appear in the recorder display once the
    // settings_get RPC resolves.
    await waitFor(() => {
      expect(screen.getByText("CmdOrCtrl+Shift+P")).toBeInTheDocument();
    });

    // Auto-pin checkbox is checked by default.
    const checkbox = screen.getByRole("checkbox", { name: /auto-fetch/i });
    expect(checkbox).toBeChecked();
  });

  it("HotkeyRecorder captures a key combo and updates the displayed accelerator", async () => {
    installMockIpc();
    render(<SettingsPanel onClose={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("CmdOrCtrl+Shift+P")).toBeInTheDocument();
    });

    // Click "Change" to start recording.
    fireEvent.click(screen.getByRole("button", { name: /change/i }));

    // Wait for the hidden recorder input to mount (recording state → render).
    let hiddenInput: HTMLInputElement | null = null;
    await waitFor(() => {
      hiddenInput = document.querySelector(
        'input[class*="absolute"]',
      ) as HTMLInputElement | null;
      expect(hiddenInput).not.toBeNull();
    });

    // Fire a Cmd+Shift+M keydown on the recorder input.
    fireEvent.keyDown(hiddenInput!, {
      key: "m",
      code: "KeyM",
      metaKey: true,
      shiftKey: true,
    });

    await waitFor(() => {
      expect(screen.getByText("CmdOrCtrl+Shift+KeyM")).toBeInTheDocument();
    });
  });

  it("Save invokes settings_set with the form payload", async () => {
    const calls: { method: string; params: any }[] = [];
    installMockIpc((m, p) => {
      calls.push({ method: m, params: p });
      return undefined; // fall through to defaults
    });

    render(<SettingsPanel onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText("CmdOrCtrl+Shift+P")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      const setCall = calls.find((c) => c.method === "settings_set");
      expect(setCall).toBeDefined();
      // Should include all three fields.
      expect(setCall?.params.hotkey).toBe("CmdOrCtrl+Shift+P");
      expect(setCall?.params.search_roots).toEqual([]);
      expect(setCall?.params.auto_pin).toBe(true);
    });
  });

  it("displays default roots when user has no custom roots configured", async () => {
    installMockIpc();
    render(<SettingsPanel onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByText(/Defaults currently in effect/i)).toBeInTheDocument();
    });
    expect(screen.getByText("/Users/tejas/Github")).toBeInTheDocument();
  });
});

// Pin a vitest mock import so vitest doesn't tree-shake it
vi.mock?.("placeholder", () => ({}));
