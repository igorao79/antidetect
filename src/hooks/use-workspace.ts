import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

/** Mirror of `workspace::WorkspaceMember`. */
export interface WorkspaceMember {
  id: string;
  name: string;
  role: "owner" | "member" | string;
  createdAt?: string | null;
}

/** Mirror of `workspace::WorkspaceInfo`: the Owl server this app joined. */
export interface WorkspaceInfo {
  server: string;
  name: string;
  member: WorkspaceMember;
}

/** Mirror of `workspace::WorkspaceInvite`. */
export interface WorkspaceInvite {
  id: string;
  code?: string | null;
  link?: string | null;
  createdAt?: string | null;
  expiresAt: string;
}

/** The joined Owl server, kept current through `workspace-changed`. */
export function useWorkspace() {
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [loaded, setLoaded] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setWorkspace(await invoke<WorkspaceInfo | null>("get_workspace"));
    } catch {
      setWorkspace(null);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen("workspace-changed", () => void refresh());
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
    };
  }, [refresh]);

  return { workspace, loaded, refresh };
}
