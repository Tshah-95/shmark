import { invoke } from "@tauri-apps/api/core";

/**
 * Calls the shmark daemon via the Tauri rpc command. The Rust side dispatches
 * through `shmark_api::dispatch`, which is the same code path the unix-socket
 * server uses for the CLI.
 */
export async function rpc<T = unknown>(
  method: string,
  params?: Record<string, unknown> | null,
): Promise<T> {
  return invoke<T>("rpc", { method, params: params ?? null });
}

export type Identity = {
  identity_pubkey: string;
  display_name: string;
  created_at: number;
  device: {
    node_pubkey: string;
    endpoint_id: string;
    cert_created_at: number;
  };
};

export type LocalGroup = {
  namespace_id: string;
  local_alias: string;
  created_locally: boolean;
  joined_at: number;
};

export type ShareItem = {
  path: string | null;
  blob_hash: string;
  size_bytes: number;
};

export type ShareRecord = {
  share_id: string;
  name: string;
  description: string | null;
  items: ShareItem[];
  author_identity: string;
  author_node: string;
  created_at: number;
};

export type ListedShare = {
  group: string;
  namespace_id: string;
  share: ShareRecord;
};

export type ShareCode = {
  group: LocalGroup;
  code: string;
  mode: "read" | "write";
};
