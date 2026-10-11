const DISMISSED_KEY = "permissions.startupPromptDismissed";

/**
 * Whether the launch-time camera and microphone prompt was already closed.
 * It is offered once; afterwards the permissions are requested from Settings.
 */
export function startupPermissionPromptDismissed(): boolean {
  try {
    return window.localStorage.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function dismissStartupPermissionPrompt() {
  try {
    window.localStorage.setItem(DISMISSED_KEY, "1");
  } catch {
    // Storage is unavailable; the prompt is offered again at the next launch.
  }
}
