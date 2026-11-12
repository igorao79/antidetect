import assert from "node:assert/strict";
import test from "node:test";
import {
  calendarDays,
  dateInputDaysFromToday,
  dateInputFromExpiry,
  expiryFromDateInput,
  formatDateField,
  formatExpiryDate,
  PROXY_EXPIRY_PRESET_DAYS,
  parseDateField,
  proxyExpiryStatus,
  shiftDateInput,
  shiftDateInputMonths,
  weekStart,
} from "./proxy-expiry.ts";

const NOON = new Date(2026, 10, 12, 12, 0, 0);
const endOf = (year, month, day) =>
  Math.floor(new Date(year, month, day, 23, 59, 59).getTime() / 1000);

test("a proxy without a date has no status", () => {
  assert.equal(proxyExpiryStatus(undefined, NOON), null);
  assert.equal(proxyExpiryStatus(null, NOON), null);
  assert.equal(proxyExpiryStatus(Number.NaN, NOON), null);
});

test("a date far ahead is fine", () => {
  const status = proxyExpiryStatus(endOf(2026, 11, 12), NOON);
  assert.equal(status.level, "ok");
  assert.equal(status.daysLeft, 30);
});

test("three days or fewer is a warning, today included", () => {
  assert.equal(proxyExpiryStatus(endOf(2026, 10, 15), NOON).level, "soon");
  assert.equal(proxyExpiryStatus(endOf(2026, 10, 15), NOON).daysLeft, 3);
  assert.equal(proxyExpiryStatus(endOf(2026, 10, 16), NOON).level, "ok");
  const today = proxyExpiryStatus(endOf(2026, 10, 12), NOON);
  assert.equal(today.level, "soon");
  assert.equal(today.daysLeft, 0);
});

test("a passed date is expired with negative days", () => {
  const status = proxyExpiryStatus(endOf(2026, 10, 10), NOON);
  assert.equal(status.level, "expired");
  assert.equal(status.daysLeft, -2);
});

test("the date input round-trips to the end of the picked day", () => {
  const at = expiryFromDateInput("2026-12-01");
  assert.equal(at, endOf(2026, 11, 1));
  assert.equal(dateInputFromExpiry(at), "2026-12-01");
});

test("a value that is not a real date gives no expiry", () => {
  assert.equal(expiryFromDateInput(""), null);
  assert.equal(expiryFromDateInput("2026-02-30"), null);
  assert.equal(expiryFromDateInput("12.11.2026"), null);
});

test("presets count calendar days from today", () => {
  assert.deepEqual([...PROXY_EXPIRY_PRESET_DAYS], [7, 14, 30, 60, 90]);
  assert.equal(dateInputDaysFromToday(7, NOON), "2026-11-19");
  assert.equal(dateInputDaysFromToday(30, NOON), "2026-12-12");
  const status = proxyExpiryStatus(
    expiryFromDateInput(dateInputDaysFromToday(14, NOON)),
    NOON,
  );
  assert.equal(status.daysLeft, 14);
});

test("the date shows its year only when it is not this year", () => {
  assert.equal(formatExpiryDate(new Date(2026, 10, 12), "ru", NOON), "12.11");
  assert.equal(
    formatExpiryDate(new Date(2027, 0, 5), "ru", NOON),
    "05.01.2027",
  );
});

test("a typed date follows the locale's order", () => {
  assert.equal(parseDateField("3.11.2026", "ru", NOON), "2026-11-03");
  assert.equal(parseDateField("03.11.26", "ru", NOON), "2026-11-03");
  assert.equal(parseDateField("11/3/2026", "en-US", NOON), "2026-11-03");
  assert.equal(parseDateField("2026-11-03", "en-US", NOON), "2026-11-03");
  assert.equal(parseDateField("2026/11/03", "ru", NOON), "2026-11-03");
});

test("a date without a year is the next such day", () => {
  assert.equal(parseDateField("20.11", "ru", NOON), "2026-11-20");
  assert.equal(parseDateField("12.11", "ru", NOON), "2026-11-12");
  assert.equal(parseDateField("1.11", "ru", NOON), "2027-11-01");
});

test("a lone number is days from today", () => {
  assert.equal(parseDateField("30", "ru", NOON), "2026-12-12");
  assert.equal(parseDateField("30 дн.", "ru", NOON), "2026-12-12");
  assert.equal(parseDateField("7d", "en", NOON), "2026-11-19");
  assert.equal(parseDateField("99999", "en", NOON), null);
});

test("an empty field clears and nonsense is refused", () => {
  assert.equal(parseDateField("  ", "ru", NOON), "");
  assert.equal(parseDateField("31.02.2026", "ru", NOON), null);
  assert.equal(parseDateField("завтра", "ru", NOON), null);
  assert.equal(parseDateField("1.2.3.4", "ru", NOON), null);
});

test("the field shows a stored date the locale's way", () => {
  assert.equal(formatDateField("2026-11-03", "ru"), "03.11.2026");
  assert.equal(formatDateField("2026-11-03", "en-US"), "11/03/2026");
  assert.equal(formatDateField("", "ru"), "");
});

test("a calendar month is six weeks starting on the locale's first day", () => {
  const days = calendarDays(2026, 10, 1);
  assert.equal(days.length, 42);
  assert.equal(days[0].value, "2026-10-26");
  assert.equal(days.find((d) => d.inMonth).value, "2026-11-01");
  const sunday = calendarDays(2026, 10, 0);
  assert.equal(sunday[0].value, "2026-11-01");
  assert.equal(weekStart("ru"), 1);
  assert.equal(weekStart("en-US"), 0);
});

test("moving through the calendar by days and months", () => {
  assert.equal(shiftDateInput("2026-11-30", 1), "2026-12-01");
  assert.equal(shiftDateInput("2026-11-03", -7), "2026-10-27");
  assert.equal(shiftDateInputMonths("2026-01-31", 1), "2026-02-28");
  assert.equal(shiftDateInputMonths("2026-11-12", -1), "2026-10-12");
});
