"use client";

import { useTranslation } from "react-i18next";
import { TickingCount } from "@/components/entity-page";
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
        <AnimatedTabsTrigger value="profiles" data-summary="profiles">
          <span>{t("rail.profiles")}</span>
          <TickingCount value={profileCount} className="text-xs" />
        </AnimatedTabsTrigger>
        <AnimatedTabsTrigger value="groups" data-summary="groups">
          <span data-slot="group-summary-title">{t("rail.groups")}</span>
          <TickingCount
            slot="group-summary-count"
            value={groupCount}
            className="text-xs"
          />
        </AnimatedTabsTrigger>
      </AnimatedTabsList>
    </AnimatedTabs>
  );
}
