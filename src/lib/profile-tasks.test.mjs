import assert from "node:assert/strict";
import test from "node:test";
import {
  dueAtFrom,
  overdueCounts,
  parseTimeField,
  splitDue,
  taskLevel,
  tasksToRemind,
} from "./profile-tasks.ts";

const task = (over) => ({
  id: "t",
  profile_id: "p",
  title: "x",
  due_at: null,
  done: false,
  reminded: false,
  created_at: 0,
  updated_at: 0,
  ...over,
});

test("a time is read from the forms people type", () => {
  assert.equal(parseTimeField("9"), "09:00");
  assert.equal(parseTimeField("930"), "09:30");
  assert.equal(parseTimeField("9:30"), "09:30");
  assert.equal(parseTimeField("21:05"), "21:05");
  assert.equal(parseTimeField("24:00"), null);
  assert.equal(parseTimeField("12:60"), null);
  assert.equal(parseTimeField("abc"), null);
  assert.equal(parseTimeField(""), null);
});

test("a due time round-trips through the date and time fields", () => {
  const due = dueAtFrom("2026-11-14", "18:45");
  assert.equal(new Date(due * 1000).getHours(), 18);
  assert.deepEqual(splitDue(due), { date: "2026-11-14", time: "18:45" });
  assert.deepEqual(splitDue(null), { date: "", time: "" });
  assert.equal(dueAtFrom("2026-02-31", "10:00"), null);
  assert.equal(dueAtFrom("", "10:00"), null);
  assert.equal(dueAtFrom("2026-11-14", "99"), null);
});

test("a task is overdue, soon, upcoming or neither", () => {
  const now = 1_000_000;
  assert.equal(taskLevel(task({}), now), "none");
  assert.equal(taskLevel(task({ due_at: now - 1, done: true }), now), "none");
  assert.equal(taskLevel(task({ due_at: now }), now), "overdue");
  assert.equal(taskLevel(task({ due_at: now + 3600 }), now), "soon");
  assert.equal(taskLevel(task({ due_at: now + 3 * 86400 }), now), "upcoming");
});

test("only open, unreminded, passed tasks are reminded", () => {
  const now = 100;
  const tasks = [
    task({ id: "a", due_at: 50 }),
    task({ id: "b", due_at: 50, reminded: true }),
    task({ id: "c", due_at: 50, done: true }),
    task({ id: "d", due_at: 500 }),
    task({ id: "e" }),
  ];
  assert.deepEqual(
    tasksToRemind(tasks, now).map((t) => t.id),
    ["a"],
  );
});

test("overdue tasks are counted per profile", () => {
  const now = 100;
  const counts = overdueCounts(
    [
      task({ profile_id: "p1", due_at: 1 }),
      task({ profile_id: "p1", due_at: 2 }),
      task({ profile_id: "p2", due_at: 3, done: true }),
      task({ profile_id: "p2", due_at: 900 }),
    ],
    now,
  );
  assert.equal(counts.get("p1"), 2);
  assert.equal(counts.has("p2"), false);
});
