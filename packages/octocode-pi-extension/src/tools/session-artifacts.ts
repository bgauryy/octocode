import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { assertContextSegmentAuthority, type ContextSegmentV1 } from '@octocodeai/octocode-awareness';
import { workspaceAgentKey } from '@octocodeai/octocode-shared/paths';
import { getOctocodeHome } from '../env.js';
import { extensionHome, extensionWorkspaceRoot } from '../extension-paths.js';

export const SESSION_MANIFEST_VERSION = 1 as const;
export const PLAN_SNAPSHOT_VERSION = 1 as const;

const PRIVATE_DIR_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;
const MAX_PRODUCER_PATHS = 200;
const MAX_PRODUCER_PATH_CHARS = 512;
const LOCK_TIMEOUT_MS = 1_000;
const WAIT_ARRAY = new Int32Array(new SharedArrayBuffer(4));

export type SessionIdentitySource = 'session-id' | 'session-file' | 'process-fallback';

export interface SessionIdentityInput {
  cwd?: string;
  /** Octocode home override, primarily for deterministic tests. */
  octocodeHome?: string;
  sessionManager?: {
    getSessionId?(): string | undefined;
    getSessionFile?(): string | undefined;
  };
  processId?: number;
}

export interface SessionIdentity {
  workspace: string;
  rawId: string;
  source: SessionIdentitySource;
  sessionKey: string;
}

export type SessionArtifactProducer =
  | 'plan'
  | 'compaction'
  | 'worker'
  | 'image'
  | 'browser'
  | 'log'
  | 'checkpoint-ref'
  | 'export';

interface ProducerManifestRecord {
  firstSeenAt: string;
  lastSeenAt: string;
  paths: string[];
}

export interface SessionArtifactManifestV1 {
  version: 1;
  sessionKey: string;
  identitySource: SessionIdentitySource;
  workspace: string;
  createdAt: string;
  updatedAt: string;
  producers: Partial<Record<SessionArtifactProducer, ProducerManifestRecord>>;
}

export interface SessionArtifactContext {
  identity: SessionIdentity;
  root: string;
  manifestPath: string;
  resolve(...segments: string[]): string;
  inspect(): SessionArtifactManifestV1 | undefined;
  registerProducer(producer: SessionArtifactProducer, relativePath: string): void;
  writeText(relativePath: string, text: string): void;
  writeBinary(relativePath: string, bytes: Uint8Array): void;
  writeJson(relativePath: string, value: unknown): void;
  appendEvent(relativePath: string, event: unknown): void;
}

interface InternalContext {
  mutateManifest(mutator: (manifest: SessionArtifactManifestV1) => void): SessionArtifactManifestV1;
}

const internals = new WeakMap<SessionArtifactContext, InternalContext>();

function nonEmpty(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function slug(value: string): string {
  const normalized = value
    .normalize('NFKC')
    .replace(/[^a-zA-Z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
  return normalized || 'session';
}

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

export function resolveSessionIdentity(input: SessionIdentityInput = {}): SessionIdentity {
  const workspace = path.resolve(input.cwd ?? process.cwd());
  const sessionId = nonEmpty(input.sessionManager?.getSessionId?.());
  const sessionFile = nonEmpty(input.sessionManager?.getSessionFile?.());

  let source: SessionIdentitySource;
  let rawId: string;
  let readable: string;
  if (sessionId) {
    source = 'session-id';
    rawId = sessionId;
    readable = sessionId;
  } else if (sessionFile) {
    source = 'session-file';
    rawId = path.resolve(sessionFile);
    readable = path.basename(rawId).replace(/\.[^.]+$/, '') || 'session-file';
  } else {
    source = 'process-fallback';
    const processId = input.processId ?? process.pid;
    rawId = `pid:${processId}:${workspace}`;
    readable = `process-${processId}`;
  }

  const suffix = sha256(`${rawId}\0${workspace}`).slice(0, 12);
  return { workspace, rawId, source, sessionKey: `${slug(readable)}-${suffix}` };
}

export { workspaceAgentKey };
/** @deprecated Internal compatibility name; resolves to the Pi extension workspace root. */
export const workspaceAgentRoot = extensionWorkspaceRoot;

export function sessionArtifactRoot(input: SessionIdentityInput = {}): string {
  const identity = resolveSessionIdentity(input);
  const octocodeHome = input.octocodeHome ?? getOctocodeHome();
  // Sessions live flat under extensionHome/sessions/ — no workspace nesting.
  // Workspace-level state (discovery.json, mcp/, lsp/) stays under workspaceAgentRoot.
  return path.join(extensionHome(octocodeHome), 'sessions', identity.sessionKey);
}

function isInside(root: string, candidate: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative));
}

function normalizeRelativePath(value: string): string {
  if (!value || !value.trim()) throw new Error('Session artifact path must not be empty');
  if (value.includes('\0')) throw new Error('Session artifact path must not contain NUL');
  if (path.isAbsolute(value)) throw new Error('Session artifact path must be relative');
  const normalized = path.normalize(value);
  const components = normalized.split(path.sep);
  if (normalized === '.' || components.some((part) => !part || part === '.' || part === '..')) {
    throw new Error('Session artifact path traversal is not allowed');
  }
  return normalized;
}

function resolveInside(root: string, segments: string[]): string {
  if (segments.length === 0 || segments.some((segment) => segment === '')) {
    throw new Error('Session artifact path must not be empty');
  }
  const normalized = segments.map(normalizeRelativePath);
  const candidate = path.resolve(root, ...normalized);
  if (!isInside(root, candidate) || candidate === root) throw new Error('Session artifact path escaped the session root');
  return candidate;
}

function ensurePrivateDir(dir: string): void {
  fs.mkdirSync(dir, { recursive: true, mode: PRIVATE_DIR_MODE });
  try {
    fs.chmodSync(dir, PRIVATE_DIR_MODE);
  } catch {
    // Windows and restricted filesystems may not expose POSIX mode changes.
  }
}

function ensureContainedSessionRoot(workspace: string, agentRoot: string, root: string): void {
  if (!fs.existsSync(workspace)) throw new Error(`Session workspace does not exist: ${workspace}`);
  if (fs.existsSync(agentRoot) && fs.lstatSync(agentRoot).isSymbolicLink()) {
    throw new Error(`Global agent root must not be a symlink: ${agentRoot}`);
  }
  ensurePrivateDir(agentRoot);
  const realAgentRoot = fs.realpathSync(agentRoot);
  const relativeRoot = path.relative(agentRoot, root);
  if (relativeRoot.startsWith('..') || path.isAbsolute(relativeRoot)) {
    throw new Error('Session root escaped the global agent root');
  }
  let cursor = agentRoot;
  for (const component of relativeRoot.split(path.sep)) {
    cursor = path.join(cursor, component);
    if (!fs.existsSync(cursor)) fs.mkdirSync(cursor, { mode: PRIVATE_DIR_MODE });
    const stat = fs.statSync(cursor);
    if (!stat.isDirectory()) throw new Error(`Session root ancestor is not a directory: ${cursor}`);
    const real = fs.realpathSync(cursor);
    if (!isInside(realAgentRoot, real)) throw new Error(`Session root symlink escaped the global agent root: ${cursor}`);
    try {
      fs.chmodSync(cursor, PRIVATE_DIR_MODE);
    } catch {
      // See ensurePrivateDir.
    }
  }
}

function assertNoSymlinkEscape(root: string, candidate: string): void {
  const realRoot = fs.realpathSync(root);
  const relative = path.relative(root, candidate);
  let cursor = root;
  for (const component of relative.split(path.sep)) {
    cursor = path.join(cursor, component);
    if (!fs.existsSync(cursor)) continue;
    const real = fs.realpathSync(cursor);
    if (!isInside(realRoot, real)) throw new Error(`Session artifact symlink escaped the session root: ${cursor}`);
  }
}

function fsyncDirectory(dir: string): void {
  let fd: number | undefined;
  try {
    fd = fs.openSync(dir, fs.constants.O_RDONLY);
    fs.fsyncSync(fd);
  } catch {
    // Best effort on platforms that cannot fsync a directory.
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

function atomicWrite(root: string, destination: string, contents: string | Buffer): void {
  assertNoSymlinkEscape(root, destination);
  const dir = path.dirname(destination);
  ensurePrivateDir(dir);
  assertNoSymlinkEscape(root, destination);
  const temp = path.join(dir, `.${path.basename(destination)}.tmp-${process.pid}-${randomBytes(8).toString('hex')}`);
  let fd: number | undefined;
  try {
    fd = fs.openSync(temp, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL, PRIVATE_FILE_MODE);
    fs.writeFileSync(fd, contents);
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = undefined;
    fs.renameSync(temp, destination);
    try {
      fs.chmodSync(destination, PRIVATE_FILE_MODE);
    } catch {
      // See ensurePrivateDir.
    }
    fsyncDirectory(dir);
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
    if (fs.existsSync(temp)) fs.rmSync(temp, { force: true });
  }
}

function sleep(milliseconds: number): void {
  Atomics.wait(WAIT_ARRAY, 0, 0, milliseconds);
}

function acquireLock(root: string, lockPath: string): string {
  assertNoSymlinkEscape(root, lockPath);
  ensurePrivateDir(path.dirname(lockPath));
  const deadline = Date.now() + LOCK_TIMEOUT_MS;
  const token = randomBytes(16).toString('hex');
  while (true) {
    try {
      const fd = fs.openSync(lockPath, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL, PRIVATE_FILE_MODE);
      try {
        fs.writeFileSync(fd, `${JSON.stringify({ token, pid: process.pid, createdAt: new Date().toISOString() })}\n`);
        fs.fsyncSync(fd);
      } finally {
        fs.closeSync(fd);
      }
      return token;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'EEXIST') throw error;
      // Never reclaim by age: deleting a path after a stale observation can remove
      // a successor's fresh lock. A crash therefore produces an explicit bounded
      // timeout; projection state is rebuildable from the active CustomEntry.
      if (Date.now() >= deadline) throw new Error(`Timed out waiting for session artifact lock: ${lockPath}`);
      sleep(10);
    }
  }
}

function releaseLock(lockPath: string, token: string): void {
  try {
    const current = JSON.parse(fs.readFileSync(lockPath, 'utf8')) as { token?: string };
    if (current.token === token) fs.rmSync(lockPath, { force: true });
  } catch {
    // Missing/replaced locks are not ours to remove.
  }
}

function withLock<T>(root: string, lockPath: string, body: () => T): T {
  const token = acquireLock(root, lockPath);
  try {
    return body();
  } finally {
    releaseLock(lockPath, token);
  }
}

function readManifest(file: string): SessionArtifactManifestV1 | undefined {
  if (!fs.existsSync(file)) return undefined;
  const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as SessionArtifactManifestV1;
  if (parsed.version !== 1 || typeof parsed.sessionKey !== 'string' || !parsed.producers) {
    throw new Error(`Invalid session artifact manifest: ${file}`);
  }
  return parsed;
}

function createManifest(identity: SessionIdentity): SessionArtifactManifestV1 {
  const now = new Date().toISOString();
  return {
    version: SESSION_MANIFEST_VERSION,
    sessionKey: identity.sessionKey,
    identitySource: identity.source,
    workspace: identity.workspace,
    createdAt: now,
    updatedAt: now,
    producers: {},
  };
}

function writeExclusive(file: string, contents: string): 'created' | 'exists' {
  ensurePrivateDir(path.dirname(file));
  let fd: number | undefined;
  try {
    fd = fs.openSync(file, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL, PRIVATE_FILE_MODE);
    fs.writeFileSync(fd, contents);
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = undefined;
    return 'created';
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST') return 'exists';
    throw error;
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

export function createSessionArtifactContext(input: SessionIdentityInput = {}): SessionArtifactContext {
  const identity = resolveSessionIdentity(input);
  const agentRoot = extensionHome(input.octocodeHome ?? getOctocodeHome());
  const root = sessionArtifactRoot({ ...input, cwd: identity.workspace });
  ensureContainedSessionRoot(identity.workspace, agentRoot, root);
  const manifestPath = path.join(root, 'manifest.json');
  writeExclusive(manifestPath, `${JSON.stringify(createManifest(identity), null, 2)}\n`);
  const manifestLock = path.join(root, '.manifest.lock');

  const context: SessionArtifactContext = {
    identity,
    root,
    manifestPath,
    resolve: (...segments) => resolveInside(root, segments),
    inspect: () => readManifest(manifestPath),
    registerProducer: (producer, relativePath) => {
      const relative = normalizeRelativePath(relativePath);
      if (relative.length > MAX_PRODUCER_PATH_CHARS) throw new Error('Session artifact producer path is too long');
      internal.mutateManifest((manifest) => {
        const now = new Date().toISOString();
        const current = manifest.producers[producer];
        const paths = current ? [...current.paths] : [];
        if (!paths.includes(relative)) {
          if (paths.length >= MAX_PRODUCER_PATHS) throw new Error(`Session artifact producer ${producer} exceeded its path limit`);
          paths.push(relative);
        }
        manifest.producers[producer] = {
          firstSeenAt: current?.firstSeenAt ?? now,
          lastSeenAt: now,
          paths,
        };
      });
    },
    writeText: (relativePath, text) => {
      if (typeof text !== 'string') throw new TypeError('Session artifact text must be a string');
      atomicWrite(root, resolveInside(root, [relativePath]), text);
    },
    writeBinary: (relativePath, bytes) => {
      if (!(bytes instanceof Uint8Array)) throw new TypeError('Session artifact bytes must be a Uint8Array');
      atomicWrite(root, resolveInside(root, [relativePath]), Buffer.from(bytes));
    },
    writeJson: (relativePath, value) => {
      const serialized = `${JSON.stringify(value, null, 2)}\n`;
      atomicWrite(root, resolveInside(root, [relativePath]), serialized);
    },
    appendEvent: (relativePath, event) => {
      const line = `${JSON.stringify(event)}\n`;
      const destination = resolveInside(root, [relativePath]);
      assertNoSymlinkEscape(root, destination);
      ensurePrivateDir(path.dirname(destination));
      assertNoSymlinkEscape(root, destination);
      const fd = fs.openSync(destination, fs.constants.O_WRONLY | fs.constants.O_APPEND | fs.constants.O_CREAT, PRIVATE_FILE_MODE);
      try {
        const buffer = Buffer.from(line);
        const written = fs.writeSync(fd, buffer, 0, buffer.length);
        if (written !== buffer.length) throw new Error(`Incomplete session event write: ${written}/${buffer.length}`);
        fs.fsyncSync(fd);
      } finally {
        fs.closeSync(fd);
      }
      try {
        fs.chmodSync(destination, PRIVATE_FILE_MODE);
      } catch {
        // See ensurePrivateDir.
      }
    },
  };

  const internal: InternalContext = {
    mutateManifest: (mutator) => withLock(root, manifestLock, () => {
      const manifest = readManifest(manifestPath) ?? createManifest(identity);
      mutator(manifest);
      manifest.updatedAt = new Date().toISOString();
      atomicWrite(root, manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
      return manifest;
    }),
  };
  internals.set(context, internal);
  return context;
}

export function isPathInsideSessionRoot(ctx: SessionArtifactContext, candidate: string): boolean {
  return isInside(ctx.root, path.resolve(candidate));
}

export interface PlanBranchSnapshotV1<T = unknown> {
  version: 1;
  sourceEntryId: string;
  generation: number;
  capturedAt: string;
  state: T;
}

export interface PlanProjectionV1<T = unknown> extends PlanBranchSnapshotV1<T> {}

export type ProjectionCasResult<T = unknown> =
  | { ok: true; value: PlanProjectionV1<T> }
  | { ok: false; reason: 'generation-conflict'; actualGeneration: number | null };

function snapshotFilename(sourceEntryId: string): string {
  return `${slug(sourceEntryId)}-${sha256(sourceEntryId).slice(0, 12)}.json`;
}

function validateSnapshot<T>(snapshot: PlanBranchSnapshotV1<T>): void {
  if (snapshot.version !== PLAN_SNAPSHOT_VERSION) throw new Error('Unsupported plan snapshot version');
  if (!nonEmpty(snapshot.sourceEntryId)) throw new Error('Plan snapshot sourceEntryId is required');
  if (!Number.isSafeInteger(snapshot.generation) || snapshot.generation < 1) throw new Error('Plan snapshot generation must be a positive integer');
  if (!Number.isFinite(Date.parse(snapshot.capturedAt))) throw new Error('Plan snapshot capturedAt must be an ISO timestamp');
}

export function writePlanBranchSnapshot<T>(ctx: SessionArtifactContext, snapshot: PlanBranchSnapshotV1<T>): string {
  validateSnapshot(snapshot);
  const relative = path.join('plan', 'branches', snapshotFilename(snapshot.sourceEntryId));
  const destination = ctx.resolve(relative);
  const serialized = `${JSON.stringify(snapshot, null, 2)}\n`;
  assertNoSymlinkEscape(ctx.root, destination);
  const status = writeExclusive(destination, serialized);
  if (status === 'exists') {
    const existing = fs.readFileSync(destination, 'utf8');
    if (existing !== serialized) throw new Error(`Immutable plan branch snapshot integrity conflict: ${snapshot.sourceEntryId}`);
  } else {
    fsyncDirectory(path.dirname(destination));
  }
  ctx.registerProducer('plan', relative);
  return destination;
}

function parseProjection<T>(file: string): PlanProjectionV1<T> | undefined {
  if (!fs.existsSync(file)) return undefined;
  const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as PlanProjectionV1<T>;
  validateSnapshot(parsed);
  return parsed;
}

export function readPlanProjection<T>(ctx: SessionArtifactContext): PlanProjectionV1<T> | undefined {
  return parseProjection<T>(ctx.resolve('plan/state.json'));
}

export function compareAndSwapPlanProjection<T>(
  ctx: SessionArtifactContext,
  expectedGeneration: number | null,
  next: PlanProjectionV1<T>,
): ProjectionCasResult<T> {
  validateSnapshot(next);
  const projectionPath = ctx.resolve('plan/state.json');
  const lockPath = ctx.resolve('plan/state.json.lock');
  return withLock(ctx.root, lockPath, () => {
    const current = parseProjection<T>(projectionPath);
    const actualGeneration = current?.generation ?? null;
    if (actualGeneration !== expectedGeneration) {
      return { ok: false, reason: 'generation-conflict', actualGeneration };
    }
    const requiredGeneration = (actualGeneration ?? 0) + 1;
    if (next.generation !== requiredGeneration) {
      throw new Error(`Plan projection generation must advance to ${requiredGeneration}, received ${next.generation}`);
    }
    ctx.writeJson('plan/state.json', next);
    ctx.registerProducer('plan', 'plan/state.json');
    return { ok: true, value: next };
  });
}

export interface RehydrationLedgerV1 {
  version: 1;
  sessionKey: string;
  workspace: string;
  capturedAt: string;
  expiresAt: string;
  digest: string;
  segments: ContextSegmentV1[];
  contentRefs?: Record<string, string>;
  plan?: { scope: string; branchSnapshotId: string; generation: number; revision?: string };
  pendingInteractionIds: string[];
  consumerCursors: Record<string, number>;
}

const REHYDRATION_LEDGER_PATH = 'compaction/rehydration-v1.json';
const REHYDRATION_CONTENT_DIR = 'compaction/segments';
const CONTEXT_KINDS = new Set<ContextSegmentV1['kind']>([
  'product-policy', 'user-request', 'project-instruction', 'skill', 'plan',
  'memory-lead', 'tool-contract', 'tool-result', 'peer-event',
]);
const CONTEXT_AUTHORITIES = new Set<ContextSegmentV1['authority']>(['product', 'user', 'project', 'external-data']);
const CONTEXT_SCOPES = new Set<ContextSegmentV1['scope']>(['session', 'turn', 'task', 'path']);
const CONTEXT_VISIBILITIES = new Set<ContextSegmentV1['visibility']>(['hidden-policy', 'inspectable', 'transcript']);
const CONTEXT_REHYDRATION = new Set<ContextSegmentV1['rehydrate']>(['always', 'on-trigger', 'summary-only', 'never']);

function validateRehydrationSegments(segments: ContextSegmentV1[]): ContextSegmentV1[] {
  const ids = new Set<string>();
  return segments.map((segment) => {
    if (!segment || typeof segment !== 'object' || segment.version !== 1
      || !CONTEXT_KINDS.has(segment.kind) || !CONTEXT_AUTHORITIES.has(segment.authority)
      || !CONTEXT_SCOPES.has(segment.scope) || !CONTEXT_VISIBILITIES.has(segment.visibility)
      || !CONTEXT_REHYDRATION.has(segment.rehydrate)) {
      throw new Error('Invalid rehydration segment contract');
    }
    const validated = assertContextSegmentAuthority(segment);
    if (ids.has(validated.id)) throw new Error(`Duplicate rehydration segment: ${validated.id}`);
    ids.add(validated.id);
    if (!/^sha256:[a-f0-9]{64}$/.test(validated.digest)) throw new Error(`Invalid rehydration digest: ${validated.id}`);
    if (validated.tokenBudget !== undefined
      && (!Number.isInteger(validated.tokenBudget) || validated.tokenBudget <= 0)) {
      throw new Error(`Invalid rehydration token budget: ${validated.id}`);
    }
    return validated;
  });
}

function rehydrationDigest(value: Omit<RehydrationLedgerV1, 'digest'>): string {
  return `sha256:${sha256(JSON.stringify(value))}`;
}

export function writeRehydrationLedger(
  ctx: SessionArtifactContext,
  input: Omit<RehydrationLedgerV1, 'version' | 'sessionKey' | 'workspace' | 'digest' | 'contentRefs' | 'expiresAt'> & {
    expiresAt?: string;
    segmentContents?: Record<string, string>;
  },
): RehydrationLedgerV1 {
  const segments = validateRehydrationSegments(input.segments);
  const segmentsById = new Map(segments.map((segment) => [segment.id, segment]));
  const contentRefs: Record<string, string> = {};
  for (const [id, content] of Object.entries(input.segmentContents ?? {}).sort(([a], [b]) => a.localeCompare(b))) {
    const segment = segmentsById.get(id);
    if (!segment) throw new Error(`Rehydration content has no segment manifest: ${id}`);
    if (contentDigestForRehydration(content) !== segment.digest) throw new Error(`Rehydration content digest mismatch: ${id}`);
    const relative = `${REHYDRATION_CONTENT_DIR}/${sha256(id).slice(0, 24)}.txt`;
    ctx.writeText(relative, content);
    ctx.registerProducer('compaction', relative);
    contentRefs[id] = relative;
  }
  const body: Omit<RehydrationLedgerV1, 'digest'> = {
    version: 1,
    sessionKey: ctx.identity.sessionKey,
    workspace: ctx.identity.workspace,
    capturedAt: input.capturedAt,
    expiresAt: input.expiresAt ?? new Date(Date.parse(input.capturedAt) + 24 * 60 * 60_000).toISOString(),
    segments,
    ...(Object.keys(contentRefs).length > 0 ? { contentRefs } : {}),
    ...(input.plan ? { plan: input.plan } : {}),
    pendingInteractionIds: [...new Set(input.pendingInteractionIds)],
    consumerCursors: Object.fromEntries(Object.entries(input.consumerCursors).sort(([a], [b]) => a.localeCompare(b))),
  };
  const ledger = { ...body, digest: rehydrationDigest(body) };
  ctx.writeJson(REHYDRATION_LEDGER_PATH, ledger);
  ctx.registerProducer('compaction', REHYDRATION_LEDGER_PATH);
  return ledger;
}

export type RehydrationLedgerInspection =
  | { status: 'valid'; ledger: RehydrationLedgerV1 }
  | { status: 'missing' | 'corrupt' | 'identity-mismatch' };

export function inspectRehydrationLedger(ctx: SessionArtifactContext): RehydrationLedgerInspection {
  const file = ctx.resolve(REHYDRATION_LEDGER_PATH);
  if (!fs.existsSync(file)) return { status: 'missing' };
  try {
    assertNoSymlinkEscape(ctx.root, file);
    const parsed = JSON.parse(fs.readFileSync(file, 'utf8')) as RehydrationLedgerV1;
    if (parsed?.sessionKey !== ctx.identity.sessionKey || parsed?.workspace !== ctx.identity.workspace) {
      return { status: 'identity-mismatch' };
    }
    if (parsed.version !== 1 || !Number.isFinite(Date.parse(parsed.capturedAt)) || !Number.isFinite(Date.parse(parsed.expiresAt))
      || !Array.isArray(parsed.segments) || !Array.isArray(parsed.pendingInteractionIds)
      || !parsed.consumerCursors || typeof parsed.consumerCursors !== 'object') return { status: 'corrupt' };
    parsed.segments = validateRehydrationSegments(parsed.segments);
    const { digest, ...body } = parsed;
    return typeof digest === 'string' && digest === rehydrationDigest(body)
      ? { status: 'valid', ledger: parsed }
      : { status: 'corrupt' };
  } catch {
    return { status: 'corrupt' };
  }
}

export function readRehydrationLedger(ctx: SessionArtifactContext): RehydrationLedgerV1 | undefined {
  const result = inspectRehydrationLedger(ctx);
  return result.status === 'valid' ? result.ledger : undefined;
}

function contentDigestForRehydration(content: string): string {
  return `sha256:${sha256(content)}`;
}

export function resolveRehydrationContentRefs(
  ctx: SessionArtifactContext,
  ledger: RehydrationLedgerV1,
): { contents: Record<string, string>; corrupt: string[] } {
  const contents: Record<string, string> = {};
  const corrupt: string[] = [];
  const segmentIds = new Set(ledger.segments.map((segment) => segment.id));
  for (const [id, relative] of Object.entries(ledger.contentRefs ?? {})) {
    try {
      if (!segmentIds.has(id) || typeof relative !== 'string' || !relative.startsWith(`${REHYDRATION_CONTENT_DIR}/`)) throw new Error('invalid content ref');
      const file = ctx.resolve(relative);
      assertNoSymlinkEscape(ctx.root, file);
      const stat = fs.lstatSync(file);
      if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('content ref is not a regular file');
      contents[id] = fs.readFileSync(file, 'utf8');
    } catch {
      corrupt.push(id);
    }
  }
  return { contents, corrupt: corrupt.sort() };
}

export function readRehydrationSegmentContents(ctx: SessionArtifactContext, ledger: RehydrationLedgerV1): Record<string, string> {
  const resolved = resolveRehydrationContentRefs(ctx, ledger);
  if (resolved.corrupt.length > 0) throw new Error(`Corrupt rehydration content reference(s): ${resolved.corrupt.join(', ')}`);
  return resolved.contents;
}

export function resolveRehydrationSegments(
  ledger: RehydrationLedgerV1,
  contentById: Record<string, string>,
  options: { totalTokenBudget?: number } = {},
): { restored: string[]; stale: string[]; skipped: string[]; overBudget: string[]; estimatedTokens: number } {
  const restored: string[] = [];
  const stale: string[] = [];
  const skipped: string[] = [];
  const overBudget: string[] = [];
  let estimatedTokens = 0;
  for (const segment of ledger.segments) {
    const content = contentById[segment.id];
    if (segment.rehydrate === 'never' || content === undefined) { skipped.push(segment.id); continue; }
    const digest = contentDigestForRehydration(content);
    const tokens = Math.ceil(content.length / 4);
    if (digest !== segment.digest) {
      stale.push(segment.id);
      continue;
    }
    if ((segment.tokenBudget !== undefined && tokens > segment.tokenBudget)
      || (options.totalTokenBudget !== undefined && estimatedTokens + tokens > options.totalTokenBudget)) {
      overBudget.push(segment.id);
      continue;
    }
    restored.push(segment.id);
    estimatedTokens += tokens;
  }
  return { restored, stale, skipped, overBudget, estimatedTokens };
}
