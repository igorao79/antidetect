"use client";

import { useTranslation } from "react-i18next";
import { LuSparkles } from "react-icons/lu";
import { CommandItem } from "@/components/ui/command";

/** The proxy pickers' "Generate proxy" entry, under "Add proxy" (fork). */
export function GenerateProxyCommandItem({
  onSelect,
}: {
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  return (
    <CommandItem value="__generate_proxy__" forceMount onSelect={onSelect}>
      <LuSparkles className="size-4 shrink-0" aria-hidden="true" />
      {t("proxyGenerator.pickerItem")}
    </CommandItem>
  );
}
