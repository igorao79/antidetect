"use client";

import { useTranslation } from "react-i18next";
import {
  AnimatedTabs,
  AnimatedTabsList,
  AnimatedTabsTrigger,
} from "@/components/ui/animated-tabs";

type ProfilesGroupsTab = "profiles" | "groups";

/**
 * The switch at the top of the profiles and groups pages, which share one rail
 * item (fork), drawn like the Network page's Proxies | VPN switch.
 */
export function ProfilesGroupsTabs({
  value,
  profileCount,
  groupCount,
  onSelect,
}: {
  value: ProfilesGroupsTab;
  profileCount: number;
  groupCount: number;
  onSelect: (tab: ProfilesGroupsTab) => void;
}) {
  const { t } = useTranslation();
  return (
    <AnimatedTabs
      value={value}
      onValueChange={(tab) => onSelect(tab as ProfilesGroupsTab)}
    >
      <AnimatedTabsList>
        <AnimatedTabsTrigger value="profiles">
          <span>{t("rail.profiles")}</span>
          <span className="text-xs tabular-nums">{profileCount}</span>
        </AnimatedTabsTrigger>
        <AnimatedTabsTrigger value="groups">
          <span>{t("rail.groups")}</span>
          <span className="text-xs tabular-nums">{groupCount}</span>
        </AnimatedTabsTrigger>
      </AnimatedTabsList>
    </AnimatedTabs>
  );
}
