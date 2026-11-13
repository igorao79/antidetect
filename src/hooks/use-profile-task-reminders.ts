"use client";

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  PROFILE_TASKS_CHANGED_EVENT,
  type ProfileTask,
  tasksToRemind,
} from "@/lib/profile-tasks";
import { notifySystem } from "@/lib/system-notify";
import { showToast } from "@/lib/toast-utils";
import type { BrowserProfile } from "@/types";

const CHECK_INTERVAL_MS = 30_000;
const LISTED_TITLES = 3;

/**
 * Reminds the person when a profile task's due time passes while the app
 * runs: a toast, and a system notification while the window is behind.
 * Tasks that came due while the app was closed are reminded at the next start.
 * Each reminder is shown once; the backend remembers it.
 */
export function useProfileTaskReminders(
  profiles: readonly BrowserProfile[],
): void {
  const { t } = useTranslation();
  const latest = useRef({ t, profiles });
  latest.current = { t, profiles };

  useEffect(() => {
    let disposed = false;
    let checking = false;

    const check = async () => {
      if (checking) return;
      checking = true;
      try {
        const known = new Set(latest.current.profiles.map((p) => p.id));
        if (known.size === 0) return;
        const tasks = await invoke<ProfileTask[]>("list_profile_tasks", {});
        const due = tasksToRemind(
          tasks.filter((task) => known.has(task.profile_id)),
          Math.floor(Date.now() / 1000),
        );
        if (disposed || due.length === 0) return;
        await invoke("mark_profile_tasks_reminded", {
          taskIds: due.map((task) => task.id),
        });
        window.dispatchEvent(new Event(PROFILE_TASKS_CHANGED_EVENT));
        announce(due);
      } catch (error) {
        console.warn("Profile task reminders failed:", error);
      } finally {
        checking = false;
      }
    };

    const announce = (due: ProfileTask[]) => {
      const { t: translate, profiles: current } = latest.current;
      const nameOf = (task: ProfileTask) =>
        current.find((p) => p.id === task.profile_id)?.name ?? "";
      const title =
        due.length === 1
          ? translate("profileTasks.reminderTitle", { title: due[0].title })
          : translate("profileTasks.reminderTitleMany", { count: due.length });
      const body =
        due.length === 1
          ? nameOf(due[0])
          : due
              .slice(0, LISTED_TITLES)
              .map((task) => `${nameOf(task)}: ${task.title}`)
              .join("\n");
      showToast({ type: "info", title, description: body });
      void notifySystem({ title, body });
    };

    const onChanged = () => {
      void check();
    };
    void check();
    const timer = window.setInterval(onChanged, CHECK_INTERVAL_MS);
    window.addEventListener(PROFILE_TASKS_CHANGED_EVENT, onChanged);
    return () => {
      disposed = true;
      window.clearInterval(timer);
      window.removeEventListener(PROFILE_TASKS_CHANGED_EVENT, onChanged);
    };
  }, []);

  // A first look once the profiles have loaded.
  const loaded = profiles.length > 0;
  useEffect(() => {
    if (loaded) window.dispatchEvent(new Event(PROFILE_TASKS_CHANGED_EVENT));
  }, [loaded]);
}
