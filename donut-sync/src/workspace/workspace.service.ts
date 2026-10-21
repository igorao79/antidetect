import {
  Injectable,
  Logger,
  type OnModuleDestroy,
  type OnModuleInit,
} from "@nestjs/common";
import { dataDir, inviteLink, publicUrlConfigured } from "./server-config.js";
import { WorkspaceStore } from "./workspace.store.js";

/** The members database, opened once per server. */
@Injectable()
export class WorkspaceService implements OnModuleInit, OnModuleDestroy {
  private readonly logger = new Logger("Owl");
  private storeInstance: WorkspaceStore | null = null;

  get store(): WorkspaceStore {
    this.storeInstance ??= new WorkspaceStore(dataDir());
    return this.storeInstance;
  }

  onModuleInit(): void {
    if (this.store.hasOwner()) return;
    const invite = this.store.createInvite("owner", null);
    if (!publicUrlConfigured()) {
      this.logger.warn(
        "PUBLIC_URL is not set: invite links name localhost, which only this computer can reach. Set PUBLIC_URL to the address members use.",
      );
    }
    this.logger.log(
      `No owner yet. Open this link in Owl to become the owner (it works once, for 7 days):\n\n    ${inviteLink(invite.code)}\n`,
    );
  }

  onModuleDestroy(): void {
    this.storeInstance?.close();
    this.storeInstance = null;
  }
}
