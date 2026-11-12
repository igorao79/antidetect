import assert from "node:assert/strict";
import test from "node:test";
import {
  dateInputDaysFromToday,
  dateInputFromExpiry,
  expiryFromDateInput,
  formatExpiryDate,
  PROXY_EXPIRY_PRESET_DAYS,
  proxyExpiryStatus,
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
