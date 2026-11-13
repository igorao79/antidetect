"use client";

import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { holderInitials } from "./lock-holder";

/** Who has a shared profile open on another machine, as their initials. */
export function LockHolderChip({
  holder,
  tooltip,
}: {
  holder: string | undefined;
  tooltip: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          data-slot="profile-lock-holder"
          role="img"
          aria-label={tooltip}
          className="grid size-5 shrink-0 place-items-center rounded-full bg-primary/10 text-[10px] leading-none font-medium text-foreground"
        >
          {holderInitials(holder)}
        </span>
      </TooltipTrigger>
      <TooltipContent>{tooltip}</TooltipContent>
    </Tooltip>
  );
}
