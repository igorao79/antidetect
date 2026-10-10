(() => {
  const rows = [...document.querySelectorAll("table tr")]
    .map((tr) => {
      const tds = [...tr.querySelectorAll("td")];
      if (tds.length < 2) return null;
      const cls = tds.map((td) => td.className).join(" ");
      return {
        name: tds[0].innerText.trim().slice(0, 60),
        value: tds[tds.length - 1].innerText.trim().slice(0, 80),
        state: /failed/.test(cls)
          ? "failed"
          : /warn/.test(cls)
            ? "warn"
            : /passed/.test(cls)
              ? "passed"
              : "",
      };
    })
    .filter(Boolean);
  return JSON.stringify({
    url: location.href,
    total: rows.length,
    failed: rows.filter((r) => r.state === "failed"),
    warn: rows.filter((r) => r.state === "warn"),
    passed: rows.filter((r) => r.state === "passed").length,
  });
})();
