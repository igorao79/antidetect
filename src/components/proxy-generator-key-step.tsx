"use client";

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LuKeyRound } from "react-icons/lu";
import { LoadingButton } from "@/components/loading-button";
import { DialogFooter } from "@/components/ui/dialog";
import { SoftFields } from "@/components/ui/field-variant";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { translateBackendError } from "@/lib/backend-errors";
import {
  clearProxyGeneratorCredentials,
  type ProxyGeneratorStatus,
  setProxyGeneratorCredentials,
} from "@/lib/proxy-generator";
import { RippleButton } from "./ui/ripple";

interface Props {
  status: ProxyGeneratorStatus | null;
  onSaved: (status: ProxyGeneratorStatus) => void;
  onForgotten: () => void;
  onCancel: () => void;
}

/** Asks for the key pair the generator's bot gives with /keys. */
export function ProxyGeneratorKeyStep({
  status,
  onSaved,
  onForgotten,
  onCancel,
}: Props) {
  const { t } = useTranslation();
  const [key, setKey] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState<"save" | "forget" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const canSave = key.trim() !== "" && secret.trim() !== "" && busy === null;

  const save = async () => {
    if (!canSave) return;
    setBusy("save");
    setError(null);
    try {
      onSaved(await setProxyGeneratorCredentials(key, secret));
    } catch (e) {
      setError(translateBackendError(t, e));
    } finally {
      setBusy(null);
    }
  };

  const forget = async () => {
    setBusy("forget");
    setError(null);
    try {
      await clearProxyGeneratorCredentials();
      onForgotten();
    } catch (e) {
      setError(translateBackendError(t, e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <form
      className="grid gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <div className="flex gap-3 rounded-lg bg-primary/8 p-3">
        <LuKeyRound className="mt-0.5 size-4 shrink-0 text-primary" />
        <p className="text-sm text-muted-foreground">
          {t("proxyGenerator.key.hint")}
        </p>
      </div>
      {status?.configured && (
        <p className="text-sm">
          {t("proxyGenerator.key.current", { hint: status.key_hint ?? "" })}
        </p>
      )}
      <SoftFields>
        <div className="grid gap-2">
          <Label htmlFor="proxy-generator-key">
            {t("proxyGenerator.key.keyLabel")}
          </Label>
          <Input
            id="proxy-generator-key"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="pgk_…"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            disabled={busy !== null}
          />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="proxy-generator-secret">
            {t("proxyGenerator.key.secretLabel")}
          </Label>
          <Input
            id="proxy-generator-secret"
            type="password"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            autoComplete="off"
            spellCheck={false}
            disabled={busy !== null}
          />
        </div>
      </SoftFields>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <DialogFooter className="gap-2 sm:justify-between">
        {status?.configured ? (
          <LoadingButton
            type="button"
            variant="ghost"
            isLoading={busy === "forget"}
            disabled={busy !== null}
            onClick={() => void forget()}
          >
            {t("proxyGenerator.key.forget")}
          </LoadingButton>
        ) : (
          <span />
        )}
        <div className="flex gap-2">
          <RippleButton type="button" variant="outline" onClick={onCancel}>
            {t("common.buttons.cancel")}
          </RippleButton>
          <LoadingButton
            type="submit"
            isLoading={busy === "save"}
            disabled={!canSave}
          >
            {t("proxyGenerator.key.save")}
          </LoadingButton>
        </div>
      </DialogFooter>
    </form>
  );
}
