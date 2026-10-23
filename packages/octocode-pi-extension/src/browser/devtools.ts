import { isLoopbackHostname } from '../shared/net.js';
import { isRecord } from '../shared/util.js';
import { devtoolsFetch } from './launch.js';

/**
 * Refuses a DevTools WebSocket URL that is not the local browser's own, in every mode: a browser on `port` (even an
 * attached one the user started) answers `/json/list` with a `webSocketDebuggerUrl`, and a hostile or confused answer
 * must not point the session at another host, port or service. The scheme is `ws:`, the host this machine, the port
 * the one asked for and the path under `/devtools/`.
 */
export function checkDevtoolsUrl(raw: string, port: number): void {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(`Refused a DevTools URL that is not a URL: ${raw}`);
  }
  const problem =
    url.protocol !== 'ws:' ? `scheme ${url.protocol} is not ws:`
    : !isLoopbackHostname(url.hostname) ? `host ${url.hostname} is not this machine`
    : Number(url.port) !== port ? `port ${url.port || '(default)'} is not ${port}`
    : !url.pathname.startsWith('/devtools/') ? `path ${url.pathname} is not under /devtools/`
    : undefined;
  if (problem) throw new Error(`Refused the DevTools URL ${url.protocol}//${url.host}${url.pathname}: ${problem}.`);
}

export interface PageTarget {
  id: string;
  webSocketDebuggerUrl: string;
  title?: string;
  url?: string;
}

/** Whether a listed or opened target names the two things a session drives a tab by: its id and its DevTools URL. */
function isTarget(value: unknown): value is PageTarget & Record<string, unknown> {
  return isRecord(value) && typeof value['id'] === 'string' && typeof value['webSocketDebuggerUrl'] === 'string';
}

/** The browser's page targets (tabs), in the order Chrome lists them (most recently used first). */
export async function pageTargets(port: number): Promise<PageTarget[]> {
  const targets: unknown = await (await devtoolsFetch(port, '/json/list', 2_000)).json();
  if (!Array.isArray(targets)) throw new Error(`Chrome on port ${port} answered /json/list without a target list`);
  return targets.filter((target): target is PageTarget => isTarget(target) && target['type'] === 'page');
}

/** Opens a tab on `url` in the session's browser. */
export async function openTab(port: number, url: string): Promise<PageTarget> {
  const response = await devtoolsFetch(port, `/json/new?${encodeURI(url)}`, 5_000, { method: 'PUT' });
  if (!response.ok) throw new Error(`Chrome on port ${port} refused a new tab (HTTP ${response.status})`);
  const target: unknown = await response.json();
  if (!isTarget(target)) throw new Error(`Chrome on port ${port} opened a tab without an id and DevTools URL`);
  return target;
}
