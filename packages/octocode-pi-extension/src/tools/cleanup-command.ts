/**
 * Octocode cleanup command — scans ~/.octocode/tmp/clone and ~/.octocode/tmp
 * for stale entries, shows a multi-select overlay sorted by age, and deletes
 * what the user picks. Session init may announce stale items, but never opens
 * a destructive picker over the active session automatically.
 */
import fs from 'node:fs';
import path from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';
import type { PiContext } from '../types.js';
import { runMultiSelectOverlay, type SelectOverlayItem } from './ui-overlays.js';

// ─── Types ───────────────────────────────────────────────────────────────────

interface CloneMeta {
  clonedAt: string;
  expiresAt: string;
  owner: string;
  repo: string;
  branch: string;
  source: string;
  sparsePath?: string;
  sizeBytes?: number;
}

interface CleanupItem {
  /** Absolute path to remove. */
  path: string;
  /** Short display label. */
  label: string;
  /** One-line detail (age · size · extras). */
  description: string;
  /** True when the clone TTL has already expired. */
  expired: boolean;
  /** mtime / clonedAt as ms since epoch for sorting. */
  ts: number;
}

// ─── Paths ───────────────────────────────────────────────────────────────────

const CLONE_META = '.octocode-clone-meta.json';
/** Top-level tmp sub-dirs owned by octocode-tools-core — skip in the generic scan. */
const MANAGED_TMP_DIRS = new Set(['clone', 'clone-locks', 'clone-tmp', 'tree', 'mcp', 'plan']);

// ─── Helpers ─────────────────────────────────────────────────────────────────

function fmtBytes(b: number): string {
  if (b < 1_024) return `${b} B`;
  if (b < 1_048_576) return `${(b / 1_024).toFixed(1)} KB`;
  return `${(b / 1_048_576).toFixed(1)} MB`;
}

function fmtAge(ms: number): string {
  const d = Math.floor(ms / 86_400_000);
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor(ms / 60_000);
  if (d >= 1) return `${d}d ago`;
  if (h >= 1) return `${h}h ago`;
  return `${m}m ago`;
}

function dirSize(dir: string): number {
  let total = 0;
  try {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      try {
        if (e.isDirectory()) total += dirSize(p);
        else if (e.isFile()) total += fs.statSync(p).size;
      } catch { /* skip */ }
    }
  } catch { /* unreadable */ }
  return total;
}

// ─── Scanners ────────────────────────────────────────────────────────────────

/** Walk ~/.octocode/tmp/clone/<owner>/<repo>/<branch+sparse> for meta files. */
function scanClones(cloneBase: string): CleanupItem[] {
  const items: CleanupItem[] = [];
  if (!fs.existsSync(cloneBase)) return items;
  try {
    for (const owner of fs.readdirSync(cloneBase)) {
      const ownerDir = path.join(cloneBase, owner);
      if (!fs.statSync(ownerDir).isDirectory()) continue;
      for (const repo of fs.readdirSync(ownerDir)) {
        const repoDir = path.join(ownerDir, repo);
        if (!fs.statSync(repoDir).isDirectory()) continue;
        for (const branch of fs.readdirSync(repoDir)) {
          const branchDir = path.join(repoDir, branch);
          if (!fs.statSync(branchDir).isDirectory()) continue;
          const metaPath = path.join(branchDir, CLONE_META);
          if (!fs.existsSync(metaPath)) continue;
          try {
            const meta = JSON.parse(fs.readFileSync(metaPath, 'utf-8')) as CloneMeta;
            const ts = new Date(meta.clonedAt).getTime();
            const expired = Date.now() > new Date(meta.expiresAt).getTime();
            const size = meta.sizeBytes ?? dirSize(branchDir);
            const label = `${meta.owner}/${meta.repo}` +
              (meta.sparsePath ? ` (${meta.sparsePath})` : '');
            const description =
              `cloned ${fmtAge(Date.now() - ts)} · ${fmtBytes(size)} · branch:${meta.branch}` +
              (expired ? ' · ⚠ TTL expired' : '');
            items.push({ path: branchDir, label, description, expired, ts });
          } catch { /* corrupt meta — skip */ }
        }
      }
    }
  } catch { /* unreadable base */ }
  return items;
}

/** Scan ~/.octocode/tmp top-level dirs not managed by octocode-tools-core. */
function scanTmpDirs(tmpBase: string): CleanupItem[] {
  const items: CleanupItem[] = [];
  if (!fs.existsSync(tmpBase)) return items;
  try {
    for (const name of fs.readdirSync(tmpBase)) {
      if (MANAGED_TMP_DIRS.has(name)) continue;
      const p = path.join(tmpBase, name);
      try {
        const st = fs.statSync(p);
        if (!st.isDirectory()) continue;
        const size = dirSize(p);
        items.push({
          path: p,
          label: name,
          description: `last modified ${fmtAge(Date.now() - st.mtimeMs)} · ${fmtBytes(size)}`,
          expired: false,
          ts: st.mtimeMs,
        });
      } catch { /* skip */ }
    }
  } catch { /* unreadable */ }
  return items;
}

// ─── Core ────────────────────────────────────────────────────────────────────

export interface ScanResult {
  clones: CleanupItem[];
  tmp: CleanupItem[];
  /** Pre-selected paths: expired clones + tmp dirs older than 7 days. */
  defaultSelected: string[];
}

export function scanCleanupTargets(home = getOctocodeHome()): ScanResult {
  const cloneBase = path.join(home, 'tmp', 'clone');
  const tmpBase = path.join(home, 'tmp');
  const clones = scanClones(cloneBase).sort((a, b) => a.ts - b.ts);
  const tmp = scanTmpDirs(tmpBase).sort((a, b) => a.ts - b.ts);
  const sevenDaysMs = 7 * 24 * 3_600_000;
  const defaultSelected = [
    ...clones.filter((c) => c.expired).map((c) => c.path),
    ...tmp.filter((t) => Date.now() - t.ts > sevenDaysMs).map((t) => t.path),
  ];
  return { clones, tmp, defaultSelected };
}

function toOverlayItems(items: CleanupItem[], prefix: string): SelectOverlayItem[] {
  return items.map((i) => ({ value: i.path, label: `${prefix} ${i.label}`, description: i.description }));
}

/** Show the multi-select overlay and delete what the user picks. Returns deleted count. */
export async function runCleanupOverlay(ctx: PiContext | undefined): Promise<number> {
  const { clones, tmp, defaultSelected } = scanCleanupTargets();
  const allItems: SelectOverlayItem[] = [
    ...toOverlayItems(clones, '[clone]'),
    ...toOverlayItems(tmp, '[tmp]  '),
  ];
  if (allItems.length === 0) {
    ctx?.ui?.notify?.('Nothing to clean — ~/.octocode/tmp is empty.', 'info');
    return 0;
  }

  const selected = await runMultiSelectOverlay(ctx, {
    title: 'Octocode cleanup — select items to delete (space to toggle, enter to confirm)',
    items: allItems,
    initial: defaultSelected,
  });

  if (!selected || selected.length === 0) {
    ctx?.ui?.notify?.('Cleanup cancelled — nothing deleted.', 'info');
    return 0;
  }

  let deleted = 0;
  const errors: string[] = [];
  for (const p of selected) {
    try { fs.rmSync(p, { recursive: true, force: true }); deleted++; }
    catch (e) { errors.push(`${path.basename(p)}: ${e instanceof Error ? e.message : String(e)}`); }
  }
  const msg = [`Deleted ${deleted} item${deleted === 1 ? '' : 's'}.`, ...errors].join('\n');
  ctx?.ui?.notify?.(msg, errors.length ? 'warning' : 'info');
  return deleted;
}

// ─── Session-init probe (once per process) ───────────────────────────────────

let _initRan = false;

/**
 * Fire-and-forget probe: runs once per process on session_start.
 * If expired clones or old tmp dirs exist, advertise the explicit command.
 * Safe to call with ctx=undefined (no-ops silently).
 */
export function runCleanupOnInit(ctx: PiContext | undefined): void {
  if (_initRan || !ctx?.hasUI) return;
  _initRan = true;
  const { defaultSelected } = scanCleanupTargets();
  if (defaultSelected.length === 0) return;
  const count = defaultSelected.length;
  ctx.ui?.notify?.(
    `Cleanup available: ${count} stale item${count === 1 ? '' : 's'}.`,
    'info',
  );
}
