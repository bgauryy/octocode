import fs from 'node:fs';
import { tildePath } from '../shared/home.js';
import { clipText, isRecord } from '../shared/util.js';
import { oneLine } from './store.js';

/**
 * Background work recorded in a session's entries: `bash` calls with `background: true` (result `details.job` and
 * `details.log`, report `octocode-bash-job` with the same log) and background `agent` calls (result `details.status`
 * `background` and `details.id`, report `octocode-agent-result` with the same id). A start without its report is work
 * still running in this process, or, read when a session starts, work that ended with an earlier process unreported.
 */
export interface BackgroundWork {
  kind: 'bash' | 'agent';
  id: string;
  /** The bash command or the subagent task, on one line. */
  label: string;
  /** A bash job's log file. */
  log?: string;
}

const JOB_REPORT_TYPE = 'octocode-bash-job';
const AGENT_RESULT_TYPE = 'octocode-agent-result';
/** Longest command or task shown for one item. */
const LABEL_CHARS = 60;

/** Bash job ids restart with each extension instance; the log path (id plus start time) is unique. */
const jobKey = (id: string, log: string | undefined) => (log ? `log:${log}` : `id:${id}`);

/** The unreported work in `entries`, oldest first; with `since` (ms), only work started at or after it. */
export function unreportedWork(entries: readonly unknown[], since?: number): BackgroundWork[] {
  const args = new Map<string, Record<string, unknown>>();
  const started = new Map<string, BackgroundWork>();
  const reported = new Set<string>();
  for (const entry of entries) {
    if (!isRecord(entry)) continue;
    if (entry['type'] === 'custom_message') {
      const details = isRecord(entry['details']) ? entry['details'] : {};
      const id = typeof details['id'] === 'string' ? details['id'] : undefined;
      if (!id) continue;
      if (entry['customType'] === JOB_REPORT_TYPE) {
        const log = typeof details['log'] === 'string' ? details['log'] : undefined;
        reported.add(jobKey(id, log));
      } else if (entry['customType'] === AGENT_RESULT_TYPE) reported.add(`agent:${id}`);
      continue;
    }
    const message = entry['type'] === 'message' && isRecord(entry['message']) ? entry['message'] : undefined;
    if (!message) continue;
    if (message['role'] === 'assistant' && Array.isArray(message['content'])) {
      for (const part of message['content']) if (isRecord(part) && part['type'] === 'toolCall' && typeof part['id'] === 'string' && isRecord(part['arguments'])) args.set(part['id'], part['arguments']);
      continue;
    }
    if (message['role'] !== 'toolResult' || message['isError'] === true || !isRecord(message['details'])) continue;
    if (since !== undefined && !(Date.parse(String(entry['timestamp'] ?? '')) >= since)) continue;
    const details = message['details'];
    const input = args.get(String(message['toolCallId'])) ?? {};
    if (message['toolName'] === 'bash' && typeof details['job'] === 'string') {
      const log = typeof details['log'] === 'string' ? details['log'] : undefined;
      started.set(jobKey(details['job'], log), { kind: 'bash', id: details['job'], label: oneLine(String(input['command'] ?? ''), LABEL_CHARS), ...(log ? { log } : {}) });
    } else if (message['toolName'] === 'agent' && details['status'] === 'background' && typeof details['id'] === 'string') {
      started.set(`agent:${details['id']}`, { kind: 'agent', id: details['id'], label: oneLine(String(input['task'] ?? ''), LABEL_CHARS) });
    }
  }
  // A report written before reports carried their log matches by id.
  return [...started].filter(([key, work]) => !reported.has(key) && !(work.kind === 'bash' && reported.has(`id:${work.id}`))).map(([, work]) => work);
}

/**
 * One item per line part: `bash-3 (`yarn test`, log ~/…/bash-3-….log)` or `general-ab12 (task "…")`. A log that is
 * gone (the retention sweep removed it) is not offered.
 */
export function describeWork(work: BackgroundWork, checkLog = true): string {
  if (work.kind === 'agent') return `${work.id} (task "${work.label}")`;
  const log = work.log ? (!checkLog || fs.existsSync(work.log) ? `, log ${tildePath(work.log)}` : ', log deleted') : '';
  return `${work.id} (\`${work.label}\`${log})`;
}

/** `items` joined with `; ` and capped at `max` characters, ending with `…` when some did not fit. */
export function workList(items: readonly string[], max: number): string {
  let text = '';
  for (const item of items) {
    const next = text ? `${text}; ${item}` : item;
    if (next.length > max) return text ? `${text}; …` : clipText(item, max);
    text = next;
  }
  return text;
}
