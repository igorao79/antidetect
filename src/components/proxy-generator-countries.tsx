"use client";

import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuCheck, LuPlus, LuX } from "react-icons/lu";
import { FlagIcon } from "@/components/flag-icon";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  type CountriesMode,
  type ProxyGeneratorOptions,
  searchCountries,
  toggleCountry,
} from "@/lib/proxy-generator-form";
import { cn } from "@/lib/utils";
import { SegmentedChoice } from "./segmented-choice";

const MODES: CountriesMode[] = ["any", "only", "exclude"];

interface Props {
  options: ProxyGeneratorOptions;
  mode: CountriesMode;
  countries: string[];
  disabled?: boolean;
  onChange: (mode: CountriesMode, countries: string[]) => void;
}

function useCountryName() {
  const { i18n } = useTranslation();
  return useMemo(() => {
    let names: Intl.DisplayNames | null = null;
    try {
      names = new Intl.DisplayNames([i18n.language], { type: "region" });
    } catch {
      names = null;
    }
    return (code: string) =>
      names?.of(code.toUpperCase()) ?? code.toUpperCase();
  }, [i18n.language]);
}

function CountryChip({
  code,
  name,
  picked,
  disabled,
  onToggle,
}: {
  code: string;
  name: string;
  picked: boolean;
  disabled?: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={picked}
      title={name}
      disabled={disabled}
      onClick={onToggle}
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-full px-2.5 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50",
        picked
          ? "bg-primary/15 text-foreground"
          : "bg-foreground/5 text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
      )}
    >
      <FlagIcon countryCode={code} className="rounded-[2px]" />
      {code.toUpperCase()}
      {picked && <LuX className="size-3" aria-hidden="true" />}
    </button>
  );
}

/** Exit countries: any, only these, or all but these. */
export function ProxyGeneratorCountries({
  options,
  mode,
  countries,
  disabled,
  onChange,
}: Props) {
  const { t } = useTranslation();
  const nameOf = useCountryName();
  const [open, setOpen] = useState(false);
  const listed = useMemo(
    () =>
      searchCountries(options.countries, options.popular_countries, "", nameOf),
    [options.countries, options.popular_countries, nameOf],
  );
  const chips = [
    ...options.popular_countries,
    ...countries.filter((c) => !options.popular_countries.includes(c)),
  ];
  const toggle = (code: string) =>
    onChange(mode, toggleCountry(countries, code));

  return (
    <div className="grid gap-2">
      <SegmentedChoice
        label={t("proxyGenerator.countries.label")}
        value={mode}
        choices={MODES.map((m) => ({
          value: m,
          label: t(`proxyGenerator.countries.modes.${m}`),
        }))}
        disabled={disabled}
        onChange={(m) => onChange(m, countries)}
      />
      {mode !== "any" && (
        <div className="flex flex-wrap gap-1.5">
          {chips.map((code) => (
            <CountryChip
              key={code}
              code={code}
              name={nameOf(code)}
              picked={countries.includes(code)}
              disabled={disabled}
              onToggle={() => toggle(code)}
            />
          ))}
          <Popover open={open} onOpenChange={setOpen}>
            <PopoverTrigger asChild>
              <button
                type="button"
                disabled={disabled}
                className="inline-flex h-7 items-center gap-1 rounded-full bg-foreground/5 px-2.5 text-xs text-muted-foreground hover:bg-foreground/10 hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
              >
                <LuPlus className="size-3" aria-hidden="true" />
                {t("proxyGenerator.countries.more")}
              </button>
            </PopoverTrigger>
            <PopoverContent className="w-64 p-0" align="start" sideOffset={6}>
              <Command>
                <CommandInput
                  placeholder={t("proxyGenerator.countries.search")}
                />
                <CommandList>
                  <CommandEmpty>
                    {t("proxyGenerator.countries.notFound")}
                  </CommandEmpty>
                  <CommandGroup>
                    {listed.map((code) => (
                      <CommandItem
                        key={code}
                        value={code}
                        keywords={[nameOf(code)]}
                        onSelect={() => toggle(code)}
                      >
                        <LuCheck
                          className={cn(
                            "size-4 shrink-0",
                            countries.includes(code)
                              ? "opacity-100"
                              : "opacity-0",
                          )}
                          aria-hidden="true"
                        />
                        <FlagIcon
                          countryCode={code}
                          className="rounded-[2px]"
                        />
                        <span className="truncate">{nameOf(code)}</span>
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
        </div>
      )}
    </div>
  );
}
