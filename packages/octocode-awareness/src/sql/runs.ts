// SQL shared by the execution-run verification gate.

export const RUNS_SELECT_PENDING_IDS =
  `SELECT run_id FROM task_runs WHERE status = 'PENDING' AND agent_id = ? {DYNAMIC_WHERE}`;

export const RUNS_SELECT_STATUS =
  `SELECT agent_id, status FROM task_runs WHERE run_id = ?`;

export const RUNS_UPDATE_PENDING_VERIFIED_BY_AGENT =
  `UPDATE task_runs SET status = ?, updated_at = ? WHERE run_id = ? AND agent_id = ? AND status = 'PENDING'`;

export const RUNS_UPDATE_PENDING_TO_FAILED =
  `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'PENDING'`;

export const RUNS_UPDATE_ACTIVE_TO_FAILED =
  `UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'`;

export const RUN_LOG_INSERT_VERIFIED =
  `INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'VERIFIED', ?, ?)`;
