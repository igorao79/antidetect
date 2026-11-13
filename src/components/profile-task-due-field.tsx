"use client";

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuCalendarDays } from "react-icons/lu";
import { ProxyExpiryCalendar } from "@/components/proxy-expiry-calendar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { DEFAULT_TASK_TIME, parseTimeField } from "@/lib/profile-tasks";
import {
  expiryExample,
  formatDateField,
  maskDateDigits,
  parseDateField,
} from "@/lib/proxy-expiry";

export interface TaskDue {
  /** `YYYY-MM-DD`, or "" for none. */
  date: string;
  /** `HH:MM`, or "" for none. */
  time: string;
}

/**
 * A task's optional due date and time: the date typed in the locale's order
 * or picked in the app's calendar, and a time typed as `9`, `930` or `9:30`.
 * A date alone gets 09:00.
 */
export function ProfileTaskDueField({
  value,
  onChange,
  disabled,
  idPrefix,
  onInvalidChange,
}: {
  value: TaskDue;
  onChange: (value: TaskDue) => void;
  disabled?: boolean;
  idPrefix: string;
  onInvalidChange?: (invalid: boolean) => void;
}) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const [dateDraft, setDateDraft] = useState<string | null>(null);
  const [timeDraft, setTimeDraft] = useState<string | null>(null);
  const [dateInvalid, setDateInvalid] = useState(false);
  const [timeInvalid, setTimeInvalid] = useState(false);
  const [calendarOpen, setCalendarOpen] = useState(false);

  const invalid = dateInvalid || timeInvalid;
  useEffect(() => {
    onInvalidChange?.(invalid);
  }, [invalid, onInvalidChange]);

  const pickDate = (date: string) => {
    setDateDraft(null);
    setDateInvalid(false);
    onChange({ date, time: date ? value.time || DEFAULT_TASK_TIME : "" });
  };

  const commitDate = () => {
    if (dateDraft === null) return;
    const parsed = parseDateField(dateDraft, locale);
    if (parsed === null) {
      setDateInvalid(true);
      return;
    }
    pickDate(parsed);
  };

  const commitTime = () => {
    if (timeDraft === null) return;
    const parsed =
      timeDraft.trim() === ""
        ? value.date
          ? DEFAULT_TASK_TIME
          : ""
        : parseTimeField(timeDraft);
    if (parsed === null) {
      setTimeInvalid(true);
      return;
    }
    setTimeDraft(null);
    setTimeInvalid(false);
    onChange({ date: value.date, time: parsed });
  };

  const commitOnEnter = (commit: () => void) => (e: React.KeyboardEvent) => {
    if (e.key === "Enter") commit();
  };

  return (
    <div className="flex items-start gap-2">
      <Popover open={calendarOpen} onOpenChange={setCalendarOpen}>
        <PopoverAnchor asChild>
          <div className="relative min-w-0 flex-1">
            <Input
              id={`${idPrefix}-date`}
              inputMode="numeric"
              autoComplete="off"
              spellCheck={false}
              value={dateDraft ?? formatDateField(value.date, locale)}
              placeholder={expiryExample(locale)}
              aria-label={t("profileTasks.dueDate")}
              aria-invalid={dateInvalid || undefined}
              disabled={disabled}
              className="pr-9 tabular-nums"
              onChange={(e) => {
                setDateDraft(maskDateDigits(e.target.value, locale));
                setDateInvalid(false);
              }}
              onBlur={commitDate}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown" && e.altKey) {
                  e.preventDefault();
                  setCalendarOpen(true);
                } else {
                  commitOnEnter(commitDate)(e);
                }
              }}
            />
            <PopoverTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                disabled={disabled}
                aria-label={t("proxies.expiry.openCalendar")}
                className="absolute top-1/2 right-1 size-7 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                <LuCalendarDays aria-hidden="true" />
              </Button>
            </PopoverTrigger>
          </div>
        </PopoverAnchor>
        <PopoverContent
          align="start"
          className="w-auto p-3"
          onOpenAutoFocus={(e) => {
            e.preventDefault();
          }}
        >
          <ProxyExpiryCalendar
            value={value.date}
            onSelect={(next) => {
              pickDate(next);
              setCalendarOpen(false);
            }}
          />
        </PopoverContent>
      </Popover>
      <Input
        id={`${idPrefix}-time`}
        inputMode="numeric"
        autoComplete="off"
        spellCheck={false}
        value={timeDraft ?? value.time}
        placeholder={DEFAULT_TASK_TIME}
        aria-label={t("profileTasks.dueTime")}
        aria-invalid={timeInvalid || undefined}
        disabled={disabled || (!value.date && timeDraft === null)}
        className="w-20 shrink-0 tabular-nums"
        onChange={(e) => {
          setTimeDraft(e.target.value);
          setTimeInvalid(false);
        }}
        onBlur={commitTime}
        onKeyDown={commitOnEnter(commitTime)}
      />
    </div>
  );
}
