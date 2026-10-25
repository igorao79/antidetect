// Runs in the browser's parent process from camoufox.cfg (see last_window.rs).
// Closing the last browser window, with its close button or by closing its
// last tab, quits the browser instead, the way Owl stops a profile, so the
// session keeps the window and its tabs.
try {
  const owlIsLastWindow = (win) => {
    for (const other of Services.wm.getEnumerator("navigator:browser")) {
      if (other !== win && !other.closed) {
        return false;
      }
    }
    return true;
  };
  const owlQuit = () => {
    Services.tm.dispatchToMainThread(() =>
      Services.startup.quit(Ci.nsIAppStartup.eForceQuit),
    );
  };
  const owlWatch = (win) => {
    // The close button.
    win.addEventListener("close", (event) => {
      if (!event.defaultPrevented && owlIsLastWindow(win)) {
        event.preventDefault();
        owlQuit();
      }
    });
    // Closing the last tab (tabbrowser calls closeWindow and keeps the tab).
    const closeWindow = win.closeWindow;
    win.closeWindow = function (close, ...rest) {
      if (close && owlIsLastWindow(win)) {
        owlQuit();
        return false;
      }
      return closeWindow.call(this, close, ...rest);
    };
  };
  Services.obs.addObserver((win) => {
    win.addEventListener(
      "load",
      () => {
        if (
          win.document.documentElement.getAttribute("windowtype") ===
          "navigator:browser"
        ) {
          owlWatch(win);
        }
      },
      { once: true },
    );
  }, "domwindowopened");
} catch (e) {
  Components.utils.reportError(`Owl last-window hook: ${e}`);
}
