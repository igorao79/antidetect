import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_FORM,
  formProblem,
  searchCountries,
  toggleCountry,
  toRequest,
  ttlMinutesOf,
} from "./proxy-generator-form.ts";

const OPTIONS = {
  enabled: true,
  countries: ["us", "de", "ru", "fr"],
  popular_countries: ["us", "de"],
  protocols: ["http", "socks5"],
  ttl_hours: [1, 24, 168],
  ttl_minutes_min: 5,
  ttl_minutes_max: 43200,
  bulk_max: 20,
  owned_max: 100,
};

test("lifetime choices turn into minutes", () => {
  assert.equal(ttlMinutesOf({ kind: "forever" }), null);
  assert.equal(ttlMinutesOf({ kind: "preset", hours: 24 }), 1440);
  assert.equal(
    ttlMinutesOf({ kind: "custom", amount: "3", unit: "days" }),
    4320,
  );
  assert.ok(
    Number.isNaN(
      ttlMinutesOf({ kind: "custom", amount: "1.5", unit: "hours" }),
    ),
  );
  assert.ok(
    Number.isNaN(ttlMinutesOf({ kind: "custom", amount: "", unit: "hours" })),
  );
});

test("the form names what keeps it from being sent", () => {
  assert.equal(formProblem(DEFAULT_FORM, OPTIONS), null);
  assert.equal(
    formProblem({ ...DEFAULT_FORM, count: 21 }, OPTIONS),
    "proxyGenerator.errors.count",
  );
  assert.equal(
    formProblem({ ...DEFAULT_FORM, countriesMode: "only" }, OPTIONS),
    "proxyGenerator.errors.countries",
  );
  assert.equal(
    formProblem(
      {
        ...DEFAULT_FORM,
        ttl: { kind: "custom", amount: "2", unit: "minutes" },
      },
      OPTIONS,
    ),
    "proxyGenerator.errors.ttl",
  );
  assert.equal(
    formProblem(
      { ...DEFAULT_FORM, ttl: { kind: "custom", amount: "31", unit: "days" } },
      OPTIONS,
    ),
    "proxyGenerator.errors.ttl",
  );
  assert.equal(
    formProblem({ ...DEFAULT_FORM, note: "x".repeat(121) }, OPTIONS),
    "proxyGenerator.errors.textTooLong",
  );
});

test("the request drops countries for any and blank text", () => {
  assert.deepEqual(
    toRequest({
      ...DEFAULT_FORM,
      count: 3,
      countries: ["us"],
      name: "  ",
      note: " reg ",
      ttl: { kind: "preset", hours: 1 },
      protocol: "http",
    }),
    {
      count: 3,
      countriesMode: "any",
      countries: [],
      ttlMinutes: 60,
      name: null,
      note: "reg",
      protocol: "http",
    },
  );
});

test("countries toggle and search with popular ones first", () => {
  assert.deepEqual(toggleCountry(["us"], "de"), ["us", "de"]);
  assert.deepEqual(toggleCountry(["us", "de"], "us"), ["de"]);
  const names = {
    us: "United States",
    de: "Germany",
    ru: "Russia",
    fr: "France",
  };
  const nameOf = (code) => names[code];
  assert.deepEqual(
    searchCountries(OPTIONS.countries, OPTIONS.popular_countries, "", nameOf),
    ["us", "de", "fr", "ru"],
  );
  assert.deepEqual(
    searchCountries(
      OPTIONS.countries,
      OPTIONS.popular_countries,
      "fra",
      nameOf,
    ),
    ["fr"],
  );
});
