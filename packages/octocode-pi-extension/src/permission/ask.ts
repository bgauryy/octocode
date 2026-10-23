import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { envFlag, PERMISSION_RELAY_ENV, takeNonce } from '../shared/env.js';
import { interactive } from '../shared/interactive.js';
import { withDialog } from '../shared/locks.js';
import { statusColor } from '../shared/render.js';
import { redactSecrets } from '../shared/sanitize.js';
import { takeSnapshot, type Snapshot } from './snapshot.js';
import type { PermissionRequest } from './request.js';
import { AuditLog, childAuditMessage, RELAY_VIA, WARN_ACTIONS, type AuditEntry } from './audit.js';
import { deniedMessage, DISMISSED, headlessMessage, selectDialog, type Asker, type Dialog } from './dialog.js';
import { auditLine, auditLines, computeCard, NO_REASON, transcriptPurpose } from './effect.js';
import { ACTION_CLASSES, actionNoun, chipText, CHIPS, decide, kindOf, modeSummary, parseGrants, PERMISSION_CYCLE, PERMISSION_ID, reasonAsks, startMode, UNGRANTABLE, type ActionClass, type ActionCount, type AskReason, type PermissionMode } from './mode.js';
import { notify } from '../shared/commands.js';

/**
 * What a check decided. `lasts` on an allow says how far it reaches: `call` for this call only (the mode allowed it, or the
 * user answered "Allow once"), `session` when it rests on a grant that outlives the call (a session or kind grant, a
 * spawn grant, or the user's answer that made one just now).
 */
type Verdict = { allowed: true; via: 'mode' | 'grant' | 'user'; lasts: 'call' | 'session' } | { allowed: false; via: 'user' | 'headless' | 'cancelled'; message: string };

/** A call a check let through: what the activity line says about it. */
interface ActionEvent {
  action: ActionClass | 'call';
  /** The first target, one line, redacted, at most 160 characters. */
  what: string;
  /** Destructive shell, browser control or a private network address. */
  warn: boolean;
  mode: PermissionMode;
}

/**
 * The permission state of one session and the only way anything asks the user about an agent action. A check runs
 * the mode table, then the session grants, then the dialog; without a UI, or in a subagent, every ask is denied.
 * Every call that reaches a check, and every refusal an invariant makes, is audited.
 */
export class Permission {
  readonly #pi: ExtensionAPI;
  readonly #env: NodeJS.ProcessEnv;
  readonly #isSubagent: boolean;
  /** A subagent whose parent can ask a person: its dialogs are relayed to the parent (RPC) instead of denied. */
  readonly #relay: boolean;
  /** The secret the parent gave this subagent: what marks a header or audit entry as this module's own. Empty when there is none. */
  readonly #nonce: string;
  readonly #agent: () => string;
  readonly #stderr: (text: string) => void;
  readonly #snapshotMs: number | undefined;
  /** The classes the parent's spawn grant allowed this subagent. */
  readonly #spawnGrants: ReadonlySet<ActionClass>;
  /** Session grants by `action\0key`, with their label. */
  readonly #grants = new Map<string, string>();
  /** Kinds of action the user allowed for the whole session ("all shell commands"). */
  readonly #kinds = new Set<ActionClass>();
  /** Profiles (by name and content hash) whose declared permissions the user saw and allowed this session. */
  readonly #seenProfiles = new Set<string>();
  readonly #log: AuditLog;
  readonly #listeners = new Set<(event: ActionEvent) => void>();
  #mode: PermissionMode = 'manual';
  #warned = false;
  /** The no-secret note was written. */
  #unforwarded = false;
  #paintCtx: Pick<ExtensionContext, 'hasUI' | 'ui' | 'mode'> | undefined;
  #policy = new AbortController();
  readonly #revoked = new Set<() => void>();

  constructor(options: { pi: ExtensionAPI; env?: NodeJS.ProcessEnv; isSubagent: boolean; agent?: () => string | undefined; stderr?: (text: string) => void; snapshotMs?: number }) {
    this.#pi = options.pi;
    this.#env = options.env ?? process.env;
    this.#isSubagent = options.isSubagent;
    this.#relay = options.isSubagent && envFlag(this.#env, PERMISSION_RELAY_ENV);
    // Read once from the parent's file (which is unlinked) and kept by the process (`ownNonce`), so the team can check a message the parent signed.
    const nonce = takeNonce(this.#env);
    this.#nonce = options.isSubagent ? (nonce ?? '') : '';
    this.#agent = () => options.agent?.() ?? (options.isSubagent ? 'subagent' : 'main');
    this.#stderr = options.stderr ?? ((text) => void process.stderr.write(text));
    this.#snapshotMs = options.snapshotMs;
    this.#spawnGrants = options.isSubagent ? parseGrants(this.#env) : new Set();
    this.#log = new AuditLog((entry) => {
      try {
        this.#pi.appendEntry<AuditEntry>(PERMISSION_ID, entry);
      } catch {
        // No session to write to (it was replaced); the decision itself stands.
      }
      this.#toParent(entry);
    });
  }

  /**
   * A subagent's session file sits in its own scratch folder, out of its parent's reach, so each entry also goes to its parent as a fire-and-forget notify that
   * the parent records under the child's name and never shows (`childAuditEntry`). With no nonce the parent would reject
   * it, so none is sent; the child then says so once on stderr (the parent keeps the tail), because a child without a
   * session file has no other record. Its entries still go to its own session when it has one.
   */
  #toParent(entry: AuditEntry): void {
    const ctx = this.#paintCtx;
    if (!this.#isSubagent || !ctx?.hasUI) return;
    if (this.#nonce === '') {
      if (!this.#unforwarded) this.#stderr('octocode: this subagent has no permission secret from its parent: its audit entries are not forwarded to the parent and stay in its own session only.\n');
      this.#unforwarded = true;
      return;
    }
    try {
      ctx.ui.notify(childAuditMessage(entry, this.#nonce), 'info');
    } catch {
      // The RPC session is gone: the entry stays in the child's own log.
    }
  }

  get mode(): PermissionMode {
    return this.#mode;
  }

  /** What full mode ran this session (the footer counter). */
  get count(): ActionCount {
    return this.#log.count;
  }

  /**
   * Session start: re-applies the start mode (the exported one, else full), drops the grants and the tallies, and, in a
   * run with no UI, says on stderr that full mode runs everything and where the record is.
   */
  start(ctx: ExtensionContext): void {
    const { mode, warning } = startMode(this.#env);
    this.#mode = mode;
    this.clearGrants();
    this.#log.reset(this.#recorded(ctx));
    if (warning && !this.#warned && interactive(ctx, this.#isSubagent)) notify(ctx.ui, warning, 'warning');
    if (warning) this.#warned = true;
    if (mode === 'full' && !ctx.hasUI && !this.#isSubagent) {
      this.#stderr(`octocode: permission mode full: every action runs without asking and is recorded in ${ctx.sessionManager.getSessionFile() ?? 'this session (not saved to a file)'}; /octocode permission log lists them.\n`);
    }
    this.paint(ctx);
  }

  /** Session end: writes the rollup of unlogged entries and, in a run with no UI, prints what full mode did to stderr. */
  shutdown(ctx: Pick<ExtensionContext, 'hasUI'>): void {
    this.#log.flush(Date.now(), this.#mode, this.#agent());
    const summary = !ctx.hasUI && !this.#isSubagent && this.#mode === 'full' ? this.#log.summary() : undefined;
    if (summary) this.#stderr(`${summary}\n`);
  }

  /** Writes the rollup of entries left out of the audit so far (the log command calls it so the rollup is shown). */
  flushAudit(): void {
    this.#log.flush(Date.now(), this.#mode, this.#agent());
  }

  #recorded(ctx: Pick<ExtensionContext, 'sessionManager'>): number {
    try {
      return ctx.sessionManager.getEntries().filter((entry) => entry.type === 'custom' && entry.customType === PERMISSION_ID).length;
    } catch {
      return 0;
    }
  }

  /** Changes the mode (the user's choice: by command or editor shortcut). */
  setMode(mode: PermissionMode, ctx: ExtensionContext): void {
    if (mode === this.#mode) return this.paint(ctx);
    const from = this.#mode;
    this.#mode = mode;
    if (ACTION_CLASSES.some((action) => decide(from, action) === 'allow' && decide(mode, action) === 'ask')) this.clearGrants();
    else this.#invalidate();
    this.paint(ctx);
    this.#record({ action: 'mode', what: [`${from} → ${mode}`], allowed: true, via: 'user' });
    if (interactive(ctx, this.#isSubagent)) notify(ctx.ui, `Permission mode: ${mode}. ${modeSummary(mode)}`, mode === 'full' ? 'warning' : 'info');
    // Hidden and turn-neutral: during a run Pi appends it after the current turn's results, so the model reads it next.
    this.#pi.sendMessage({ customType: PERMISSION_ID, content: `[${PERMISSION_ID}] mode is now ${mode}: ${modeSummary(mode)}`, display: false, details: { mode } }, { triggerTurn: false });
  }

  /** The editor shortcut: the next mode in `PERMISSION_CYCLE`, wrapping to the first. */
  cycle(ctx: ExtensionContext): void {
    this.setMode(PERMISSION_CYCLE[(PERMISSION_CYCLE.indexOf(this.#mode) + 1) % PERMISSION_CYCLE.length]!, ctx);
  }

  /** Session grants, as `action: label`. */
  grants(): string[] {
    return [...[...this.#kinds].map((action) => `${action}: all ${kindOf(action)}`), ...[...this.#grants].map(([key, label]) => `${key.split('\0')[0]}: ${label}`)];
  }

  /** The kinds a subagent of this session runs without asking (it cannot ask anyone): those allowed for the session. */
  childGrants(): ActionClass[] {
    return [...this.#kinds];
  }

  /** Whether the user already allowed the profile `key` (name and content hash) this session. */
  profileSeen(key: string): boolean {
    return this.#seenProfiles.has(key);
  }

  markProfileSeen(key: string): void {
    this.#seenProfiles.add(key);
  }

  clearGrants(): void {
    this.#grants.clear();
    this.#kinds.clear();
    this.#seenProfiles.clear();
    this.#invalidate();
    for (const listener of this.#revoked) listener();
  }

  /** Delegated runs must stop when their session grants or permission scope are revoked. */
  onRevoke(listener: () => void): () => void {
    this.#revoked.add(listener);
    return () => this.#revoked.delete(listener);
  }

  /** Calls `listener` for every call a check let through (the activity line). */
  onAction(listener: (event: ActionEvent) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #invalidate(): void {
    this.#policy.abort();
    this.#policy = new AbortController();
  }

  /** Reasons on this call that force an ask in the current mode. A reason only tightens. */
  #forcing(request: PermissionRequest): AskReason[] {
    const seen = new Set<AskReason>();
    const reasons: AskReason[] = [];
    for (const reason of request.reasons ?? []) {
      if (seen.has(reason) || !reasonAsks(this.#mode, reason)) continue;
      seen.add(reason);
      reasons.push(reason);
    }
    return reasons;
  }

  /** The request with its `purpose` settled: the caller's, else the assistant's last sentence for this call, else `no reason given`. */
  #withPurpose(ctx: Asker, request: PermissionRequest): PermissionRequest {
    return { ...request, purpose: request.purpose ?? transcriptPurpose(ctx.sessionManager, request.toolCallId) ?? NO_REASON };
  }

  /** The only way anything asks. `dialog` (the file review) replaces the default select; every rule stays here. */
  async check(ctx: Asker, call: PermissionRequest, dialog?: Dialog): Promise<Verdict> {
    const request = this.#withPurpose(ctx, call);
    const signal = request.signal ? AbortSignal.any([request.signal, this.#policy.signal]) : this.#policy.signal;
    const action = request.action;
    const forced = (): boolean => this.#forcing(request).length > 0;
    const classes = action ? [action, ...(request.also ?? [])] : [];
    const grant = action && !forced() && !UNGRANTABLE.has(action) && classes.length === 1 ? request.grant : undefined;
    const grantKey = grant ? `${action}\0${grant.key}` : undefined;
    let snapshot: Snapshot | undefined;
    const snap = async (): Promise<void> => {
      if (action === 'shell.destructive' && !snapshot) snapshot = await takeSnapshot(request.cwd ?? process.cwd(), this.#snapshotMs === undefined ? {} : { timeoutMs: this.#snapshotMs });
    };
    const decided = (verdict: Verdict, note?: string): Verdict => this.#decided(request, verdict, note, snapshot);
    const automatic = (): Verdict | undefined => {
      if (signal.aborted) return decided({ allowed: false, via: 'cancelled', message: DISMISSED });
      if (forced()) return undefined;
      if (classes.every((each) => decide(this.#mode, each) === 'allow')) return { allowed: true, via: 'mode', lasts: 'call' };
      const covered = (each: ActionClass): boolean => decide(this.#mode, each) === 'allow' || this.#spawnGrants.has(each) || this.#kinds.has(each) || (each === action && grantKey !== undefined && this.#grants.has(grantKey));
      return classes.every(covered) ? { allowed: true, via: 'grant', lasts: 'session' } : undefined;
    };
    const ready = automatic();
    if (ready) {
      if (ready.allowed) await snap();
      return ready.allowed ? decided(ready) : ready;
    }
    // A relayed child asks its parent through its own dialogs; any other subagent has no one to ask.
    if (!(this.#relay ? ctx.hasUI : interactive(ctx, this.#isSubagent))) return decided({ allowed: false, via: 'headless', message: headlessMessage(request, this.#mode, this.#isSubagent, this.#forcing(request)) });
    await snap();
    const asked: PermissionRequest = { ...request, ...(await computeCard(request, snapshot)), signal, ...(grant ? { grant } : {}) };
    if (!grant) delete asked.grant;
    return withDialog(async () => {
      // A preceding dialog may have granted this scope while this call waited. A forcing reason is never granted.
      const now = automatic();
      if (now) return now.allowed ? decided(now) : now;
      const answer = await (dialog ?? this.#select)(ctx, asked);
      if (answer === undefined || signal.aborted) return decided({ allowed: false, via: 'cancelled', message: DISMISSED });
      if (answer === 'session') {
        if (!grant || !grantKey) return decided({ allowed: false, via: 'cancelled', message: DISMISSED });
        this.#grants.set(grantKey, grant.label);
      }
      if (answer === 'kind') {
        if (!action || forced() || classes.length > 1 || kindOf(action) === undefined) return decided({ allowed: false, via: 'cancelled', message: DISMISSED });
        this.#kinds.add(action);
      }
      if (answer === 'once' || answer === 'session' || answer === 'kind') return decided({ allowed: true, via: 'user', lasts: answer === 'once' ? 'call' : 'session' });
      const note = typeof answer === 'object' ? answer.deny : undefined;
      return decided({ allowed: false, via: 'user', message: deniedMessage(request, note, this.#forcing(request)) }, note);
    }, signal);
  }

  /**
   * Records a refusal an invariant made (a reservation, Octocode's home, a private redirect, a failed attach check, a
   * hook's deny, an untrusted project's shared memory): no mode skips it, and the audit says what was refused and why.
   */
  refused(what: readonly string[], why: string, tool?: string): void {
    this.#record({ action: 'invariant', what: auditLines(what), allowed: false, via: 'invariant', note: auditLine(`${tool ? `${tool}: ` : ''}${why}`) });
  }

  /**
   * Records something that happened outside a check, with the verdict it had: a hook that timed out or crashed (`via: 'hook'`,
   * allowed when the call went on), or a fact reported after the event that no check could stop (`via: 'reported'`).
   * `action` is `call` unless the entry is an `invariant`; `tool` names who it came from.
   */
  noted(what: readonly string[], note: string, options: { allowed: boolean; via: 'hook' | 'reported'; action?: 'call' | 'invariant'; tool?: string }): void {
    this.#record({ action: options.action ?? 'call', what: auditLines(what), allowed: options.allowed, via: options.via, note: auditLine(`${options.tool ? `${options.tool}: ` : ''}${note}`) });
  }

  /** A command the user typed: consent by typing, so it runs, and the audit still says so. */
  typed(command: string, cwd: string): void {
    this.#record({ action: 'shell', what: auditLines([command, `cwd: ${cwd}`]), allowed: true, via: 'typed' });
  }

  /** Paints the footer chip (Pi keeps the plain text over RPC). */
  paint(ctx: Pick<ExtensionContext, 'hasUI' | 'ui' | 'mode'>): undefined {
    this.#paintCtx = ctx;
    if (!ctx.hasUI) return undefined;
    const chip = CHIPS[this.#mode];
    ctx.ui.setStatus(PERMISSION_ID, statusColor(ctx, chip.color, chipText(this.#mode, this.#log.count)));
    return undefined;
  }

  /** The default dialog (dialog.ts), with this session's mode and, for a relayed child, the secret its header carries. */
  readonly #select: Dialog = (ctx, request) =>
    selectDialog(ctx, request, { forced: this.#forcing(request).length > 0, mode: this.#mode, ...(this.#relay && this.#nonce !== '' ? { relayNonce: this.#nonce } : {}) });

  #decided(request: PermissionRequest, verdict: Verdict, note?: string, snapshot?: Snapshot): Verdict {
    const reasons = [...new Set(request.reasons ?? [])];
    const action = request.action ?? 'call';
    this.#record({
      action,
      ...(reasons.length > 0 ? { reasons } : {}),
      what: auditLines(request.what),
      ...(request.purpose ? { purpose: auditLine(request.purpose) } : {}),
      allowed: verdict.allowed,
      via: verdict.via,
      ...(note ? { note: redactSecrets(note) } : {}),
      ...(snapshot && 'ref' in snapshot ? { snapshot: snapshot.ref } : {}),
    });
    if (verdict.allowed) {
      const event: ActionEvent = { action, what: auditLine(request.what[0] ?? ''), warn: WARN_ACTIONS.has(action), mode: this.#mode };
      for (const listener of this.#listeners) listener(event);
    }
    return verdict;
  }

  /** Whether this session's own mode allows every class of a subagent's prompt and no reason on it asks: the parent may answer it itself. */
  allows(classes: readonly ActionClass[], reasons: readonly AskReason[]): boolean {
    return classes.every((each) => decide(this.#mode, each) === 'allow') && !reasons.some((reason) => reasonAsks(this.#mode, reason));
  }

  /** Records an audit entry a subagent sent (already validated by `childAuditEntry`): under its own name, in this session's log and tallies. */
  adopt(entry: AuditEntry): void {
    this.#write(entry);
  }

  /** Records, under `agent`'s name, what the parent did with that subagent's audit notifications instead of recording them (never counted as a call). */
  relayNote(agent: string, note: string): void {
    this.#write({ at: Date.now(), mode: this.#mode, action: 'call', what: [`audit entries of ${auditLine(agent)}`], agent: auditLine(agent), allowed: true, via: RELAY_VIA, note: auditLine(note) });
  }

  /** Records that this session's mode answered `agent`'s "Allow once" for `action` without a dialog (the child's own entry counts the call). */
  relayedAllow(agent: string, action: ActionClass, what: readonly string[]): void {
    this.#write({ at: Date.now(), mode: this.#mode, action, what: auditLines(what), agent: auditLine(agent), allowed: true, via: RELAY_VIA, note: `allowed ${actionNoun(action)} for ${auditLine(agent)} without a dialog: ${this.#mode} mode allows it` });
  }

  #record(entry: Omit<AuditEntry, 'at' | 'mode' | 'agent'>): void {
    this.#write({ at: Date.now(), mode: this.#mode, agent: this.#agent(), ...entry });
  }

  #write(entry: AuditEntry): void {
    const before = this.#log.count;
    this.#log.record(entry);
    if (this.#log.count !== before && this.#paintCtx) {
      try {
        this.paint(this.#paintCtx);
      } catch {
        // The session was replaced: its context is gone, and the next start paints a fresh chip.
      }
    }
  }
}
