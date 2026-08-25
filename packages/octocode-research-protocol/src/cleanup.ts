import { z } from "zod/v3";

export const AGENT_SYNC_CLEANUP_STAGES = Object.freeze({
  CANCEL_REMOTE_TASK: "cancel_remote_task",
  CLOSE_SESSION: "close_agent_sync_session",
  CLEANUP_REMOTE_TASK: "cleanup_remote_task",
} as const);

export const AGENT_SYNC_CLEANUP_STAGE_VALUES = [
  AGENT_SYNC_CLEANUP_STAGES.CANCEL_REMOTE_TASK,
  AGENT_SYNC_CLEANUP_STAGES.CLOSE_SESSION,
  AGENT_SYNC_CLEANUP_STAGES.CLEANUP_REMOTE_TASK,
] as const;

export const AgentSyncCleanupStageSchema = z.enum(AGENT_SYNC_CLEANUP_STAGE_VALUES);
export type AgentSyncCleanupStage = z.infer<typeof AgentSyncCleanupStageSchema>;
