"use client";

import {
  type KeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { LuChevronLeft, LuChevronRight } from "react-icons/lu";
import { Button } from "@/components/ui/button";
import {
  calendarDays,
  shiftDateInput,
  shiftDateInputMonths,
  toDateInputValue,
  weekStart,
} from "@/lib/proxy-expiry";
import { cn } from "@/lib/utils";

const DAYS_IN_WEEK = 7;

/** Where the arrow keys move the focused day. */
const KEY_STEPS: Record<string, (value: string) => string> = {
  ArrowLeft: (value) => shiftDateInput(value, -1),
  ArrowRight: (value) => shiftDateInput(value, 1),
  ArrowUp: (value) => shiftDateInput(value, -DAYS_IN_WEEK),
  ArrowDown: (value) => shiftDateInput(value, DAYS_IN_WEEK),
  PageUp: (value) => shiftDateInputMonths(value, -1),
  PageDown: (value) => shiftDateInputMonths(value, 1),
};

const monthOf = (value: string) => {
  const [year, month] = value.split("-").map(Number);
  return { year, month: month - 1 };
};

const dateOf = (value: string) => {
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
};

function capitalize(text: string, locale: string): string {
  return text.charAt(0).toLocaleUpperCase(locale) + text.slice(1);
}

/**
 * A month grid for the proxy expiry: the locale's weekdays and first day,
 * today ringed, the picked day filled. Arrow keys move by day and week,
 * Page Up/Down by month, Enter or Space picks.
 */
export function ProxyExpiryCalendar({
  value,
  onSelect,
}: {
  /** `YYYY-MM-DD`, or "" when nothing is picked yet. */
  value: string;
  onSelect: (value: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const today = toDateInputValue(new Date());
  const [focused, setFocused] = useState(value || today);
  const [shown, setShown] = useState(() => monthOf(value || today));
  const grid = useRef<HTMLDivElement>(null);
  // The focused day takes the keyboard focus on opening and when the keys
  // move it, never when a month button is clicked.
  const moved = useRef(true);

  const firstWeekday = useMemo(() => weekStart(locale), [locale]);
  const days = calendarDays(shown.year, shown.month, firstWeekday);
  const weekdays = useMemo(() => {
    const format = new Intl.DateTimeFormat(locale, { weekday: "short" });
    return days
      .slice(0, DAYS_IN_WEEK)
      .map((day) => format.format(dateOf(day.value)));
  }, [days, locale]);
  const title = capitalize(
    new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(
      new Date(shown.year, shown.month, 1),
    ),
    locale,
  );
  const dayLabel = new Intl.DateTimeFormat(locale, { dateStyle: "long" });

  useEffect(() => {
    if (!moved.current) return;
    moved.current = false;
    // A frame later: the popover places itself before it can take focus.
    const frame = requestAnimationFrame(() => {
      grid.current
        ?.querySelector<HTMLButtonElement>(`[data-day="${focused}"]`)
        ?.focus();
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [focused]);

  const focusDay = (next: string) => {
    moved.current = true;
    setFocused(next);
    const month = monthOf(next);
    if (month.year !== shown.year || month.month !== shown.month) {
      setShown(month);
    }
  };

  const turnMonth = (months: number) => {
    const next = shiftDateInputMonths(focused, months);
    setFocused(next);
    setShown(monthOf(next));
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      onSelect(focused);
      return;
    }
    const step = KEY_STEPS[event.key];
    if (!step) return;
    event.preventDefault();
    focusDay(step(focused));
  };

  return (
    <div className="grid gap-2" data-testid="proxy-expiry-calendar">
      <div className="flex items-center justify-between gap-2">
        <Button
          type="button"
          variant="subtle"
          size="icon"
          className="size-7"
          onClick={() => {
            turnMonth(-1);
          }}
          aria-label={t("proxies.expiry.prevMonth")}
        >
          <LuChevronLeft aria-hidden="true" />
        </Button>
        <p className="text-sm font-medium" aria-live="polite">
          {title}
        </p>
        <Button
          type="button"
          variant="subtle"
          size="icon"
          className="size-7"
          onClick={() => {
            turnMonth(1);
          }}
          aria-label={t("proxies.expiry.nextMonth")}
        >
          <LuChevronRight aria-hidden="true" />
        </Button>
      </div>
      <div
        ref={grid}
        role="group"
        aria-label={title}
        onKeyDown={handleKeyDown}
        className="grid grid-cols-7 gap-0.5"
      >
        {weekdays.map((weekday) => (
          <span
            key={weekday}
            aria-hidden="true"
            className="grid h-7 place-items-center text-[11px] text-muted-foreground"
          >
            {weekday}
          </span>
        ))}
        {days.map((day) => {
          const picked = day.value === value;
          const isToday = day.value === today;
          return (
            <button
              key={day.value}
              type="button"
              data-day={day.value}
              tabIndex={day.value === focused ? 0 : -1}
              aria-pressed={picked}
              aria-current={isToday ? "date" : undefined}
              aria-label={dayLabel.format(dateOf(day.value))}
              onClick={() => {
                onSelect(day.value);
              }}
              onFocus={() => {
                setFocused(day.value);
              }}
              className={cn(
                "grid size-8 cursor-pointer place-items-center rounded-md text-sm tabular-nums transition-colors duration-100 outline-none focus-visible:ring-2 focus-visible:ring-ring",
                picked
                  ? "bg-primary text-primary-foreground"
                  : "hover:bg-foreground/8",
                !picked && !day.inMonth && "text-muted-foreground/45",
                !picked &&
                  day.inMonth &&
                  day.value < today &&
                  "text-muted-foreground",
                !picked && isToday && "ring-1 ring-primary/60 ring-inset",
              )}
            >
              {day.day}
            </button>
          );
        })}
      </div>
    </div>
  );
}
