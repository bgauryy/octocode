/** Public compatibility barrel for tasks.ts. */
export { getTask, activeTaskClaimForAgent } from './tasks-catalog.js';
export { createTask, addTaskDependency, listTasks, countTasks, listReadyTasks, countReadyTasks } from './tasks-ready.js';
export { claimTask, heartbeatTaskClaim, submitTask, releaseTaskClaim } from './tasks-claims.js';
export type { PlanTaskStatus, PlanTaskRecord, TaskClaimRecord, TaskRunRecord, CreateTaskParams } from './tasks-catalog.js';
export type { ClaimTaskResult } from './tasks-claims.js';
