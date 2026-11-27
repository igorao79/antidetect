"use client";

import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/** Stored as is: the profile follows its exit country. */
const FROM_PROXY = "proxy";
/** Unset or this: the backend reads this computer's language at launch. */
const SYSTEM_LANGUAGE = "system";

const PROFILE_LANGUAGES = [
  "en-US",
  "en-GB",
  "ru-RU",
  "uk-UA",
  "kk-KZ",
  "be-BY",
  "de-DE",
  "fr-FR",
  "es-ES",
  "es-MX",
  "it-IT",
  "pt-BR",
  "pt-PT",
  "pl-PL",
  "nl-NL",
  "cs-CZ",
  "sv-SE",
  "tr-TR",
  "ar-SA",
  "hi-IN",
  "id-ID",
  "th-TH",
  "vi-VN",
  "ja-JP",
  "ko-KR",
  "zh-CN",
  "zh-TW",
] as const;

function localeName(tag: string, displayIn: string): string {
  try {
    return (
      new Intl.DisplayNames([displayIn], { type: "language" }).of(tag) ?? tag
    );
  } catch {
    return tag;
  }
}

interface ProfileLanguageFieldProps {
  /** A tag, `proxy` to follow the exit's country, or unset for this computer's. */
  value?: string | null;
  onChange: (value: string | undefined) => void;
  disabled?: boolean;
}

/**
 * The language sites see in a Camoufox profile (fork): this computer's by
 * default, the exit country's, or one the profile keeps whatever proxy it
 * goes through.
 */
export function ProfileLanguageField({
  value,
  onChange,
  disabled,
}: ProfileLanguageFieldProps) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const chosen = value || SYSTEM_LANGUAGE;
  const hint =
    chosen === FROM_PROXY
      ? t("profileLanguage.fromProxyHint")
      : chosen === SYSTEM_LANGUAGE
        ? t("profileLanguage.systemHint")
        : t("profileLanguage.fixedHint");
  const languages: readonly string[] =
    value &&
    value !== SYSTEM_LANGUAGE &&
    value !== FROM_PROXY &&
    !(PROFILE_LANGUAGES as readonly string[]).includes(value)
      ? [value, ...PROFILE_LANGUAGES]
      : PROFILE_LANGUAGES;

  return (
    <div className="min-w-0 space-y-1.5">
      <Label htmlFor={id}>{t("profileLanguage.label")}</Label>
      <Select
        value={chosen}
        onValueChange={(next) => {
          onChange(next === SYSTEM_LANGUAGE ? undefined : next);
        }}
        disabled={disabled}
      >
        <SelectTrigger id={id} data-profile-language={chosen}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={SYSTEM_LANGUAGE}>
            {t("profileLanguage.system")}
          </SelectItem>
          <SelectItem value={FROM_PROXY}>
            {t("profileLanguage.fromProxy")}
          </SelectItem>
          {languages.map((tag) => (
            <SelectItem key={tag} value={tag}>
              {localeName(tag, i18n.language)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs text-muted-foreground">{hint}</p>
    </div>
  );
}
