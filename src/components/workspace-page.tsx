"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  LuLink,
  LuLogOut,
  LuServer,
  LuUserMinus,
  LuUserPlus,
  LuX,
} from "react-icons/lu";
import { DeleteConfirmationDialog } from "@/components/delete-confirmation-dialog";
import { LoadingButton } from "@/components/loading-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  FeatureIcon,
  SECTION_LABEL_CLASS,
  SettingsPanel,
} from "@/components/ui/settings-panel";
import { WorkspaceApiPanel } from "@/components/workspace-api-panel";
import { WorkspaceDeployForm } from "@/components/workspace-deploy-form";
import { WorkspaceServerList } from "@/components/workspace-server-list";
import {
  useWorkspace,
  type WorkspaceInvite,
  type WorkspaceMember,
} from "@/hooks/use-workspace";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import { cn } from "@/lib/utils";

interface WorkspacePageProps {
  isOpen: boolean;
  onClose: () => void;
  subPage?: boolean;
  /** An invite link opened from outside the app, to fill the join form. */
  initialLink?: string | null;
  /** A section to scroll to once it is shown: the API for scripts. */
  initialSection?: "api" | null;
}

/** The Owl server this app joins: invites, members, the API, leaving. */
export function WorkspacePage({
  isOpen,
  onClose,
  subPage,
  initialLink,
  initialSection,
}: WorkspacePageProps) {
  const { t } = useTranslation();
  const { workspace, loaded } = useWorkspace();
  const [adding, setAdding] = useState(false);

  // A new server joined (or another one switched to) ends the adding.
  const activeServer = workspace?.server ?? null;
  useEffect(() => {
    if (activeServer) setAdding(false);
  }, [activeServer]);
  // An invite link opened from outside while connected adds that server.
  useEffect(() => {
    if (initialLink) setAdding(true);
  }, [initialLink]);

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      subPage={subPage}
    >
      <DialogContent className="flex max-h-[calc(100vh-5rem)] max-w-3xl flex-col">
        {!subPage && (
          <DialogHeader className="shrink-0">
            <DialogTitle>{t("workspace.title")}</DialogTitle>
          </DialogHeader>
        )}
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div
            className={cn(
              "flex flex-col gap-4",
              subPage && "mx-auto w-full max-w-4xl",
            )}
          >
            {loaded && (
              <WorkspaceServerList
                adding={adding}
                onAdd={() => setAdding(true)}
              />
            )}
            {!loaded ? null : workspace && !adding ? (
              <JoinedServer
                key={workspace.server}
                scrollToApi={initialSection === "api"}
              />
            ) : (
              <>
                {workspace && (
                  <div className="flex items-center justify-between gap-3">
                    <h3 className="text-sm font-medium">
                      {t("workspace.servers.add")}
                    </h3>
                    <Button
                      variant="subtle"
                      size="sm"
                      onClick={() => setAdding(false)}
                    >
                      {t("common.buttons.cancel")}
                    </Button>
                  </div>
                )}
                <JoinForm initialLink={initialLink ?? null} />
              </>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function JoinForm({ initialLink }: { initialLink: string | null }) {
  const { t } = useTranslation();
  const linkId = useId();
  const nameId = useId();
  const [link, setLink] = useState(initialLink ?? "");
  const [name, setName] = useState("");
  const [joining, setJoining] = useState(false);

  useEffect(() => {
    if (initialLink) setLink(initialLink);
  }, [initialLink]);

  const join = async () => {
    setJoining(true);
    try {
      const info = await invoke<{ name: string }>("join_workspace", {
        link,
        name,
      });
      showSuccessToast(t("workspace.joined", { name: info.name }));
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setJoining(false);
    }
  };

  return (
    <>
      <SettingsPanel className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <FeatureIcon active={false}>
            <LuLink />
          </FeatureIcon>
          <div className="flex min-w-0 flex-col gap-0.5">
            <h3 className="text-sm font-medium">{t("workspace.joinTitle")}</h3>
            <p className="text-xs leading-relaxed text-muted-foreground">
              {t("workspace.joinDescription")}
            </p>
          </div>
        </div>
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            void join();
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={linkId}>{t("workspace.linkLabel")}</Label>
            <Input
              id={linkId}
              value={link}
              placeholder={t("workspace.linkPlaceholder")}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setLink(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={nameId}>{t("workspace.nameLabel")}</Label>
            <Input
              id={nameId}
              value={name}
              maxLength={64}
              placeholder={t("workspace.namePlaceholder")}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div>
            <LoadingButton
              type="submit"
              isLoading={joining}
              disabled={!link.trim() || !name.trim()}
            >
              {t("workspace.join")}
            </LoadingButton>
          </div>
        </form>
      </SettingsPanel>
      <WorkspaceDeployForm />
    </>
  );
}

function JoinedServer({ scrollToApi }: { scrollToApi: boolean }) {
  const { t, i18n } = useTranslation();
  const apiRef = useRef<HTMLDivElement>(null);
  const { workspace } = useWorkspace();
  const [members, setMembers] = useState<WorkspaceMember[]>([]);
  const [invites, setInvites] = useState<WorkspaceInvite[]>([]);
  const [revoked, setRevoked] = useState(false);
  const [inviting, setInviting] = useState(false);
  const [leaveOpen, setLeaveOpen] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [removing, setRemoving] = useState<WorkspaceMember | null>(null);
  const isOwner = workspace?.member.role === "owner";

  const load = useCallback(async () => {
    try {
      await invoke("refresh_workspace");
      setMembers(await invoke<WorkspaceMember[]>("list_workspace_members"));
      setRevoked(false);
    } catch (error) {
      const message = String(error);
      if (message.includes("WORKSPACE_ACCESS_REVOKED")) {
        setRevoked(true);
        return;
      }
      showErrorToast(translateBackendError(t, error));
    }
  }, [t]);

  const loadInvites = useCallback(async () => {
    try {
      setInvites(await invoke<WorkspaceInvite[]>("list_workspace_invites"));
    } catch {
      setInvites([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (isOwner) void loadInvites();
  }, [isOwner, loadInvites]);

  const formatDate = (iso: string) =>
    new Date(iso).toLocaleDateString(i18n.language, {
      day: "numeric",
      month: "short",
    });

  const createInvite = async () => {
    setInviting(true);
    try {
      const invite = await invoke<WorkspaceInvite>("create_workspace_invite");
      if (invite.link) await navigator.clipboard.writeText(invite.link);
      showSuccessToast(t("workspace.inviteCreated"), {
        description: invite.link ?? undefined,
      });
      await loadInvites();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setInviting(false);
    }
  };

  const revokeInvite = async (invite: WorkspaceInvite) => {
    try {
      await invoke("revoke_workspace_invite", { inviteId: invite.id });
      await loadInvites();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    }
  };

  const removeMember = async () => {
    if (!removing) return;
    try {
      await invoke("remove_workspace_member", { memberId: removing.id });
      showSuccessToast(t("workspace.memberRemoved", { name: removing.name }));
      await load();
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setRemoving(null);
    }
  };

  const leave = async () => {
    setLeaving(true);
    try {
      await invoke("leave_workspace");
      showSuccessToast(t("workspace.left"));
    } catch (error) {
      showErrorToast(translateBackendError(t, error));
    } finally {
      setLeaving(false);
      setLeaveOpen(false);
    }
  };

  const showApi = isOwner && !revoked;
  useEffect(() => {
    if (scrollToApi && showApi) {
      apiRef.current?.scrollIntoView({ block: "start" });
    }
  }, [scrollToApi, showApi]);

  if (!workspace) return null;

  return (
    <>
      <SettingsPanel className="flex items-start gap-3">
        <FeatureIcon active={!revoked}>
          <LuServer />
        </FeatureIcon>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h3 className="truncate text-sm font-medium">{workspace.name}</h3>
          <p className="truncate font-mono text-xs text-muted-foreground">
            {workspace.server}
          </p>
          <p className="text-xs text-muted-foreground">
            {revoked
              ? t("workspace.revoked")
              : t("workspace.connectedAs", {
                  name: workspace.member.name,
                  role: t(`workspace.roles.${isOwner ? "owner" : "member"}`),
                })}
          </p>
        </div>
        <Button variant="soft" size="sm" onClick={() => setLeaveOpen(true)}>
          <LuLogOut className="size-3.5" />
          {t("workspace.leave")}
        </Button>
      </SettingsPanel>

      {!revoked && (
        <SettingsPanel className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-3">
            <span className={SECTION_LABEL_CLASS}>
              {t("workspace.members")}
            </span>
            {isOwner && (
              <LoadingButton
                size="sm"
                variant="soft"
                isLoading={inviting}
                onClick={() => void createInvite()}
              >
                <LuUserPlus className="size-3.5" />
                {t("workspace.invite")}
              </LoadingButton>
            )}
          </div>
          <ul className="flex flex-col">
            {members.map((member) => (
              <li
                key={member.id}
                className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-foreground/5"
              >
                <span className="min-w-0 flex-1 truncate text-sm">
                  {member.name}
                  {member.id === workspace.member.id && (
                    <span className="text-muted-foreground">
                      {" "}
                      ({t("workspace.you")})
                    </span>
                  )}
                </span>
                <Badge variant="soft">
                  {t(
                    `workspace.roles.${member.role === "owner" ? "owner" : "member"}`,
                  )}
                </Badge>
                {isOwner && member.role !== "owner" && (
                  <Button
                    variant="subtle"
                    size="sm"
                    className="h-7"
                    aria-label={t("workspace.removeMember")}
                    onClick={() => setRemoving(member)}
                  >
                    <LuUserMinus className="size-3.5" />
                  </Button>
                )}
              </li>
            ))}
          </ul>
          {!isOwner && (
            <p className="text-xs text-muted-foreground">
              {t("workspace.ownerOnlyHint")}
            </p>
          )}
        </SettingsPanel>
      )}

      {isOwner && !revoked && (
        <SettingsPanel className="flex flex-col gap-3">
          <span className={SECTION_LABEL_CLASS}>{t("workspace.invites")}</span>
          {invites.length === 0 ? (
            <p className="text-xs text-muted-foreground">
              {t("workspace.noInvites")}
            </p>
          ) : (
            <ul className="flex flex-col">
              {invites.map((invite) => (
                <li
                  key={invite.id}
                  className="flex items-center gap-2 rounded-md px-2 py-1.5 hover:bg-foreground/5"
                >
                  <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
                    {t("workspace.inviteExpires", {
                      date: formatDate(invite.expiresAt),
                    })}
                  </span>
                  <Button
                    variant="subtle"
                    size="sm"
                    className="h-7"
                    onClick={() => void revokeInvite(invite)}
                  >
                    <LuX className="size-3.5" />
                    {t("workspace.revokeInvite")}
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </SettingsPanel>
      )}

      {showApi && (
        <div ref={apiRef} data-workspace-section="api" className="scroll-mt-2">
          <WorkspaceApiPanel />
        </div>
      )}

      <DeleteConfirmationDialog
        isOpen={leaveOpen}
        onClose={() => setLeaveOpen(false)}
        onConfirm={leave}
        title={t("workspace.leave")}
        description={t("workspace.leaveDescription")}
        confirmButtonText={t("workspace.leave")}
        isLoading={leaving}
      />
      <DeleteConfirmationDialog
        isOpen={removing !== null}
        onClose={() => setRemoving(null)}
        onConfirm={removeMember}
        title={t("workspace.removeMember")}
        description={t("workspace.removeMemberConfirm", {
          name: removing?.name ?? "",
        })}
        confirmButtonText={t("workspace.removeMember")}
      />
    </>
  );
}

/** Opens the server page when an `owl://join` link reaches the app. */
export function useWorkspaceJoinLink(onLink: (link: string) => void) {
  useEffect(() => {
    let disposed = false;
    void invoke<string | null>("take_pending_workspace_link").then((link) => {
      if (link && !disposed) onLink(link);
    });
    const unlisten = listen<string>("workspace-join-link", (event) => {
      void invoke("take_pending_workspace_link");
      onLink(event.payload);
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => {
        fn();
      });
    };
  }, [onLink]);
}
