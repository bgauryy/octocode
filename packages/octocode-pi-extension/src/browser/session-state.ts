import { shortPath } from '../shared/format.js';
import { cutText } from '../shared/util.js';
import type { CdpConnection, CdpPage } from './cdp.js';
import { ConsoleLog } from './console-log.js';
import type { OwnChrome } from './launch.js';
import type { PageNetworkPolicy } from './policy.js';
import { isHttpUrl } from '../web/guard.js';

/** The state a browser session's pages share (console, origins, downloads) and the bounds on what it remembers. */
export interface BrowserSession {
  /** The browser-level DevTools socket every page of the session goes through; closed when the session ends. */
  connection: CdpConnection;
  /** The session on the page (tab) driven now. */
  page: CdpPage;
  /**
   * The tabs under the session's network policy by target id: every tab the session drove (it stays attached, with the
   * policy and auto-attach on, when the session moves to another tab, until the tab closes) and each popup the browser
   * attached to before it ran.
   */
  governed: Map<string, CdpPage>;
  /** The workers and frames in other processes under the policy by target id (a dedicated, shared or service worker, an out-of-process frame). */
  workers: Map<string, CdpPage>;
  /** Origins of the documents the governed tabs loaded, which is where a service or shared worker of theirs lives (see `relatedToDriven` in `page-network.ts`). */
  origins: Set<string>;
  /** The origin of the page each governed tab shows now, by target id: never forgotten while the tab stays on it (see `noteOrigin`). */
  tabOrigins: Map<string, string>;
  /** DevTools target id of the page (tab) this session drives now. */
  targetId: string;
  /** DevTools port of the browser this session drives. */
  port: number;
  /** `own`: Chrome this session launched (headless, or visible on the persistent profile in visible mode); `shared`: a Chrome already running, driven in a tab of our own; `attached`: someone else's window, driven in place. */
  kind: 'own' | 'shared' | 'attached';
  headless?: boolean;
  chrome?: OwnChrome;
  /** Profile folder that outlives the session (visible mode): the user's logins stay, and Chrome stays open at shutdown. */
  profile?: string;
  /** The JavaScript dialog (alert, confirm, prompt, beforeunload) waiting for an answer. */
  dialog?: { type: string; message: string };
  /** Downloads by guid, once `download` turned them on. */
  downloads: Map<string, Download>;
  downloadDir?: string;
  /** Closes the tab this session opened in a Chrome it attached to. */
  closeTab?: () => Promise<void>;
  console: string[];
  /** Console lines the session no longer holds: pushed out of the buffer, or discarded when the bound tab changed. */
  consoleLost: { dropped: number; cleared: number };
  /** Every console line, whole, in a rotating file: what `console` and `consoleLost` no longer hold stays readable there. Made with the first line. */
  consoleLog?: ConsoleLog;
  /** Checks every request the driven page makes (documents, frames, subresources, redirect hops); a rejection fails the request. */
  policy?: PageNetworkPolicy;
  /** Why the policy last failed a request, until an action reports it. */
  blocked?: string;
  /** What the policy could only report (a WebSocket it cannot stop), until an action reports it. */
  warning?: string;
}

export interface Download {
  name: string;
  state: 'inProgress' | 'completed' | 'canceled';
  bytes: number;
  /** Where Chrome saved the file, once it reports the path. */
  path?: string;
}

/**
 * Most origins a session remembers, beyond those of the tabs it drives. At the bound the unpinned origin noted longest ago
 * is forgotten (an origin loaded again counts as noted now, and the one just noted is never the one forgotten). An origin a governed tab shows now is pinned and never
 * forgotten: a page cannot push its own origin out by loading frames on many others, which would leave its service or
 * shared worker ungoverned in a shared Chrome. When every origin is pinned the set is kept whole and may exceed the bound;
 * it then holds the tabs' origins and the one just noted. What forgetting an unpinned origin costs: in a Chrome
 * the session shares (`attach`, `OCTOCODE_CHROME_PORT`) a service or shared worker of it is no longer taken for a tab's
 * own and is left alone, like any worker not related to the tabs the session drives; the session's own Chrome governs
 * every worker whatever the origins.
 */
export const ORIGINS_MAX = 1_000;

/** Remembers the origin of an http(s) document a governed tab loads (or is about to). */
export function noteOrigin(session: BrowserSession, url: string | undefined): void {
  const origin = httpOrigin(url);
  if (origin === undefined) return;
  session.origins.delete(origin);
  session.origins.add(origin);
  if (session.origins.size <= ORIGINS_MAX) return;
  const pinned = new Set(session.tabOrigins.values());
  for (const oldest of session.origins) {
    if (session.origins.size <= ORIGINS_MAX) break;
    if (oldest !== origin && !pinned.has(oldest)) session.origins.delete(oldest);
  }
}

/** Records that tab `tab` shows `url` now: its origin is noted and pinned, and the origin it showed before is no longer. A page that is not http(s) pins nothing. */
export function noteTabOrigin(session: BrowserSession, tab: string, url: string | undefined): void {
  const origin = httpOrigin(url);
  if (origin === undefined) session.tabOrigins.delete(tab);
  else session.tabOrigins.set(tab, origin);
  noteOrigin(session, url);
}

/** The origin of an http(s) URL; undefined for anything else (an empty or blank target, a URL that does not parse). */
export function httpOrigin(url: string | undefined): string | undefined {
  try {
    const parsed = new URL(url ?? '');
    return isHttpUrl(parsed) ? parsed.origin : undefined;
  } catch {
    return undefined;
  }
}

/** Forgets the tabs that closed: their pinned origins and the approvals the policy holds for them. */
export function pruneClosed(session: BrowserSession): void {
  for (const [id, tab] of session.governed) {
    if (tab.open) continue;
    session.governed.delete(id);
    session.tabOrigins.delete(id);
    session.policy?.closed(id);
  }
  for (const [id, worker] of session.workers) if (!worker.open) session.workers.delete(id);
}

/** What a session remembers: the console lines kept, and the longest one. */
export const CONSOLE_KEEP = 200;
const CONSOLE_ENTRY_CHARS = 2_000, DOWNLOADS_KEEP = 100;

/**
 * Appends one console line. Every line goes whole to the session's console log file; only the last `CONSOLE_KEEP` stay in
 * `session.console` (the count pushed out is in `session.consoleLost`). A line over `CONSOLE_ENTRY_CHARS` is cut there and
 * says how many characters it lost and where the whole line is.
 */
export function logConsole(session: BrowserSession, line: string): void {
  const log = (session.consoleLog ??= new ConsoleLog());
  log.append(line);
  const kept = line.length > CONSOLE_ENTRY_CHARS ? cutText(line, CONSOLE_ENTRY_CHARS, { chars: true }) : line;
  session.consoleLost.dropped += Math.max(session.console.length - (CONSOLE_KEEP - 1), 0);
  const shown = kept === line ? line : `${kept}… [${line.length - kept.length} more characters cut; ${log.file ? `the whole line is in ${shortPath(log.file)}` : 'the whole line could not be saved'}]`;
  session.console = [...session.console.slice(1 - CONSOLE_KEEP), shown];
}

/** Records a download's state; past `DOWNLOADS_KEEP` the oldest finished one is dropped (a running one only when none is finished). */
export function trackDownload(session: BrowserSession, guid: string, download: Download): void {
  session.downloads.set(guid, download);
  if (session.downloads.size <= DOWNLOADS_KEEP) return;
  session.downloads.delete(([...session.downloads].find(([key, item]) => key !== guid && item.state !== 'inProgress') ?? [...session.downloads][0])[0]);
}
