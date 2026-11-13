"use client";

import { useTranslation } from "react-i18next";
import { LuCopy, LuEllipsis, LuReplace, LuUsers } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatProxyUri } from "@/lib/proxy-uri";
import { showErrorToast, showSuccessToast } from "@/lib/toast-utils";
import type { StoredProxy } from "@/types";

interface ProxyRowMenuProps {
  proxy: StoredProxy;
  onReplace: (proxy: StoredProxy) => void;
  onAssign: (proxy: StoredProxy) => void;
}

/** The Network page's less common proxy actions, behind one row button. */
export function ProxyRowMenu({
  proxy,
  onReplace,
  onAssign,
}: ProxyRowMenuProps) {
  const { t } = useTranslation();

  const copyProxy = async () => {
    try {
      await navigator.clipboard.writeText(formatProxyUri(proxy.proxy_settings));
      showSuccessToast(t("proxies.actions.copied"));
    } catch {
      showErrorToast(t("proxies.actions.copyFailed"));
    }
  };

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="subtle"
              size="icon"
              aria-label={t("proxies.actions.more", { name: proxy.name })}
              className="row-reveal size-7 rounded-md [&_svg]:size-3.5"
            >
              <LuEllipsis />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>
          <p>{t("proxies.actions.moreTooltip")}</p>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => void copyProxy()}>
          <LuCopy />
          {t("proxies.actions.copyProxy")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onReplace(proxy)}>
          <LuReplace />
          {t("proxies.actions.replaceInProfiles")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => onAssign(proxy)}>
          <LuUsers />
          {t("proxies.actions.assignToProfiles")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
