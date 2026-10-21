import { dataDir, inviteLink } from "./workspace/server-config.js";
import { WorkspaceStore } from "./workspace/workspace.store.js";

/**
 * `node dist/cli owner-link`: print a fresh owner invite link while the server
 * has no owner (the one in the startup log expired or was lost).
 */
function main(): number {
  const [command] = process.argv.slice(2);
  if (command !== "owner-link") {
    console.error("Usage: node dist/cli owner-link");
    return 2;
  }
  const store = new WorkspaceStore(dataDir());
  try {
    if (store.hasOwner()) {
      console.error(
        "This server already has an owner. The owner invites everyone else from the Server page in Owl.",
      );
      return 1;
    }
    const invite = store.createInvite("owner", null);
    console.log(inviteLink(invite.code));
    return 0;
  } finally {
    store.close();
  }
}

process.exitCode = main();
