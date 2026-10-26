"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuKeyRound, LuPlus, LuServer, LuTrash2 } from "react-icons/lu";
import { DeleteConfirmationDialog } from "@/components/delete-confirmation-dialog";
import { IntegrationDiagnostics } from "@/components/integration-diagnostics";
import { LoadingButton } from "@/components/loading-button";
import { Button } from "@/components/ui/button";
import { CopyToClipboard } from "@/components/ui/copy-to-clipboard";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  FeatureIcon,
  SECTION_LABEL_CLASS,
  SettingsPanel,
} from "@/components/ui/settings-panel";
import { useWorkspace } from "@/hooks/use-workspace";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";

/** Mirror of `workspace_api::WorkspaceApiKey`. */
interface ApiKey {
  id: string;
  name: string;
  createdAt: string;
  lastUsedAt?: string | null;
  stored: boolean;
}

/** What a script can call with a key, as listed on the page. */
const ENDPOINTS = [
  { method: "GET", path: "/v1/members", key: "members" },
  { method: "DELETE", path: "/v1/members/{id}", key: "removeMember" },
  { method: "POST", path: "/v1/invites", key: "createInvite" },
  { method: "GET", path: "/v1/invites", key: "invites" },
  { method: "DELETE", path: "/v1/invites/{id}", key: "revokeInvite" },
  { method: "GET", path: "/v1/profiles", key: "profiles" },
  { method: "GET", path: "/v1/profile-locks", key: "locks" },
] as const;

/**
 * The joined Owl server's API for scripts, on the Server page: its address,
 * the API keys the owner creates, a check of a key, and what a key can do.
 */
export function WorkspaceApiPanel() {
  const { t, i18n } = useTranslation();
  const { workspace, loaded } = useWorkspace();
  const nameId = useId();
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [checkedKey, setCheckedKey] = useState<string | null>(null);
  const [revoking, setRevoking] = useState<ApiKey | null>(null);
  const isOwner = workspace?.member.role === "owner";

  const load = useCallback(async () => {
    try {
      const listed = await invoke<ApiKey[]>("list_workspace_api_keys");
      setKeys(listed);
      setCheckedKey(
        (current) =>
          listed.find((k) => k.id === current && k.stored)?.id ??
          listed.find((k) => k.stored)?.id ??
          null,
      );
    } catch {
      setKeys([]);
    }
  }, []);

  useEffect(() => {
    if (isOwner) void load();
  }, [isOwner, load]);

  const create = async () => {
    setCreating(true);
    try {
      const key = await invoke<string>("create_workspace_api_key", { name });
      await navigator.clipboard.writeText(key).catch(() => undefined);
      showSuccessToast(t("integrations.server.keyCreated"));
      setName("");
      await load();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setCreating(false);
    }
  };

  const copyKey = async (key: ApiKey) => {
    try {
      const value = await invoke<string>("get_workspace_api_key", {
        id: key.id,
      });
      await navigator.clipboard.writeText(value);
      showSuccessToast(t("integrations.server.keyCopied"));
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    }
  };

  const revoke = async () => {
    if (!revoking) return;
    try {
      await invoke("revoke_workspace_api_key", { id: revoking.id });
      await load();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setRevoking(null);
    }
  };

  const formatDate = (iso: string) =>
    new Date(iso).toLocaleDateString(i18n.language, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });

  // Shown on the Server page to the owner, the one who creates keys.
  if (!loaded || !workspace || !isOwner) return null;

  const example = `curl -H "Authorization: Bearer owlk_…" ${workspace.server}/v1/profiles`;

  return (
    <div className="flex flex-col gap-4">
      <SettingsPanel className="flex flex-col gap-3">
        <div className="flex items-start gap-3">
          <FeatureIcon active>
            <LuServer />
          </FeatureIcon>
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <h3 className="text-sm font-medium">
              {t("integrations.server.title")}
            </h3>
            <p className="text-xs leading-relaxed text-muted-foreground">
              {t("integrations.server.description", { name: workspace.name })}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2 rounded-lg bg-foreground/5 px-3 py-2">
          <span className="min-w-0 flex-1 truncate font-mono text-xs">
            {workspace.server}
          </span>
          <CopyToClipboard
            variant="subtle"
            className="size-7 shrink-0 rounded-md"
            text={workspace.server}
            successMessage={t("integrations.server.addressCopied")}
          />
        </div>
      </SettingsPanel>

      <SettingsPanel className="flex flex-col gap-3">
        <span className={SECTION_LABEL_CLASS}>
          {t("integrations.server.keys")}
        </span>

        {keys.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            {t("integrations.server.noKeys")}
          </p>
        ) : (
          <ul className="flex flex-col">
            {keys.map((key) => (
              <li
                key={key.id}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-foreground/5"
              >
                <LuKeyRound className="size-3.5 shrink-0 text-muted-foreground" />
                <div className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm">{key.name}</span>
                  <span className="truncate text-xs text-muted-foreground tabular-nums">
                    {key.lastUsedAt
                      ? t("integrations.server.keyUsed", {
                          created: formatDate(key.createdAt),
                          used: formatDate(key.lastUsedAt),
                        })
                      : t("integrations.server.keyUnused", {
                          created: formatDate(key.createdAt),
                        })}
                  </span>
                </div>
                {key.stored && (
                  <Button
                    variant="subtle"
                    size="sm"
                    className="h-7"
                    onClick={() => void copyKey(key)}
                  >
                    {t("integrations.server.copyKey")}
                  </Button>
                )}
                <Button
                  variant="subtle"
                  size="sm"
                  className="h-7"
                  aria-label={t("integrations.server.revokeKey")}
                  onClick={() => setRevoking(key)}
                >
                  <LuTrash2 className="size-3.5" />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <form
          className="flex flex-wrap items-end gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            if (name.trim()) void create();
          }}
        >
          <div className="flex min-w-48 flex-1 flex-col gap-1.5">
            <Label htmlFor={nameId}>{t("integrations.server.keyName")}</Label>
            <Input
              id={nameId}
              value={name}
              maxLength={64}
              placeholder={t("integrations.server.keyNamePlaceholder")}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <LoadingButton
            type="submit"
            variant="soft"
            isLoading={creating}
            disabled={!name.trim()}
          >
            <LuPlus className="size-3.5" />
            {t("integrations.server.createKey")}
          </LoadingButton>
        </form>
      </SettingsPanel>

      {checkedKey && (
        <IntegrationDiagnostics
          key={`${workspace.server}:${checkedKey}`}
          target="server"
          serverKeyId={checkedKey}
        />
      )}

      <SettingsPanel className="flex flex-col gap-3">
        <span className={SECTION_LABEL_CLASS}>
          {t("integrations.server.endpoints")}
        </span>
        <ul className="flex flex-col gap-1.5">
          {ENDPOINTS.map((endpoint) => (
            <li
              key={`${endpoint.method} ${endpoint.path}`}
              className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 text-xs"
            >
              <span className="w-14 shrink-0 font-mono text-muted-foreground">
                {endpoint.method}
              </span>
              <span className="font-mono">{endpoint.path}</span>
              <span className="text-muted-foreground">
                {t(`integrations.server.endpoint.${endpoint.key}`)}
              </span>
            </li>
          ))}
        </ul>
        <div className="flex items-center gap-2 rounded-lg bg-foreground/5 px-3 py-2">
          <code className="min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-xs">
            {example}
          </code>
          <CopyToClipboard
            variant="subtle"
            className="size-7 shrink-0 rounded-md"
            text={example}
          />
        </div>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {t("integrations.server.scope")}
        </p>
      </SettingsPanel>

      <DeleteConfirmationDialog
        isOpen={revoking !== null}
        onClose={() => setRevoking(null)}
        onConfirm={revoke}
        title={t("integrations.server.revokeKey")}
        description={t("integrations.server.revokeConfirm", {
          name: revoking?.name ?? "",
        })}
        confirmButtonText={t("integrations.server.revokeKey")}
      />
    </div>
  );
}

/**
 * The Integrations page's way to the server API, which lives on the Server
 * page: the joined server's name and a button there, or why there is none.
 */
export function ServerApiLink({ onOpen }: { onOpen?: () => void }) {
  const { t } = useTranslation();
  const { workspace, loaded } = useWorkspace();
  if (!loaded) return null;
  const isOwner = workspace?.member.role === "owner";
  return (
    <SettingsPanel className="flex items-start gap-3">
      <FeatureIcon active={workspace !== null}>
        <LuServer />
      </FeatureIcon>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <h3 className="truncate text-sm font-medium">
          {workspace
            ? t("integrations.serverLink.title", { name: workspace.name })
            : t("integrations.server.title")}
        </h3>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {!workspace
            ? t("integrations.server.notJoined")
            : isOwner
              ? t("integrations.serverLink.description")
              : t("integrations.server.ownerOnly")}
        </p>
      </div>
      {onOpen && (!workspace || isOwner) && (
        <Button variant="soft" size="sm" className="shrink-0" onClick={onOpen}>
          {t("integrations.serverLink.open")}
        </Button>
      )}
    </SettingsPanel>
  );
}
