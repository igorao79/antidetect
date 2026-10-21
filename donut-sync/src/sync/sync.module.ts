import { Module } from "@nestjs/common";
import { AuthGuard } from "../auth/auth.guard.js";
import { BlobController } from "../storage/blob.controller.js";
import { LocksService } from "../workspace/locks.service.js";
import {
  JoinController,
  LocksController,
  WorkspaceController,
} from "../workspace/workspace.controller.js";
import { WorkspaceService } from "../workspace/workspace.service.js";
import { InternalController } from "./internal.controller.js";
import { SyncController } from "./sync.controller.js";
import { SyncService } from "./sync.service.js";

@Module({
  controllers: [
    SyncController,
    InternalController,
    BlobController,
    JoinController,
    WorkspaceController,
    LocksController,
  ],
  providers: [SyncService, AuthGuard, WorkspaceService, LocksService],
  exports: [SyncService, WorkspaceService],
})
export class SyncModule {}
