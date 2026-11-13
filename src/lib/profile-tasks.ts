export interface ProfileTask {
  id: string;
  profile_id: string;
  title: string;
  /** Unix seconds; null for a task with no due time. */
  due_at: number | null;
  done: boolean;
  reminded: boolean;
  created_at: number;
  updated_at: number;
}

/** Fired on `window` whenever tasks were added, edited or reminded. */
export const PROFILE_TASKS_CHANGED_EVENT = "owl:profile-tasks-changed";

export type TaskLevel = "none" | "upcoming" | "soon" | "overdue";

/** A task closer than this is "soon". */
export const TASK_SOON_SECONDS = 24 * 60 * 60;

/** The time a date gets when none was typed. */
export const DEFAULT_TASK_TIME = "09:00";

const TIME_FIELD = /^(\d{1,2})(?::?(\d{2}))?$/;
const DATE_INPUT = /^(\d{4})-(\d{2})-(\d{2})$/;

const pad = (value: number): string => String(value).padStart(2, "0");

/** `YYYY-MM-DD` of a local date, the value the app's calendar takes. */
export function toDateInputValue(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** `HH:MM` from what was typed ("9", "930", "9:30", "21:05"); null if not a time. */
export function parseTimeField(text: string): string | null {
  const match = TIME_FIELD.exec(text.trim());
  if (!match) return null;
  const compact = match[2] === undefined && match[1].length > 2;
  const hours = compact ? Number(match[1].slice(0, -2)) : Number(match[1]);
  const minutes = compact
    ? Number(match[1].slice(-2))
    : match[2] === undefined
      ? 0
      : Number(match[2]);
  if (hours > 23 || minutes > 59) return null;
  return `${pad(hours)}:${pad(minutes)}`;
}

/** Unix seconds of a local `YYYY-MM-DD` date at `HH:MM`; null when invalid. */
export function dueAtFrom(dateInput: string, time: string): number | null {
  const date = DATE_INPUT.exec(dateInput.trim());
  const clock = parseTimeField(time);
  if (!date || !clock) return null;
  const [year, month, day] = date.slice(1).map(Number);
  const [hours, minutes] = clock.split(":").map(Number);
  const moment = new Date(year, month - 1, day, hours, minutes, 0);
  if (moment.getMonth() !== month - 1 || moment.getDate() !== day) return null;
  return Math.floor(moment.getTime() / 1000);
}

/** The date and time fields' values for a stored due time. */
export function splitDue(dueAt: number | null): { date: string; time: string } {
  if (dueAt === null) return { date: "", time: "" };
  const moment = new Date(dueAt * 1000);
  return {
    date: toDateInputValue(moment),
    time: `${pad(moment.getHours())}:${pad(moment.getMinutes())}`,
  };
}

/** Where a task stands against its due time; finished or undated ones are "none". */
export function taskLevel(
  task: Pick<ProfileTask, "due_at" | "done">,
  nowSeconds: number,
): TaskLevel {
  if (task.done || task.due_at === null) return "none";
  if (task.due_at <= nowSeconds) return "overdue";
  return task.due_at - nowSeconds <= TASK_SOON_SECONDS ? "soon" : "upcoming";
}

/** Open tasks whose due time has passed and whose reminder was not shown. */
export function tasksToRemind(
  tasks: readonly ProfileTask[],
  nowSeconds: number,
): ProfileTask[] {
  return tasks.filter(
    (task) =>
      !task.done &&
      !task.reminded &&
      task.due_at !== null &&
      task.due_at <= nowSeconds,
  );
}

/** How many open tasks of each profile are due or overdue. */
export function overdueCounts(
  tasks: readonly ProfileTask[],
  nowSeconds: number,
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const task of tasks) {
    if (taskLevel(task, nowSeconds) !== "overdue") continue;
    counts.set(task.profile_id, (counts.get(task.profile_id) ?? 0) + 1);
  }
  return counts;
}

/** Today's `YYYY-MM-DD`, for the due date field's default. */
export function todayInput(now = new Date()): string {
  return toDateInputValue(now);
}
