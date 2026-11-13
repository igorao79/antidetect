"use client";

import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuTimer } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { StoppedQuickProfile } from "@/hooks/use-quick-profile";

interface PromptProps {
  profile: StoppedQuickProfile;
  onKeep: (id: string, name: string) => Promise<void>;
  onDiscard: (id: string) => Promise<void>;
}

function QuickProfileRow({ profile, onKeep, onDiscard }: PromptProps) {
  const { t } = useTranslation();
  const inputId = useId();
  const [name, setName] = useState(profile.name);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setName(profile.name);
  }, [profile.name]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };

  return (
    <li
      data-testid="quick-profile-row"
      data-profile-id={profile.id}
      className="space-y-2"
    >
      <label htmlFor={inputId} className="text-xs text-muted-foreground">
        {t("quickProfile.nameLabel")}
      </label>
      <Input
        id={inputId}
        variant="soft"
        value={name}
        disabled={busy}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && name.trim() && !busy) {
            void run(() => onKeep(profile.id, name.trim()));
          }
        }}
      />
      <div className="flex justify-end gap-1">
        <Button
          type="button"
          size="sm"
          variant="soft"
          className="h-7 text-destructive-text"
          disabled={busy}
          onClick={() => void run(() => onDiscard(profile.id))}
        >
          {t("quickProfile.discard")}
        </Button>
        <Button
          type="button"
          size="sm"
          className="h-7"
          disabled={busy || !name.trim()}
          onClick={() => void run(() => onKeep(profile.id, name.trim()))}
        >
          {t("quickProfile.keep")}
        </Button>
      </div>
    </li>
  );
}

interface Props {
  rows: readonly StoppedQuickProfile[];
  onKeep: PromptProps["onKeep"];
  onDiscard: PromptProps["onDiscard"];
}

/**
 * "Keep this profile?" for quick profiles whose browser has stopped. Not
 * modal: a card in the corner that waits for an answer and takes no focus.
 */
export function QuickProfilePrompt({ rows, onKeep, onDiscard }: Props) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <div role="status" aria-live="polite">
      {rows.length > 0 && (
        <section
          aria-labelledby={titleId}
          data-testid="quick-profile-prompt"
          className="fixed right-4 bottom-4 z-50 w-72 rounded-xl border border-border bg-card p-3 shadow-lg"
        >
          <h2
            id={titleId}
            className="mb-2 flex items-center gap-2 text-sm font-medium text-card-foreground"
          >
            <LuTimer aria-hidden="true" className="size-4 shrink-0" />
            {t("quickProfile.title")}
          </h2>
          <p className="mb-2 text-xs text-muted-foreground">
            {t("quickProfile.body")}
          </p>
          <ul className="space-y-3">
            {rows.map((row) => (
              <QuickProfileRow
                key={row.id}
                profile={row}
                onKeep={onKeep}
                onDiscard={onDiscard}
              />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
