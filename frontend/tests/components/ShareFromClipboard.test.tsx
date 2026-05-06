import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { ShareFromClipboard } from "../../src/ShareFromClipboard";
import type { LocalGroup, ShareRecord } from "../../src/api";

afterEach(() => {
  clearMocks();
});

const groupA: LocalGroup = {
  namespace_id: "a".repeat(64),
  local_alias: "engineering",
  created_locally: true,
  joined_at: 1700000000,
};

type Handler = (cmd: string, args: any) => unknown | undefined;

function install(handler: Handler) {
  mockIPC((cmd, args: any) => handler(cmd, args));
}

describe("ShareFromClipboard", () => {
  it("renders a candidate picker when paths_resolve returns multiple", async () => {
    install((cmd, args) => {
      // clipboard plugin
      if (cmd === "plugin:clipboard-manager|read_text") {
        return "README.md";
      }
      if (cmd === "rpc" && args.method === "paths_resolve") {
        return {
          kind: "candidates",
          candidates: [
            {
              path: "/repo-1/README.md",
              parent_dir: "/repo-1",
              size_bytes: 12,
              mtime_secs: 1700000000,
            },
            {
              path: "/repo-2/README.md",
              parent_dir: "/repo-2",
              size_bytes: 24,
              mtime_secs: 1699000000,
            },
          ],
        };
      }
      return undefined;
    });

    render(
      <ShareFromClipboard groups={[groupA]} onClose={() => {}} onShared={() => {}} />,
    );

    await waitFor(() => {
      expect(screen.getByText(/Multiple matches/i)).toBeInTheDocument();
    });
    expect(screen.getByText("/repo-1")).toBeInTheDocument();
    expect(screen.getByText("/repo-2")).toBeInTheDocument();
  });

  it("auto-fills the form when paths_resolve returns a single match", async () => {
    install((cmd, args) => {
      if (cmd === "plugin:clipboard-manager|read_text")
        return "/Users/tejas/notes/foo.md";
      if (cmd === "rpc" && args.method === "paths_resolve") {
        return {
          kind: "path",
          candidate: {
            path: "/Users/tejas/notes/foo.md",
            parent_dir: "/Users/tejas/notes",
            size_bytes: 100,
            mtime_secs: 1700000000,
          },
        };
      }
      return undefined;
    });

    render(
      <ShareFromClipboard groups={[groupA]} onClose={() => {}} onShared={() => {}} />,
    );

    await waitFor(() => {
      expect(
        screen.getByDisplayValue("foo.md"),
      ).toBeInTheDocument();
    });
    // The full path is shown above the form.
    expect(screen.getByText("/Users/tejas/notes/foo.md")).toBeInTheDocument();
  });

  it("submits share_create with the resolved path", async () => {
    const calls: { method: string; params: any }[] = [];
    const sharedRecords: ShareRecord[] = [];

    install((cmd, args) => {
      if (cmd === "plugin:clipboard-manager|read_text")
        return "/abs/path/to/file.md";
      if (cmd === "rpc") {
        calls.push({ method: args.method, params: args.params });
        if (args.method === "paths_resolve") {
          return {
            kind: "path",
            candidate: {
              path: "/abs/path/to/file.md",
              parent_dir: "/abs/path/to",
              size_bytes: 50,
              mtime_secs: 1700000000,
            },
          };
        }
        if (args.method === "share_create") {
          const r: ShareRecord = {
            share_id: "test-share-id",
            name: args.params.name ?? "file.md",
            description: args.params.description,
            items: [
              {
                path: null,
                blob_hash: "fff",
                size_bytes: 50,
              },
            ],
            author_identity: "0".repeat(64),
            author_node: "1".repeat(64),
            created_at: 1700000000,
          };
          sharedRecords.push(r);
          return r;
        }
      }
      return undefined;
    });

    render(
      <ShareFromClipboard
        groups={[groupA]}
        onClose={() => {}}
        onShared={(r) => {
          sharedRecords.push(r);
        }}
      />,
    );

    // Wait for form to render, then click Share.
    await waitFor(() => {
      expect(screen.getByDisplayValue("file.md")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /^share$/i }));

    await waitFor(() => {
      const submit = calls.find((c) => c.method === "share_create");
      expect(submit).toBeDefined();
      expect(submit?.params.path).toBe("/abs/path/to/file.md");
      expect(submit?.params.group).toBe("engineering");
      expect(submit?.params.name).toBe("file.md");
    });
  });

  it("shows 'unsupported' state for non-path text", async () => {
    install((cmd, args) => {
      if (cmd === "plugin:clipboard-manager|read_text")
        return "just a sentence here";
      if (cmd === "rpc" && args.method === "paths_resolve") {
        return { kind: "unsupported", raw: "just a sentence here" };
      }
      return undefined;
    });

    render(
      <ShareFromClipboard groups={[groupA]} onClose={() => {}} onShared={() => {}} />,
    );
    await waitFor(() => {
      expect(
        screen.getByText(/didn't recognize this clipboard content/i),
      ).toBeInTheDocument();
    });
  });
});
