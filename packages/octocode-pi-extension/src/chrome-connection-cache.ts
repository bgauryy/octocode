/**
 * chrome-connection-cache — reuse one live CDP connection across chromeDebug
 * tool calls instead of opening a fresh WebSocket every invocation.
 *
 * Why: connecting per call tears down the DOM/Debugger agents, so stateful CDP
 * flows (DOM.performSearch → getSearchResults, breakpoints, injected script IDs)
 * break across calls, and every call pays reconnect latency. Keyed by
 * port + target, a connection stays open (when keepTab) and is reused.
 *
 * Leak safety: closed sessions are pruned on access, the cache is LRU-capped,
 * and closeAllChromeConnections() runs on session shutdown.
 */

import fs from "node:fs";
import type { ChromeConnection } from "./chrome-debug.js";

export const MAX_CACHED_CONNECTIONS = 8;

interface CacheEntry {
  connection: ChromeConnection;
  port: number;
  createdAt: number;
  lastUsedAt: number;
  uses: number;
}

export interface CDPSessionInfo {
  key: string;
  port: number;
  mode: string;
  targetId: string;
  url: string;
  closed: boolean;
  uses: number;
  ageMs: number;
  idleMs: number;
}

// Insertion order in a Map is preserved, so it doubles as an LRU list: the first
// key is the least-recently-used (re-inserted on touch to move to the end).
const cache = new Map<string, CacheEntry>();

function now(): number {
  return Date.now();
}

/** Stable cache key for a port + optional target selector (targetId/url/newTab). */
export function connectionKey(
  port: number,
  targetSig: string | undefined,
): string {
  return `${port}::${targetSig && targetSig.length > 0 ? targetSig : "default"}`;
}

function isClosed(entry: CacheEntry): boolean {
  try {
    return entry.connection.session.closed === true;
  } catch {
    return true;
  }
}

/** Remove every entry whose session is closed. Returns the count removed. */
export function pruneClosedConnections(): number {
  let removed = 0;
  for (const [key, entry] of cache) {
    if (isClosed(entry)) {
      cache.delete(key);
      removed++;
    }
  }
  return removed;
}

/** Return a cached connection only if it is still live; prune it otherwise. */
export function getLiveConnection(key: string): ChromeConnection | undefined {
  const entry = cache.get(key);
  if (!entry) return undefined;
  if (isClosed(entry)) {
    cache.delete(key);
    return undefined;
  }
  entry.lastUsedAt = now();
  entry.uses++;
  // Move to most-recently-used position.
  cache.delete(key);
  cache.set(key, entry);
  return entry.connection;
}

function closeEntry(entry: CacheEntry): void {
  try {
    entry.connection.session.close();
  } catch {
    /* already gone */
  }
}

/** Store a connection, pruning dead entries and enforcing the LRU cap. */
export function cacheConnection(
  key: string,
  port: number,
  connection: ChromeConnection,
): void {
  pruneClosedConnections();
  // Replace any existing entry for this key (close the old session first).
  const existing = cache.get(key);
  if (existing && existing.connection !== connection) closeEntry(existing);
  cache.delete(key);
  cache.set(key, {
    connection,
    port,
    createdAt: now(),
    lastUsedAt: now(),
    uses: 1,
  });
  // Enforce cap by evicting least-recently-used (front of the Map).
  while (cache.size > MAX_CACHED_CONNECTIONS) {
    const oldestKey = cache.keys().next().value as string | undefined;
    if (oldestKey === undefined) break;
    const oldest = cache.get(oldestKey);
    if (oldest) closeEntry(oldest);
    cache.delete(oldestKey);
  }
}

/** Mark a connection most-recently-used without incrementing the reuse count. */
export function touchConnection(key: string): void {
  const entry = cache.get(key);
  if (!entry) return;
  entry.lastUsedAt = now();
  cache.delete(key);
  cache.set(key, entry);
}

/** Close and remove a single cached connection. */
export function evictConnection(key: string): void {
  const entry = cache.get(key);
  if (!entry) return;
  closeEntry(entry);
  cache.delete(key);
}

/** Snapshot of cached CDP sessions with metrics (for /octocode-chrome + leak checks). */
export function listCDPSessions(): CDPSessionInfo[] {
  const t = now();
  const out: CDPSessionInfo[] = [];
  for (const [key, entry] of cache) {
    const target = (() => {
      try {
        return entry.connection.session.targetInfo;
      } catch {
        return undefined;
      }
    })();
    const meta = entry.connection.metadata as { mode?: string } | undefined;
    out.push({
      key,
      port: entry.port,
      mode: meta?.mode ?? "unknown",
      targetId: target?.id ?? "?",
      url: target?.url ?? "?",
      closed: isClosed(entry),
      uses: entry.uses,
      ageMs: t - entry.createdAt,
      idleMs: t - entry.lastUsedAt,
    });
  }
  return out;
}

/**
 * Close every cached session and clear the cache. Returns the count closed.
 * Chromes that WE launched (session carries `_launchedPid`) are also terminated
 * and their throwaway profile dirs removed — spawned detached+unref, they would
 * otherwise outlive the Pi session whenever the model forgets `cleanup:true`.
 * Attach-mode connections have no `_launchedPid` and the user's browser is
 * never touched.
 */
export function closeAllChromeConnections(): number {
  let n = 0;
  for (const entry of cache.values()) {
    const s = entry.connection.session as unknown as Record<string, unknown>;
    const launchedPid = s["_launchedPid"] as number | undefined;
    const launchedUserDataDir = s["_launchedUserDataDir"] as string | undefined;
    closeEntry(entry);
    if (launchedPid !== undefined) {
      try {
        process.kill(launchedPid, "SIGTERM");
      } catch {
        /* already gone */
      }
      if (launchedUserDataDir) {
        try {
          fs.rmSync(launchedUserDataDir, { recursive: true, force: true });
        } catch {
          /* best-effort */
        }
      }
    }
    n++;
  }
  cache.clear();
  return n;
}
