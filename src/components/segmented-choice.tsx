"use client";

import { cn } from "@/lib/utils";

interface Choice<T extends string> {
  value: T;
  label: string;
}

interface Props<T extends string> {
  label: string;
  value: T;
  choices: Choice<T>[];
  disabled?: boolean;
  onChange: (value: T) => void;
}

/** A row of mutually exclusive choices drawn as one segmented control. */
export function SegmentedChoice<T extends string>({
  label,
  value,
  choices,
  disabled,
  onChange,
}: Props<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex w-fit max-w-full flex-wrap rounded-lg bg-foreground/5 p-0.5"
    >
      {choices.map((choice) => (
        <button
          key={choice.value}
          type="button"
          role="radio"
          aria-checked={value === choice.value}
          disabled={disabled}
          onClick={() => onChange(choice.value)}
          className={cn(
            "h-7 rounded-md px-3 text-xs transition-colors focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50",
            value === choice.value
              ? "bg-background text-foreground shadow-xs"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {choice.label}
        </button>
      ))}
    </div>
  );
}
