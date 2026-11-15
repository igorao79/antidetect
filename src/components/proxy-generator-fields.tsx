"use client";

import { useTranslation } from "react-i18next";
import { SoftFields } from "@/components/ui/field-variant";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  type GeneratorFormState,
  type GeneratorProtocol,
  type ProxyGeneratorOptions,
  TEXT_MAX,
  type TtlChoice,
  type TtlUnit,
} from "@/lib/proxy-generator-form";
import { ProxyGeneratorCountries } from "./proxy-generator-countries";
import { SegmentedChoice } from "./segmented-choice";

const HOURS_IN_DAY = 24;
const UNITS: TtlUnit[] = ["minutes", "hours", "days"];
const PROTOCOLS: GeneratorProtocol[] = ["socks5", "http"];

interface Props {
  options: ProxyGeneratorOptions;
  form: GeneratorFormState;
  single: boolean;
  disabled: boolean;
  onChange: (form: GeneratorFormState) => void;
}

function ttlKey(ttl: TtlChoice): string {
  if (ttl.kind === "preset") return `h${ttl.hours}`;
  return ttl.kind;
}

/** The fields of the generate form; the dialog owns the state. */
export function ProxyGeneratorFields({
  options,
  form,
  single,
  disabled,
  onChange,
}: Props) {
  const { t } = useTranslation();
  const set = (patch: Partial<GeneratorFormState>) =>
    onChange({ ...form, ...patch });
  const presetLabel = (hours: number) =>
    hours % HOURS_IN_DAY === 0
      ? t("proxyGenerator.ttl.days", { count: hours / HOURS_IN_DAY })
      : t("proxyGenerator.ttl.hours", { count: hours });
  const ttlChoices = [
    { value: "forever", label: t("proxyGenerator.ttl.forever") },
    ...options.ttl_hours.map((hours) => ({
      value: `h${hours}`,
      label: presetLabel(hours),
    })),
    { value: "custom", label: t("proxyGenerator.ttl.custom") },
  ];
  const pickTtl = (key: string) => {
    if (key === "forever") set({ ttl: { kind: "forever" } });
    else if (key === "custom")
      set({ ttl: { kind: "custom", amount: "", unit: "hours" } });
    else set({ ttl: { kind: "preset", hours: Number(key.slice(1)) } });
  };

  return (
    <SoftFields>
      <div className="grid gap-4">
        {!single && (
          <div className="grid gap-2">
            <Label htmlFor="proxy-generator-count">
              {t("proxyGenerator.count", { max: options.bulk_max })}
            </Label>
            <Input
              id="proxy-generator-count"
              type="number"
              inputMode="numeric"
              min={1}
              max={options.bulk_max}
              value={form.count}
              onChange={(e) => set({ count: Number(e.target.value) })}
              className="w-28 tabular-nums"
              disabled={disabled}
            />
          </div>
        )}

        <div className="grid gap-2">
          <Label>{t("proxyGenerator.countries.label")}</Label>
          <ProxyGeneratorCountries
            options={options}
            mode={form.countriesMode}
            countries={form.countries}
            disabled={disabled}
            onChange={(countriesMode, countries) =>
              set({ countriesMode, countries })
            }
          />
        </div>

        <div className="grid gap-2">
          <Label>{t("proxyGenerator.ttl.label")}</Label>
          <SegmentedChoice
            label={t("proxyGenerator.ttl.label")}
            value={ttlKey(form.ttl)}
            choices={ttlChoices}
            disabled={disabled}
            onChange={pickTtl}
          />
          {form.ttl.kind === "custom" && (
            <div className="flex gap-2">
              <Input
                aria-label={t("proxyGenerator.ttl.amount")}
                inputMode="numeric"
                value={form.ttl.amount}
                onChange={(e) =>
                  form.ttl.kind === "custom" &&
                  set({ ttl: { ...form.ttl, amount: e.target.value } })
                }
                className="w-24 tabular-nums"
                autoFocus
                disabled={disabled}
              />
              <Select
                value={form.ttl.unit}
                onValueChange={(unit: TtlUnit) =>
                  form.ttl.kind === "custom" &&
                  set({ ttl: { ...form.ttl, unit } })
                }
                disabled={disabled}
              >
                <SelectTrigger
                  className="w-32"
                  aria-label={t("proxyGenerator.ttl.unit")}
                >
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {UNITS.map((unit) => (
                    <SelectItem key={unit} value={unit}>
                      {t(`proxyGenerator.ttl.units.${unit}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          <p className="text-xs text-muted-foreground">
            {t("proxyGenerator.ttl.hint")}
          </p>
        </div>

        <div className="grid gap-2">
          <Label>{t("proxyGenerator.protocol")}</Label>
          <SegmentedChoice
            label={t("proxyGenerator.protocol")}
            value={form.protocol}
            choices={PROTOCOLS.map((p) => ({
              value: p,
              label: p.toUpperCase(),
            }))}
            disabled={disabled}
            onChange={(protocol) => set({ protocol })}
          />
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div className="grid gap-2">
            <Label htmlFor="proxy-generator-name">
              {t("proxyGenerator.name")}
            </Label>
            <Input
              id="proxy-generator-name"
              value={form.name}
              maxLength={TEXT_MAX}
              placeholder={
                single
                  ? t("proxyGenerator.namePlaceholder")
                  : t("proxyGenerator.namePlaceholderBatch")
              }
              onChange={(e) => set({ name: e.target.value })}
              disabled={disabled}
            />
          </div>
          <div className="grid gap-2">
            <Label htmlFor="proxy-generator-note">
              {t("proxies.note.label")}
            </Label>
            <Input
              id="proxy-generator-note"
              value={form.note}
              maxLength={TEXT_MAX}
              placeholder={t("proxies.note.placeholder")}
              onChange={(e) => set({ note: e.target.value })}
              disabled={disabled}
            />
          </div>
        </div>
      </div>
    </SoftFields>
  );
}
