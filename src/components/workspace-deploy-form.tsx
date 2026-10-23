"use client";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuServer } from "react-icons/lu";
import { LoadingButton } from "@/components/loading-button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { OperationFlow } from "@/components/ui/operation-flow";
import { FeatureIcon, SettingsPanel } from "@/components/ui/settings-panel";
import { translateBackendError } from "@/lib/backend-errors";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";

const STEPS = ["connect", "docker", "server", "https", "owner"] as const;
type Step = (typeof STEPS)[number];

/**
 * Install an Owl server on a VPS over SSH and join it as its owner
 * (`deploy_workspace_server`).
 */
export function WorkspaceDeployForm() {
  const { t } = useTranslation();
  const ids = {
    host: useId(),
    port: useId(),
    user: useId(),
    password: useId(),
    key: useId(),
    passphrase: useId(),
    domain: useId(),
    serverName: useId(),
    memberName: useId(),
  };
  const [host, setHost] = useState("");
  const [port, setPort] = useState("22");
  const [user, setUser] = useState("root");
  const [password, setPassword] = useState("");
  const [keyPath, setKeyPath] = useState("");
  const [keyPassphrase, setKeyPassphrase] = useState("");
  const [domain, setDomain] = useState("");
  const [serverName, setServerName] = useState("");
  const [memberName, setMemberName] = useState("");
  const [deploying, setDeploying] = useState(false);
  const [step, setStep] = useState<Step | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const unlisten = listen<{ step: Step }>(
      "workspace-deploy-progress",
      (event) => setStep(event.payload.step),
    );
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
    };
  }, []);

  const deploy = async () => {
    setDeploying(true);
    setFailed(false);
    setStep("connect");
    try {
      const info = await invoke<{ name: string }>("deploy_workspace_server", {
        request: {
          host,
          port: Number(port) || 22,
          user,
          password: password || null,
          keyPath: keyPath || null,
          keyPassphrase: keyPassphrase || null,
          domain: domain || null,
          serverName,
          memberName,
        },
      });
      showSuccessToast(t("workspace.deploy.done", { name: info.name }));
    } catch (error) {
      setFailed(true);
      showErrorToast(translateBackendError(t, error));
    } finally {
      setDeploying(false);
    }
  };

  const canDeploy =
    host.trim() &&
    user.trim() &&
    (password || keyPath.trim()) &&
    memberName.trim() &&
    !deploying;

  return (
    <SettingsPanel className="flex flex-col gap-4">
      <div className="flex items-start gap-3">
        <FeatureIcon active={false}>
          <LuServer />
        </FeatureIcon>
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 className="text-sm font-medium">{t("workspace.deploy.title")}</h3>
          <p className="text-xs leading-relaxed text-muted-foreground">
            {t("workspace.deploy.description")}
          </p>
        </div>
      </div>
      <form
        className="flex flex-col gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (canDeploy) void deploy();
        }}
      >
        <fieldset disabled={deploying} className="flex flex-col gap-3">
          <div className="grid grid-cols-[minmax(0,1fr)_6rem] gap-3">
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.host}>
                {t("workspace.deploy.hostLabel")}
              </Label>
              <Input
                id={ids.host}
                value={host}
                placeholder="203.0.113.7"
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => setHost(event.target.value)}
              />
            </div>
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.port}>
                {t("workspace.deploy.portLabel")}
              </Label>
              <Input
                id={ids.port}
                value={port}
                inputMode="numeric"
                className="tabular-nums"
                onChange={(event) =>
                  setPort(event.target.value.replace(/\D/g, "").slice(0, 5))
                }
              />
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.user}>
                {t("workspace.deploy.userLabel")}
              </Label>
              <Input
                id={ids.user}
                value={user}
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => setUser(event.target.value)}
              />
            </div>
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.password}>
                {t("workspace.deploy.passwordLabel")}
              </Label>
              <Input
                id={ids.password}
                type="password"
                value={password}
                autoComplete="off"
                onChange={(event) => setPassword(event.target.value)}
              />
            </div>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.key}>{t("workspace.deploy.keyLabel")}</Label>
              <Input
                id={ids.key}
                value={keyPath}
                placeholder="~/.ssh/id_ed25519"
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => setKeyPath(event.target.value)}
              />
            </div>
            {keyPath.trim() && (
              <div className="flex min-w-0 flex-col gap-1.5">
                <Label htmlFor={ids.passphrase}>
                  {t("workspace.deploy.keyPassphraseLabel")}
                </Label>
                <Input
                  id={ids.passphrase}
                  type="password"
                  value={keyPassphrase}
                  autoComplete="off"
                  onChange={(event) => setKeyPassphrase(event.target.value)}
                />
              </div>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={ids.domain}>
              {t("workspace.deploy.domainLabel")}
            </Label>
            <Input
              id={ids.domain}
              value={domain}
              placeholder="owl.example.com"
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setDomain(event.target.value)}
            />
            <p className="text-xs text-muted-foreground">
              {t("workspace.deploy.domainHint")}
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.serverName}>
                {t("workspace.deploy.serverNameLabel")}
              </Label>
              <Input
                id={ids.serverName}
                value={serverName}
                maxLength={64}
                placeholder={t("workspace.deploy.serverNamePlaceholder")}
                onChange={(event) => setServerName(event.target.value)}
              />
            </div>
            <div className="flex min-w-0 flex-col gap-1.5">
              <Label htmlFor={ids.memberName}>{t("workspace.nameLabel")}</Label>
              <Input
                id={ids.memberName}
                value={memberName}
                maxLength={64}
                placeholder={t("workspace.namePlaceholder")}
                onChange={(event) => setMemberName(event.target.value)}
              />
            </div>
          </div>
        </fieldset>
        {step && (
          <div className="flex flex-col gap-1">
            <OperationFlow
              label={t("workspace.deploy.title")}
              active={STEPS.indexOf(step)}
              busy={deploying}
              failed={failed}
              steps={STEPS.map((id) => ({
                id,
                label: t(`workspace.deploy.steps.${id}`),
                detail: "",
              }))}
            />
            {deploying && (
              <p role="status" className="text-xs text-muted-foreground">
                {t("workspace.deploy.running")}
              </p>
            )}
          </div>
        )}
        <div>
          <LoadingButton
            type="submit"
            isLoading={deploying}
            disabled={!canDeploy}
          >
            {t("workspace.deploy.submit")}
          </LoadingButton>
        </div>
      </form>
      <p className="text-xs leading-relaxed text-muted-foreground">
        {t("workspace.deploy.manual")}
      </p>
    </SettingsPanel>
  );
}
