// Runs in the browser's parent process from camoufox.cfg (see
// profile_identity.rs). Every browser window of this profile says which profile
// it is: its title ends with the profile's name and the macOS Dock tile carries
// a short badge for it. The identity file lives in the profile the browser was
// started with (`-profile`), so concurrent profiles never read each other's.
// The chrome color is userChrome.css, not this script.
try {
  const owlIdentityFile = "owl-identity.json";
  const owlTitleSeparator = " \u2014 ";

  const owlReport = (e) => {
    try {
      Components.utils.reportError(`Owl profile identity: ${e}`);
    } catch (ignored) {
      // The error reporter itself is gone (early shutdown, unsandboxed build).
    }
  };

  // The file is read once per browser process, on the first window's load: by
  // then the profile directory is final. A missing or malformed file means the
  // hook does nothing.
  let owlIdentityPromise = null;
  const owlReadIdentity = () => {
    owlIdentityPromise ??= (async () => {
      let utils = typeof IOUtils !== "undefined" ? IOUtils : null;
      if (!utils) {
        const loader =
          typeof ChromeUtils !== "undefined" ? ChromeUtils : Components.utils;
        utils = loader.importESModule(
          "resource://gre/modules/IOUtils.sys.mjs",
        ).IOUtils;
      }
      const file = Services.dirsvc.get("ProfD", Ci.nsIFile);
      file.append(owlIdentityFile);
      let identity;
      try {
        identity = await utils.readJSON(file.path);
      } catch (e) {
        // Absent or unreadable: a browser not started by Owl, or one whose
        // launch did not get as far as writing the file. Nothing to add.
        return null;
      }
      if (!identity || typeof identity.name !== "string") {
        return null;
      }
      const name = identity.name.trim();
      if (!name) {
        return null;
      }
      const badge =
        typeof identity.badge === "string" && identity.badge
          ? identity.badge
          : name;
      return {
        name,
        badge: Array.from(badge).slice(0, 6).join(""),
      };
    })();
    return owlIdentityPromise;
  };

  const owlSetDockBadge = (badge) => {
    if (!badge || Services.appinfo.OS !== "Darwin") {
      return;
    }
    if (!("nsIMacDockSupport" in Ci)) {
      return;
    }
    try {
      const dock = Cc["@mozilla.org/widget/macdocksupport;1"].getService(
        Ci.nsIMacDockSupport,
      );
      dock.badgeText = badge;
    } catch (e) {
      owlReport(e);
    }
  };

  const owlApplyTitle = (win, name) => {
    const suffix = owlTitleSeparator + name;
    const title = win.document.title;
    if (title && !title.endsWith(suffix)) {
      win.document.title = title + suffix;
    }
  };

  const owlWatchWindow = (win, name) => {
    const browser = win.gBrowser;
    if (
      !browser ||
      browser.owlProfileIdentityInstalled ||
      typeof browser.updateTitlebar !== "function"
    ) {
      return;
    }
    browser.owlProfileIdentityInstalled = true;
    // Firefox rewrites the title from the selected tab on every call, so the
    // suffix is applied after each rewrite and never accumulates.
    const updateTitlebar = browser.updateTitlebar;
    browser.updateTitlebar = function (...args) {
      const result = updateTitlebar.apply(this, args);
      try {
        owlApplyTitle(win, name);
      } catch (e) {
        owlReport(e);
      }
      return result;
    };
    owlApplyTitle(win, name);
  };

  const owlInstallWindow = (win) => {
    void owlReadIdentity()
      .then((identity) => {
        if (!identity || win.closed) {
          return;
        }
        owlSetDockBadge(identity.badge);
        owlWatchWindow(win, identity.name);
      })
      .catch((e) => owlReport(e));
  };

  Services.obs.addObserver((win) => {
    try {
      win.addEventListener(
        "load",
        () => {
          try {
            if (
              win.document.documentElement.getAttribute("windowtype") !==
              "navigator:browser"
            ) {
              return;
            }
            owlInstallWindow(win);
          } catch (e) {
            owlReport(e);
          }
        },
        { once: true },
      );
    } catch (e) {
      owlReport(e);
    }
  }, "domwindowopened");

  // `domwindowopened` fires before a window has finished loading and can miss
  // the window that is already opening when autoconfig runs; the delayed
  // startup notification is the reliable point for that first window. The
  // per-window guard makes the second call a no-op.
  Services.obs.addObserver((win) => {
    try {
      if (
        win &&
        win.document &&
        win.document.documentElement.getAttribute("windowtype") ===
          "navigator:browser"
      ) {
        owlInstallWindow(win);
      }
    } catch (e) {
      owlReport(e);
    }
  }, "browser-delayed-startup-finished");
} catch (e) {
  Components.utils.reportError(`Owl profile identity: ${e}`);
}
