"use client";

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { LuListTodo, LuPencil, LuPlus, LuTrash2 } from "react-icons/lu";
import { SectionHeader } from "@/components/profile-info/draft-save-bar";
import {
  ProfileTaskDueField,
  type TaskDue,
} from "@/components/profile-task-due-field";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { translateBackendError } from "@/lib/backend-errors";
import {
  DEFAULT_TASK_TIME,
  dueAtFrom,
  PROFILE_TASKS_CHANGED_EVENT,
  type ProfileTask,
  splitDue,
  type TaskLevel,
  taskLevel,
} from "@/lib/profile-tasks";
import { cn } from "@/lib/utils";

const MAX_TITLE_CHARS = 200;
const NO_DUE: TaskDue = { date: "", time: "" };

const LEVEL_TEXT: Record<TaskLevel, string> = {
  none: "text-muted-foreground",
  upcoming: "text-muted-foreground",
  soon: "text-warning-text",
  overdue: "text-destructive-text",
};

function notifyChanged() {
  window.dispatchEvent(new Event(PROFILE_TASKS_CHANGED_EVENT));
}

/** Title and due time fields, for a new task and for editing one. */
function TaskForm({
  idPrefix,
  initialTitle = "",
  initialDue = NO_DUE,
  submitLabel,
  onSubmit,
  onCancel,
  disabled,
}: {
  idPrefix: string;
  initialTitle?: string;
  initialDue?: TaskDue;
  submitLabel: string;
  onSubmit: (title: string, dueAt: number | null) => Promise<boolean>;
  onCancel?: () => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const [title, setTitle] = useState(initialTitle);
  const [due, setDue] = useState<TaskDue>(initialDue);
  const [isBusy, setIsBusy] = useState(false);
  const [dueInvalid, setDueInvalid] = useState(false);
  const canSubmit =
    title.trim().length > 0 && !disabled && !isBusy && !dueInvalid;

  const submit = async () => {
    if (!canSubmit) return;
    setIsBusy(true);
    const dueAt = due.date
      ? dueAtFrom(due.date, due.time || DEFAULT_TASK_TIME)
      : null;
    const ok = await onSubmit(title, dueAt);
    setIsBusy(false);
    if (ok) {
      setTitle("");
      setDue(NO_DUE);
    }
  };

  return (
    <div className="flex flex-col gap-2" data-slot="profile-task-form">
      <Input
        id={`${idPrefix}-title`}
        value={title}
        maxLength={MAX_TITLE_CHARS}
        placeholder={t("profileTasks.titlePlaceholder")}
        aria-label={t("profileTasks.titleLabel")}
        disabled={disabled}
        onChange={(e) => {
          setTitle(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onCancel?.();
          }
        }}
      />
      <ProfileTaskDueField
        idPrefix={idPrefix}
        value={due}
        onChange={setDue}
        onInvalidChange={setDueInvalid}
        disabled={disabled}
      />
      <div className="flex items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={!canSubmit}
          onClick={() => void submit()}
        >
          {!onCancel && <LuPlus aria-hidden="true" />}
          {submitLabel}
        </Button>
        {onCancel && (
          <Button type="button" size="sm" variant="subtle" onClick={onCancel}>
            {t("common.buttons.cancel")}
          </Button>
        )}
      </div>
    </div>
  );
}

function DueText({ task, now }: { task: ProfileTask; now: number }) {
  const { t, i18n } = useTranslation();
  if (task.due_at === null) return null;
  const level = taskLevel(task, now);
  const when = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(task.due_at * 1000));
  return (
    <span
      data-task-level={level}
      className={cn(
        "text-xs tabular-nums",
        task.done ? "text-muted-foreground" : LEVEL_TEXT[level],
      )}
    >
      {level === "overdue"
        ? t("profileTasks.overdue", { when })
        : t("profileTasks.dueAt", { when })}
    </span>
  );
}

/** The profile's to-dos: add, tick off, edit and delete, kept on this machine. */
export function ProfileTasksSection({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const formId = useId();
  const [tasks, setTasks] = useState<ProfileTask[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  const load = useCallback(async () => {
    try {
      setTasks(
        await invoke<ProfileTask[]>("list_profile_tasks", { profileId }),
      );
      setNow(Math.floor(Date.now() / 1000));
    } catch (e) {
      setError(translateBackendError(t, e));
    }
  }, [profileId, t]);

  useEffect(() => {
    void load();
    const refresh = () => {
      void load();
    };
    window.addEventListener(PROFILE_TASKS_CHANGED_EVENT, refresh);
    const timer = window.setInterval(refresh, 60_000);
    return () => {
      window.removeEventListener(PROFILE_TASKS_CHANGED_EVENT, refresh);
      window.clearInterval(timer);
    };
  }, [load]);

  /** Runs a change, then reloads; false (with the error shown) when it fails. */
  const run = async (change: () => Promise<unknown>): Promise<boolean> => {
    setError(null);
    try {
      await change();
      notifyChanged();
      return true;
    } catch (e) {
      setError(translateBackendError(t, e));
      return false;
    }
  };

  const add = (title: string, dueAt: number | null) =>
    run(() => invoke("add_profile_task", { profileId, title, dueAt }));

  const edit = async (
    task: ProfileTask,
    title: string,
    dueAt: number | null,
  ) => {
    const ok = await run(() =>
      invoke("update_profile_task", {
        taskId: task.id,
        patch: dueAt === null ? { title, clearDue: true } : { title, dueAt },
      }),
    );
    if (ok) setEditingId(null);
    return ok;
  };

  const tick = (task: ProfileTask, done: boolean) =>
    run(() =>
      invoke("update_profile_task", { taskId: task.id, patch: { done } }),
    );

  const remove = (task: ProfileTask) =>
    run(() => invoke("delete_profile_task", { taskId: task.id }));

  const openCount = tasks?.filter((task) => !task.done).length ?? 0;

  return (
    <div className="flex flex-col gap-3" data-slot="profile-tasks">
      <SectionHeader
        icon={<LuListTodo className="size-4" />}
        title={t("profileInfo.sections.tasks")}
      />
      <p className="text-xs text-muted-foreground">
        {t("profileTasks.description")}
      </p>
      <TaskForm
        idPrefix={formId}
        submitLabel={t("profileTasks.add")}
        onSubmit={add}
      />
      {error && (
        <p role="alert" className="text-xs text-destructive-text">
          {error}
        </p>
      )}
      {tasks !== null && tasks.length === 0 && (
        <p className="text-xs text-muted-foreground">
          {t("profileTasks.empty")}
        </p>
      )}
      {tasks !== null && tasks.length > 0 && (
        <>
          <p className="text-xs text-muted-foreground tabular-nums">
            {t("profileTasks.openCount", { count: openCount })}
          </p>
          <ul className="flex flex-col gap-1.5">
            {tasks.map((task) => (
              <li
                key={task.id}
                data-slot="profile-task"
                data-done={task.done || undefined}
                className="rounded-lg bg-foreground/5 px-3 py-2"
              >
                {editingId === task.id ? (
                  <TaskForm
                    idPrefix={`${formId}-${task.id}`}
                    initialTitle={task.title}
                    initialDue={splitDue(task.due_at)}
                    submitLabel={t("common.buttons.save")}
                    onSubmit={(title, dueAt) => edit(task, title, dueAt)}
                    onCancel={() => {
                      setEditingId(null);
                    }}
                  />
                ) : (
                  <div className="flex items-start gap-2.5">
                    <Checkbox
                      className="mt-0.5"
                      checked={task.done}
                      aria-label={t("profileTasks.markDone", {
                        title: task.title,
                      })}
                      onCheckedChange={(checked) => {
                        void tick(task, checked === true);
                      }}
                    />
                    <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span
                        className={cn(
                          "text-sm break-words",
                          task.done && "text-muted-foreground line-through",
                        )}
                      >
                        {task.title}
                      </span>
                      <DueText task={task} now={now} />
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="size-7 text-muted-foreground hover:text-foreground"
                      aria-label={t("profileTasks.edit", { title: task.title })}
                      onClick={() => {
                        setEditingId(task.id);
                      }}
                    >
                      <LuPencil aria-hidden="true" />
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="size-7 text-muted-foreground hover:text-destructive-text"
                      aria-label={t("profileTasks.delete", {
                        title: task.title,
                      })}
                      onClick={() => {
                        void remove(task);
                      }}
                    >
                      <LuTrash2 aria-hidden="true" />
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
