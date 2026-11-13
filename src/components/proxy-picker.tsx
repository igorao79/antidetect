"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";
import { LuCheck, LuChevronsUpDown, LuPlus } from "react-icons/lu";
import {
  checkHighlightedProxy,
  ProxyChoiceLabel,
} from "@/components/proxy-choice-label";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { orderProxyChoices } from "@/lib/proxy-choices";
import { cn } from "@/lib/utils";
import type { StoredProxy } from "@/types";

interface ProxyPickerProps {
  proxies: StoredProxy[];
  /** The assigned proxy, or null for none. It is listed first. */
  value: string | null;
  disabled?: boolean;
  ariaLabel: string;
  onSelect: (proxyId: string | null) => void;
  onAdd: () => void;
}

/** A searchable proxy list with the current route first and a Check action per row. */
export function ProxyPicker({
  proxies,
  value,
  disabled,
  ariaLabel,
  onSelect,
  onAdd,
}: ProxyPickerProps) {
  const { t } = useTranslation();
  const listboxId = React.useId();
  const [open, setOpen] = React.useState(false);
  const ordered = React.useMemo(
    () => orderProxyChoices(proxies, value),
    [proxies, value],
  );
  const selected = proxies.find((proxy) => proxy.id === value);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          role="combobox"
          aria-label={ariaLabel}
          aria-expanded={open}
          aria-controls={listboxId}
          disabled={disabled}
          className="flex h-8 min-w-0 flex-1 items-center justify-between gap-2 rounded-md bg-foreground/5 px-3 text-left text-xs outline-none transition-colors hover:bg-foreground/10 focus-visible:ring-2 focus-visible:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span className="truncate">
            {selected?.name ?? t("profileInfo.values.none")}
          </span>
          <LuChevronsUpDown
            className="size-3.5 shrink-0 opacity-50"
            aria-hidden="true"
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        id={listboxId}
        className="w-(--radix-popover-trigger-width) min-w-72 p-0"
        align="start"
        sideOffset={6}
      >
        <Command onKeyDown={checkHighlightedProxy}>
          <CommandInput placeholder={t("createProfile.proxy.search")} />
          <CommandList>
            <CommandEmpty>{t("createProfile.proxy.notFound")}</CommandEmpty>
            <CommandGroup>
              <CommandItem
                value="__none__"
                keywords={[t("profileInfo.values.none")]}
                onSelect={() => {
                  onSelect(null);
                  setOpen(false);
                }}
              >
                <LuCheck
                  className={cn(
                    "size-4 shrink-0",
                    value === null ? "opacity-100" : "opacity-0",
                  )}
                  aria-hidden="true"
                />
                {t("profileInfo.values.none")}
              </CommandItem>
              {ordered.map((proxy) => (
                <CommandItem
                  aria-label={proxy.name}
                  key={proxy.id}
                  value={proxy.id}
                  keywords={[proxy.name]}
                  onSelect={() => {
                    onSelect(proxy.id);
                    setOpen(false);
                  }}
                >
                  <LuCheck
                    className={cn(
                      "size-4 shrink-0",
                      value === proxy.id ? "opacity-100" : "opacity-0",
                    )}
                    aria-hidden="true"
                  />
                  <ProxyChoiceLabel
                    proxy={proxy}
                    current={value === proxy.id}
                  />
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandSeparator />
            <CommandGroup forceMount>
              <CommandItem
                forceMount
                value="__add_proxy__"
                onSelect={() => {
                  setOpen(false);
                  onAdd();
                }}
              >
                <LuPlus className="size-4 shrink-0" aria-hidden="true" />
                {t("createProfile.proxy.addProxy")}
              </CommandItem>
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
