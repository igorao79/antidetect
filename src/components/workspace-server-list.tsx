"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuPlus, LuTrash2 } from "react-icons/lu";
import { DeleteConfirmationDialog } from "@/components/delete-confirmation-dialog";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  SECTION_LABEL_CLASS,
  SettingsPanel,
} from "@/components/ui/settings-panel";
import type { WorkspaceMember } from "@/hooks/use-workspace";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";

/** Mirror of `workspace_servers::SavedWorkspace`. */
interface SavedWorkspace {
  server: string;
  name: string;
  member: WorkspaceMember;
  active: boolean;
}

function hostOf(server: string) {
  try {
    return new URL(server).host;
  } catch {
    return server;
  }
}

/**
 * The Owl servers this app has joined. One syncs at a time; the others wait
 * with their profiles parked, and switching needs no new invite.
 */
export function WorkspaceServerList({
  onAdd,
  adding,
}: {
  onAdd: () => void;
  adding: boolean;
}) {
  const { t } = useTranslation();
  const [servers, setServers] = useState<SavedWorkspace[]>([]);
  const [switching, setSwitching] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<SavedWorkspace | null>(null);

  const load = useCallback(async () => {
    try {
      setServers(await invoke<SavedWorkspace[]>("list_saved_workspaces"));
    } catch {
      setServers([]);
    }
  }, []);

  useEffect(() => {
    void load();
    const unlisten = listen("workspace-changed", () => void load());
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
    };
  }, [load]);

  const switchTo = async (saved: SavedWorkspace) => {
    setSwitching(saved.server);
    try {
      await invoke("switch_workspace", { server: saved.server });
      showSuccessToast(t("workspace.servers.switched", { name: saved.name }));
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setSwitching(null);
    }
  };

  const forget = async () => {
    if (!forgetting) return;
    try {
      await invoke("forget_saved_workspace", { server: forgetting.server });
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setForgetting(null);
    }
  };

  if (servers.length === 0) return null;
  const hasActive = servers.some((s) => s.active);

  return (
    <SettingsPanel className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <span className={SECTION_LABEL_CLASS}>
          {t("workspace.servers.title")}
        </span>
        {hasActive && !adding && (
          <Button variant="soft" size="sm" onClick={onAdd}>
            <LuPlus className="size-3.5" />
            {t("workspace.servers.add")}
          </Button>
        )}
      </div>
      <ul className="flex flex-col">
        {servers.map((saved) => (
          <li
            key={saved.server}
            className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-foreground/5"
          >
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-sm">{saved.name}</span>
              <span className="truncate font-mono text-xs text-muted-foreground">
                {hostOf(saved.server)}
              </span>
            </div>
            <Badge variant="soft">
              {t(
                `workspace.roles.${saved.member.role === "owner" ? "owner" : "member"}`,
              )}
            </Badge>
            {saved.active ? (
              <Badge className="bg-success/15 text-success">
                {t("workspace.servers.active")}
              </Badge>
            ) : (
              <>
                <LoadingButton
                  variant="soft"
                  size="sm"
                  className="h-7"
                  isLoading={switching === saved.server}
                  disabled={switching !== null}
                  onClick={() => void switchTo(saved)}
                >
                  {t("workspace.servers.switch")}
                </LoadingButton>
                <Button
                  variant="subtle"
                  size="sm"
                  className="h-7"
                  aria-label={t("workspace.servers.forget")}
                  disabled={switching !== null}
                  onClick={() => setForgetting(saved)}
                >
                  <LuTrash2 className="size-3.5" />
                </Button>
              </>
            )}
          </li>
        ))}
      </ul>
      <p className="text-xs text-muted-foreground">
        {t("workspace.servers.description")}
      </p>
      <DeleteConfirmationDialog
        isOpen={forgetting !== null}
        onClose={() => setForgetting(null)}
        onConfirm={forget}
        title={t("workspace.servers.forget")}
        description={t("workspace.servers.forgetConfirm", {
          name: forgetting?.name ?? "",
        })}
        confirmButtonText={t("workspace.servers.forget")}
      />
    </SettingsPanel>
  );
}
