/**
 * plan — the session and shared task-breakdown facade for the think-first gate.
 * The plan is projected into the system prompt every turn from `getCurrentPlanReadModel`,
 * so it survives compaction and stays visible. Shared scope reconciles stable steps
 * onto Awareness internally; callers never synchronize a second mutable graph.
 */

import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { ToolDefinition, ToolCallResult, PiContext, PiTheme, NotifyFn, RenderResultOptions } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { CLI_STATUS_TEXT, paint } from '../tui/cli-design.js';
import { SEP } from '../tui/palette.js';
import { buildPlanPrompt } from '../prompts/plan-prompt.js';
import { adoptPlanModePolicy, enterPlanMode, exitPlanMode, isPlanMode } from './plan-mode.js';
import { runAskPrompt, type AskOutcome } from './ask-user-tool.js';
import { consumeHumanAuthorizationReceipt, createHumanAuthorizationReceipt } from './interaction-broker.js';
import { getCurrentPlanReadModel, renderPlanContext, type PlanReadModelV1 } from './plan-read-model.js';
import { enablePlanHtmlSync, resetPlanHtmlSync, openPlanHtml, syncCurrentPlanHtmlIfEnabled, writeCurrentPlanArtifacts as writeCanonicalPlanArtifacts, writePlanReadModelArtifacts, planArtifactsDir, readRfcDoc } from './plan-html.js';
import { serveDirectory, unmount } from './local-server.js';
import { FREE_TEXT_TELL_DIFFERENTLY, PLAN_APPROVE_DESC, PLAN_APPROVE_LABEL, PLAN_APPROVED_HEADER, PLAN_APPROVED_REVIEW_QUESTION, PLAN_APPROVAL_HEADER, PLAN_COMPLETE_HEADER, PLAN_COMPLETE_QUESTION, PLAN_PROPOSE_HINT, PLAN_READY_HEADER, PLAN_REJECT_DESC, PLAN_REJECT_LABEL, PLAN_RFC_REVIEW_HEADER, PLAN_SET_BROWSER_QUESTION } from '../tui/content.js';
import { buildQueryCallBlocks, buildToolView } from './render-helpers.js';
import { wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { refreshStatusPanel } from './status-panel.js';
import { setManagedActivity } from './runtime-renderer.js';
import { activePlanScope, setPlan, setPlanLifecycle, finishPlanVerification, activatePlan, proposePlanReview, acceptPlanReview, requestPlanChanges, startAcceptedPlan, rollbackAcceptedPlanStart, addStep, startStep, restorePlanSteps, completeStep, removeStep, clearPlan, getPlan, getPlanReviewState, getPlanCoordination, updatePlanCoordination, setPlanAwarenessMappings, MARK, stepLabel, displayStatus, depsMet, dependencyIndexes, resolveRfcPath, setPlanRfc, getPlanRfc, addPlanDecision, getPlanDecisions, planPhaseIndex, PLAN_PHASES, type PlanPhase, type PlanStep, type DisplayStatus, type StepInput } from './active-plan.js';
import { completeUnifiedPlanTask, finalizeUnifiedPlan, getAwarenessAgentId, projectUnifiedPlan, type ObservedCheckReceipt, type UnifiedPlanScope } from './awareness-shared.js';
import { buildQueryEnvelopeSchema, executeQueryBatch, type QueryRecord } from './query-envelope.js';

let planBrowserMessageSender: ((message: string) => void | Promise<void>) | undefined;
let planDirectoryServer: typeof serveDirectory = serveDirectory;
let unifiedPlanProjector: typeof projectUnifiedPlan = projectUnifiedPlan;

export function setUnifiedPlanProjectorForTests(next?: typeof projectUnifiedPlan): void {
  unifiedPlanProjector = next ?? projectUnifiedPlan;
}

/** Test seam for browser-first review without binding the process-wide localhost server. */
export function setPlanDirectoryServerForTests(next?: typeof serveDirectory): void {
  planDirectoryServer = next ?? serveDirectory;
}
type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

type PlanAction = 'set' | 'propose' | 'clarify' | 'add' | 'start' | 'complete' | 'remove' | 'clear' | 'show';

/** One clarify-phase question: a prompt plus optional multiple-choice options. */
interface ClarifyQuestion {
  prompt: string;
  options?: Array<{ value?: string; label: string; description?: string; recommended?: boolean; pros?: string[]; cons?: string[] }>;
}

/** Cap on questions per clarify call — a bounded interview, not an interrogation. */
const MAX_CLARIFY = 3;

/** At/above this step count a plan is treated as consequential regardless of self-report. */
const CONSEQUENTIAL_STEP_COUNT = 5;
/** Risk vocabulary that flags consequential work in a step's text. */
const RISK_RE = /\b(migrat|schema|auth|delete|\bdrop\b|truncate|rename|breaking|public[\s-]?api|secret|credential|\btoken\b|encrypt|permission|rollback|backfill|lockfile|release)\w*/i;

/**
 * Heuristic "does this look consequential?" from the proposed steps alone — step
 * count and risk vocabulary. Pure and exported for testing. Returns the verdict
 * plus the human-readable signals that fired (for the gate's block message).
 */
export function inferConsequential(steps: StepInput[]): { consequential: boolean; signals: string[] } {
  const texts = steps.map((s) => (typeof s === 'string' ? s : s?.text ?? ''));
  const signals: string[] = [];
  if (texts.length >= CONSEQUENTIAL_STEP_COUNT) signals.push(`${texts.length} steps`);
  const hits = new Set<string>();
  for (const t of texts) {
    const m = t.match(RISK_RE);
    if (m) hits.add(m[0].toLowerCase());
  }
  if (hits.size) signals.push(`risk terms: ${[...hits].slice(0, 4).join(', ')}`);
  return { consequential: signals.length > 0, signals };
}

interface PlanParams extends QueryRecord {
  action: PlanAction;
  scope?: UnifiedPlanScope;
  receipt?: ObservedCheckReceipt;
  steps?: StepInput[];
  text?: string;
  activeForm?: string;
  dependsOn?: number[];
  paths?: string[];
  taskReasoning?: string;
  acceptance?: string;
  checkCommand?: string;
  index?: number;
  /** For set/propose: mark the work consequential so the RFC review gate applies. */
  consequential?: boolean;
  /** For set/propose: path to the reviewable RFC (a `.octocode/rfc/<name>/` dir or its RFC.md). Renders on the plan page. */
  rfcPath?: string;
  /** For action:clarify — up to 3 high-impact questions to ask the user before proposing. */
  questions?: ClarifyQuestion[];
  /** Required with consequential:false when the work still looks consequential — the justification for skipping the RFC. */
  reason?: string;
}

const PLAN_ACTION_FIELDS: Readonly<Record<PlanAction, readonly string[]>> = Object.freeze({
  set: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  propose: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  clarify: ['questions'],
  add: ['scope', 'text', 'activeForm', 'dependsOn', 'paths', 'taskReasoning', 'acceptance', 'checkCommand'],
  start: ['scope', 'index'],
  complete: ['scope', 'index', 'receipt'],
  remove: ['index'],
  clear: [],
  show: [],
});

function assertPlanActionFields(query: QueryRecord, action: PlanAction): void {
  const allowed = new Set(['reasoning', 'action', ...PLAN_ACTION_FIELDS[action]]);
  const extra = Object.keys(query).filter((field) => !allowed.has(field));
  if (extra.length > 0) throw new Error(`action:${action} does not accept ${extra.join(', ')}.`);
}

const TEXT_MARK: Record<DisplayStatus, string> = { ...MARK, blocked: '[!]' };

function renderList(steps: PlanStep[]): string {
  if (steps.length === 0) return '(no active plan)';
  return steps.map((s, i) => {
    const ds = displayStatus(s, steps);
    const dependencies = dependencyIndexes(s, steps);
    const needs = ds === 'blocked' && dependencies.length ? ` (needs ${dependencies.join(',')})` : '';
    return `${TEXT_MARK[ds]} ${i + 1}. ${s.text}${needs}`;
  }).join('\n');
}

function planPresentation(ctx: PiContext | undefined, scope: string) {
  const plan = getCurrentPlanReadModel(ctx, scope);
  return { plan, steps: plan.tasks, addendum: renderPlanContext(plan) };
}

const BAR_WIDTH = 8;

/**
 * A one-line phase stepper — `✓ Research → ✓ RFC → ▸ Build …` — so the panel
 * always shows where in the flow the plan is. Done phases fade, the current one
 * is brand-bold, upcoming ones are muted. Same color contract as the checklist.
 */
export function phaseStepperLine(phase: PlanPhase, theme?: PiTheme): string {
  const cur = planPhaseIndex(phase);
  const bold = (t: string) => (typeof theme?.bold === 'function' ? theme.bold(t) : t);
  const parts = PLAN_PHASES.map((label, i) => {
    if (i < cur) return paint(theme, 'dim', `✓ ${label}`);
    if (i === cur) return paint(theme, 'brand', bold(`▸ ${label}`));
    return paint(theme, 'muted', `○ ${label}`);
  });
  return parts.join(paint(theme, 'dim', ' → '));
}

/** Render a compact `███░░` progress bar for done/total. */
function progressBar(done: number, total: number): string {
  if (total <= 0) return '';
  const filled = Math.max(0, Math.min(BAR_WIDTH, Math.round((done / total) * BAR_WIDTH)));
  return `${'█'.repeat(filled)}${'░'.repeat(BAR_WIDTH - filled)}`;
}

/**
 * Complete plan projection for the persistent below-editor panel. Every task is
 * visible and the running lane is explicit; hiding tasks here caused the UI to
 * disagree with durable plan storage and made resumed/compacted work look lost.
 */
/** Pure terminal projection of the canonical model. */
export function planPanelModelLines(readModel: PlanReadModelV1, theme?: PiTheme, width?: number): string[] {
  if (readModel.tasks.length === 0) return [];
  const done = readModel.summary.done;
  const doing = readModel.tasks.filter((step) => step.status === 'doing');
  const currentLabel = doing.length > 1
    ? `${SEP}now: ${doing.map((task) => task.activeText ?? task.text).join(SEP)}`
    : doing[0] ? `${SEP}now: ${doing[0].activeText ?? doing[0].text}` : '';
  const header = paint(theme, 'brand', `Plan  ${progressBar(done, readModel.summary.total)}  ${done}/${readModel.summary.total} done${currentLabel}`);
  const rows = readModel.tasks.map((task) => {
    const status = task.status;
    const mark = status === 'done' ? '✓' : status === 'doing' ? '▶' : status === 'blocked' ? '!' : '○';
    const token = status === 'done' ? 'dim' : status === 'doing' ? 'brand' : status === 'blocked' ? 'warning' : 'muted';
    const label = status === 'doing' ? (task.activeText ?? task.text) : task.text;
    const text = `${mark} ${task.index}. ${label}${status === 'doing' ? '  running' : ''}`;
    return status === 'doing' && typeof theme?.bold === 'function'
      ? paint(theme, token, theme.bold(text))
      : paint(theme, token, text);
  });
  const lines = [header, phaseStepperLine(readModel.phase, theme), ...rows];
  return width
    ? lines.flatMap((line) => wrapTextWithAnsi(line, Math.max(1, width)))
    : lines;
}

/**
 * Write the plan doc, start a localhost server hosting it, arm live sync, and
 * open the served URL in a browser (interactive TUI only). Returns the served
 * URL, or undefined if the doc write or the server failed. Consequential
 * proposals use this as their primary, non-blocking review surface when the
 * host can relay browser messages; `/octocode-plan html` opens it on demand.
 */
/**
 * Per-scope mount name so parallel plan scopes in one process get distinct URLs
 * instead of silently clobbering a shared `/plan/` mount. The artifact dir's
 * basename is already the scope hash, so reuse it.
 */
function planMountName(scope: string): string {
  return `plan-${path.basename(planArtifactsDir(scope))}`;
}

function planWorkspace(scope: string): string {
  return scope.split('\0')[0] || scope;
}

function requestedPlanScope(scope: string, explicit?: UnifiedPlanScope): UnifiedPlanScope {
  if (explicit) return explicit;
  const mode = getPlanCoordination(scope).mode;
  return mode === 'required' ? 'shared' : mode === 'local' ? 'session' : 'auto';
}

function configurePlanScope(scope: string, requested?: UnifiedPlanScope): void {
  if (!requested) return;
  const current = getPlanCoordination(scope);
  if (requested === 'session' && current.awarenessPlanId) {
    throw new Error('cannot switch a mapped shared plan to session scope; complete or abandon the shared plan first');
  }
  if (requested === 'shared') updatePlanCoordination(scope, { mode: 'required', localReason: null });
  else if (requested === 'session') updatePlanCoordination(scope, { mode: 'local', localReason: 'explicit plan scope=session' });
  else updatePlanCoordination(scope, { mode: 'auto', localReason: null });
}

function ensureUnifiedProjection(scope: string, explicit: UnifiedPlanScope | undefined, ctx?: PiContext): 'session' | 'shared' {
  configurePlanScope(scope, explicit);
  const steps = getPlan(scope);
  const coordination = getPlanCoordination(scope);
  const review = getPlanReviewState(scope);
  const projection = unifiedPlanProjector({
    sourceKind: 'pi',
    requestedScope: requestedPlanScope(scope, explicit),
    workspace: coordination.coordinationWorkspace || planWorkspace(scope),
    sourcePlanKey: coordination.sourcePlanKey,
    awarenessPlanId: coordination.awarenessPlanId,
    title: steps[0]?.text ? `Plan: ${steps[0].text}` : 'Octocode plan',
    goal: steps.map((step) => step.text).join(' → '),
    rfcPath: getPlanRfc(scope),
    rfcRevision: review.acceptedRevision ?? review.revision,
    agentId: getAwarenessAgentId(ctx),
    steps,
  });
  if (projection.scope === 'shared') {
    setPlanAwarenessMappings(scope, {
      awarenessPlanId: projection.awarenessPlanId!,
      taskIdsByStepId: projection.taskIdsByStepId!,
      materializedRevision: review.acceptedRevision ?? review.revision,
    });
  }
  return projection.scope;
}

function sharedStartContractError(steps: PlanStep[]): string | undefined {
  const knownIds = new Set(steps.map((step) => step.id));
  for (const [index, step] of steps.entries()) {
    const missingDependency = step.dependsOnStepIds?.find((id) => !knownIds.has(id));
    if (missingDependency) return `step ${index + 1} references missing dependency ${missingDependency}`;
    if (!step.paths?.length && !step.reasoning?.trim()) {
      return `step ${index + 1} must declare paths or explain why it has no path scope`;
    }
    if (!step.acceptance?.trim()) return `step ${index + 1} must declare acceptance criteria`;
  }
  return undefined;
}

function writeCurrentPlanArtifacts(ctx: PiContext | undefined, scope: string, status: 'draft' | 'approved' | 'active' = 'active') {
  return writeCanonicalPlanArtifacts(ctx, scope, { status, workspace: planWorkspace(scope) });
}

/** Compact, review-safe handoff for local-file, chat, and headless surfaces. */
export function buildRfcReviewTldr(
  scope: string,
  steps: PlanStep[],
  revision: string,
  artifacts?: { htmlPath: string; mdPath: string },
): string {
  const rfc = readRfcDoc(scope);
  const title = rfc?.markdown.match(/^#\s+(.+?)\s*$/m)?.[1] ?? 'RFC review';
  const status = rfc?.status ?? 'Draft';
  const localPath = rfc?.path ?? getPlanRfc(scope) ?? '(RFC path unavailable)';
  const fileUri = path.isAbsolute(localPath) ? pathToFileURL(localPath).href : undefined;
  const stepLines = steps.slice(0, 5).map((step, index) => `  ${index + 1}. ${step.text}`);
  if (steps.length > 5) stepLines.push(`  … ${steps.length - 5} more in plan.md`);
  return [
    `[PLAN] RFC ready for review · rev ${revision.slice(0, 8)} · implementation not started`,
    '',
    'Summary',
    `- ${title} · ${status}`,
    `- ${steps.length} dependency-ordered step${steps.length === 1 ? '' : 's'}; Accept binds these RFC bytes but does not Start implementation.`,
    ...stepLines,
    '',
    `RFC file: ${localPath}`,
    fileUri ? `RFC URI: ${fileUri}` : undefined,
    artifacts ? `Plan Markdown: ${artifacts.mdPath}` : undefined,
    artifacts ? `Plan HTML: ${artifacts.htmlPath}` : undefined,
    '',
    'After review:',
    `- Accept: /octocode-plan accept ${revision}`,
    '- Request changes: /octocode-plan changes <feedback>',
    '- Open browser later: /octocode-plan html',
  ].filter((line): line is string => typeof line === 'string').join('\n');
}

/** Tear down a scope's plan surface: stop live sync and drop its server mount. */
function tearDownPlanHtml(scope: string): void {
  resetPlanHtmlSync();
  unmount(planMountName(scope));
}

async function servePlanPage(ctx: PiContext | undefined, scope: string): Promise<string | undefined> {
  // servePlanPage is the sole writer for the browser path (callers must not
  // pre-write) so the doc and the served bytes never diverge.
  const model = getCurrentPlanReadModel(ctx, scope);
  const phase = model.phase;
  const artifactStatus = phase === 'accepted'
    ? 'approved'
    : phase === 'executing' || phase === 'verifying' || phase === 'complete'
      ? 'active'
      : 'draft';
  const artifacts = writePlanReadModelArtifacts(scope, model, { status: artifactStatus, workspace: planWorkspace(scope) });
  if (!artifacts) return undefined;
  // Host the plan's artifact dir under /plan-<hash>/ on the shared CLI server.
  const served = await planDirectoryServer(planMountName(scope), planArtifactsDir(scope), {
    indexFile: 'plan.html',
    onMessage: planBrowserMessageSender,
  });
  if (!served) return undefined;
  // Arm live sync so later plan mutations rewrite the files the server reads and
  // the page's meta-refresh picks them up.
  enablePlanHtmlSync(scope);
  if (ctx?.hasUI && ctx.mode === 'tui') {
    const opened = await openPlanHtml(served.url);
    if (!opened.ok && opened.message) ctx.ui?.notify?.(opened.message, 'warn');
  }
  return served.url;
}

/** Mirror the active plan into the unified below-editor status panel only. */
export function refreshPlanUi(ctx?: PiContext): void {
  // Live HTML sync is independent of the TUI: headless mutations still keep
  // an opened plan page fresh.
  const scope = activePlanScope(ctx);
  const steps = getPlan(scope);
  publishPlanActivity(ctx, scope, steps);
  syncCurrentPlanHtmlIfEnabled(ctx, scope);
  if (steps.length > 0) adoptPlanModePolicy(ctx, getPlanReviewState(scope));
  else exitPlanMode(ctx);
  if (!ctx?.hasUI) return;
  refreshStatusPanel(ctx);
}

function publishPlanActivity(ctx: PiContext | undefined, scope: string, steps: PlanStep[]): void {
  const review = getPlanReviewState(scope);
  switch (review.phase) {
    case 'researching':
      setManagedActivity(ctx, { kind: 'researching', planScope: scope });
      return;
    case 'needs_answers':
      setManagedActivity(ctx, { kind: 'awaiting_input', planScope: scope, question: 'Planning input required' });
      return;
    case 'draft':
      setManagedActivity(ctx, { kind: 'planning', planScope: scope });
      return;
    case 'in_review':
      setManagedActivity(ctx, { kind: 'reviewing', planScope: scope, revision: review.revision });
      return;
    case 'accepted':
      if (review.acceptedRevision) setManagedActivity(ctx, { kind: 'awaiting_start', planScope: scope, revision: review.acceptedRevision });
      return;
    case 'executing': {
      const active = steps.find((step) => step.status === 'doing');
      if (active) {
        setManagedActivity(ctx, { kind: 'working', planScope: scope, stepId: active.id, label: stepLabel(active) });
        return;
      }
      const runnable = steps.find((step) => step.status === 'todo' && depsMet(step, steps));
      if (runnable) {
        setManagedActivity(ctx, { kind: 'ready_to_work', planScope: scope, label: stepLabel(runnable) });
        return;
      }
      if (steps.some((step) => step.status !== 'done')) {
        setManagedActivity(ctx, { kind: 'blocked', label: 'No dependency-ready plan step' });
        return;
      }
      setManagedActivity(ctx, { kind: 'verifying', planScope: scope, label: 'Plan steps complete' });
      return;
    }
    case 'verifying':
      setManagedActivity(ctx, { kind: 'verifying', planScope: scope });
      return;
    case 'complete':
      setManagedActivity(ctx, { kind: 'complete', label: 'Plan complete' });
      return;
    case 'blocked':
      setManagedActivity(ctx, { kind: 'blocked', label: review.outcomeReason ?? 'Plan blocked' });
      return;
    case 'failed':
      setManagedActivity(ctx, { kind: 'failed', label: review.outcomeReason ?? 'Plan failed' });
      return;
    case 'abandoned':
      setManagedActivity(ctx, { kind: 'idle' });
      return;
  }
}

// ─── /octocode-plan command (user can view / complete / delete tasks) ────────

export const OCTOCODE_PLAN_COMMAND_USAGE = '/octocode-plan [new <goal>|off|show|html|accept <revision>|changes [feedback]|complete <n>|start <displayed-revision|n>|remove <n>|clear]';
export const OCTOCODE_PLAN_COMMAND_COMPLETIONS = ['new ', 'off', 'show', 'html', 'accept ', 'changes ', 'complete ', 'start ', 'remove ', 'clear'] as const;

/** Host hook for `/octocode-plan new`: sends the plan-mode prompt to the agent as the next user turn. */
export type SendPlanPrompt = (text: string) => void | Promise<void>;


export async function handleOctocodePlanCommand(args: string, ctx: PiContext | undefined, notify: NotifyFn, sendPrompt?: SendPlanPrompt): Promise<void> {
  const scope = activePlanScope(ctx);
  const tokens = args.trim().split(/\s+/).filter(Boolean);
  const [action = 'show', arg] = tokens;
  const remainder = tokens.slice(1).join(' ');
  if (action === 'off') {
    const was = isPlanMode(ctx);
    exitPlanMode(ctx);
    notify(ctx, was ? 'Plan mode off — write tools restored.' : 'Plan mode was not on.', 'info');
    return;
  }
  if (action === 'new') {
    // Plan mode: hand the agent an explicit research → propose → gate prompt.
    // The goal is everything after `new`; the agent asks for one when absent.
    const goal = args.trim().replace(/^new\b/, '').trim();
    if (!sendPrompt) {
      notify(ctx, 'This host cannot send prompts — describe the goal and ask the agent to `plan(propose)` it.', 'warning');
      return;
    }
    enterPlanMode(ctx);
    setPlanLifecycle(scope, 'researching');
    setManagedActivity(ctx, { kind: 'researching', planScope: scope, detail: goal || undefined });
    await sendPrompt(buildPlanPrompt(goal));
    notify(ctx, goal ? `Plan mode on (write tools blocked until the required authorization gate): planning “${goal.slice(0, 80)}”.` : 'Plan mode on (write tools blocked until the required authorization gate): the agent will ask for the goal.', 'info');
    return;
  }
  const n = Number(arg);
  // Bad indices must say WHY nothing changed — the plan reprint alone reads as
  // a silent success (the tool path returns [PLAN] errors; parity for the command).
  const validStep = (verb: string): boolean => {
    const count = getPlan(scope).length;
    if (count === 0) {
      notify(ctx, `No active plan — nothing to ${verb}.`, 'warning');
      return false;
    }
    if (!Number.isFinite(n) || !Number.isInteger(n) || n < 1 || n > count) {
      notify(ctx, `Usage: /octocode-plan ${verb} <n> with n between 1 and ${count} (got "${arg ?? ''}").`, 'warning');
      return false;
    }
    return true;
  };
  switch (action) {
    case 'html': {
      // Explicit user intent — serve + open the live local page; from now on
      // every plan mutation rewrites it (the page meta-refreshes) so the browser
      // tab tracks the plan while you keep working in the terminal.
      const url = await servePlanPage(ctx, scope);
      if (!url) {
        notify(ctx, 'Could not start the local plan server (is ~/.octocode/ writable?).', 'warning');
        return;
      }
      notify(ctx, `Plan page: ${url} (local server, live — updates on every plan change)`, 'info');
      return;
    }
    case 'clear': {
      const current = getPlan(scope);
      if (current.some((step) => step.awarenessTaskId) && current.some((step) => step.status !== 'done')) {
        notify(ctx, 'Mapped shared plans cannot be cleared while work is unfinished; complete or abandon the shared work first.', 'warning');
        return;
      }
      clearPlan(scope);
      tearDownPlanHtml(scope);
      notify(ctx, 'Plan cleared.', 'info');
      break;
    }
    case 'accept': {
      if (!arg) {
        notify(ctx, 'Usage: /octocode-plan accept <displayed-revision>', 'warning');
        return;
      }
      const planId = getPlanCoordination(scope).sourcePlanKey;
      const receipt = createHumanAuthorizationReceipt(ctx, {
        planId,
        revision: arg,
        scope: 'plan.accept',
        question: `Accept RFC revision ${arg} for this plan?`,
      });
      const accepted = acceptPlanReview(scope, arg, receipt.receiptId);
      if (!accepted.ok) {
        notify(ctx, `RFC acceptance failed: ${accepted.message}`, 'warning');
        refreshPlanUi(ctx);
        return;
      }
      consumeHumanAuthorizationReceipt(scope, { receiptId: receipt.receiptId, planId, revision: arg, scope: 'plan.accept' });
      writeCurrentPlanArtifacts(ctx, scope, 'approved');
      refreshPlanUi(ctx);
      notify(ctx, `RFC accepted · rev ${accepted.state.acceptedRevision?.slice(0, 8) ?? 'unknown'} — mutation remains blocked until Start.`, 'info');
      return;
    }
    case 'changes': {
      const changed = requestPlanChanges(scope);
      if (!changed.ok) {
        notify(ctx, `Could not request changes: ${changed.message}`, 'warning');
        refreshPlanUi(ctx);
        return;
      }
      if (remainder) addPlanDecision(scope, 'Requested plan changes', remainder);
      writeCurrentPlanArtifacts(ctx, scope, 'draft');
      refreshPlanUi(ctx);
      notify(ctx, `Changes requested${remainder ? `: ${remainder}` : ''}. Revise the RFC and re-propose.`, 'info');
      return;
    }
    case 'complete':
      if (validStep('complete')) {
        const target = getPlan(scope)[n - 1];
        if (target?.awarenessTaskId) {
          notify(ctx, 'Shared completion requires an observed receipt; use plan.complete with receipt {command,status,message}.', 'warning');
          return;
        }
        const completed = completeStep(scope, n);
        if (completed.length > 0 && completed.every((step) => step.status === 'done')) {
          refreshPlanUi(ctx);
          finishPlanVerification(scope, true, 'All local plan steps completed');
        }
      }
      break;
    case 'start': {
      if (getPlanReviewState(scope).phase === 'accepted') {
        const contractError = sharedStartContractError(getPlan(scope));
        if (contractError) {
          notify(ctx, `Implementation did not start: invalid shared step contract — ${contractError}.`, 'warning');
          refreshPlanUi(ctx);
          return;
        }
        const state = getPlanReviewState(scope);
        const revision = state.acceptedRevision!;
        if (!arg) {
          notify(ctx, 'Usage: /octocode-plan start <displayed-revision>. Start is bound to the revision shown by the browser or terminal.', 'warning');
          return;
        }
        if (arg !== revision) {
          notify(ctx, `Implementation did not start: displayed revision is stale (expected ${revision.slice(0, 8)}). Refresh the plan and Start again.`, 'warning');
          refreshPlanUi(ctx);
          return;
        }
        const planId = getPlanCoordination(scope).sourcePlanKey;
        const receipt = createHumanAuthorizationReceipt(ctx, {
          planId,
          revision,
          scope: 'plan.start',
          question: `Start implementation of accepted RFC revision ${revision}?`,
        });
        consumeHumanAuthorizationReceipt(scope, { receiptId: receipt.receiptId, planId, revision, scope: 'plan.start' });
        const started = startAcceptedPlan(scope, receipt.receiptId);
        if (!started.ok) {
          notify(ctx, `Implementation did not start: ${started.message}`, 'warning');
          refreshPlanUi(ctx);
          return;
        }
        try {
          ensureUnifiedProjection(scope, undefined, ctx);
        } catch (error) {
          const reason = error instanceof Error ? error.message : String(error);
          rollbackAcceptedPlanStart(scope, `Shared Start failed: ${reason}`);
          notify(ctx, `Implementation did not start; RFC acceptance was preserved: ${reason}`, 'warning');
          refreshPlanUi(ctx);
          return;
        }
        writeCurrentPlanArtifacts(ctx, scope, 'active');
        refreshPlanUi(ctx);
        notify(ctx, `Implementation started from accepted revision ${started.state.acceptedRevision?.slice(0, 8) ?? 'unknown'}.`, 'info');
        return;
      }
      if (validStep('start')) {
        const beforeStart = getPlan(scope).map((step) => ({ ...step }));
        startStep(scope, n);
        try {
          ensureUnifiedProjection(scope, undefined, ctx);
        } catch (error) {
          restorePlanSteps(scope, beforeStart);
          notify(ctx, `Step did not start; local plan state was restored after shared projection failed: ${error instanceof Error ? error.message : String(error)}`, 'warning');
          refreshPlanUi(ctx);
          return;
        }
      }
      break;
    }
    case 'remove':
      if (validStep('remove')) {
        if (getPlan(scope)[n - 1]?.awarenessTaskId) {
          notify(ctx, 'Mapped shared steps cannot be removed in place; abandon or revise the shared plan explicitly.', 'warning');
          return;
        }
        removeStep(scope, n);
      }
      break;
    case 'show':
    default:
      break;
  }
  refreshPlanUi(ctx);
  const steps = getPlan(scope);
  const done = steps.filter((s) => s.status === 'done').length;
  notify(ctx, steps.length === 0 ? 'No active plan.' : `Plan ${done}/${steps.length} done\n${renderList(steps)}`, 'info');
}

/**
 * Core per-query plan executor — the body of what was the monolithic `execute`.
 * Handles one action (PlanParams) against the given ctx and returns a
 * ToolCallResult. Called from inside `executeQueryBatch` per query.
 */
async function executePlanQuery(p: PlanParams, ctx: PiContext | undefined): Promise<ToolCallResult> {
  const scope = activePlanScope(ctx);
  let steps: PlanStep[];

  // ── Clarify phase (interview) ────────────────────────────────────────────
  if (p.action === 'clarify') {
    const clarifyResult = (text: string, isError = false, extraDetails: Record<string, unknown> = {}): ToolCallResult => ({
      content: [{ type: 'text' as const, text }],
      ...(isError ? { isError: true } : {}),
      details: { action: 'clarify', decisions: getPlanDecisions(scope), ...extraDetails },
    }) as unknown as ToolCallResult;
    const questions = (Array.isArray(p.questions) ? p.questions : []).filter((q) => q && String(q.prompt ?? '').trim()).slice(0, MAX_CLARIFY);
    if (questions.length === 0) {
      return clarifyResult('[PLAN] clarify needs a questions[] list (≤3 high-impact questions the repo cannot answer). Skip clarify for obvious work.', true);
    }
    if (!ctx) {
      return clarifyResult(`[PLAN] this host cannot prompt — ask these inline and continue:\n${questions.map((q, i) => `${i + 1}. ${q.prompt}`).join('\n')}`);
    }
    const recorded: string[] = [];
    let halted: string | undefined;
    let pendingInteraction: AskOutcome['interaction'] | undefined;
    // Use a mutable index so back-navigation can revisit a prior question.
    let qi = 0;
    while (qi < questions.length) {
      const q = questions[qi]!;
      const prompt = String(q.prompt).trim();
      const options = (Array.isArray(q.options) ? q.options : [])
        .map((o) => ({ value: String(o.value ?? o.label ?? '').trim(), label: o.label, description: o.description, recommended: o.recommended, pros: o.pros, cons: o.cons }))
        .filter((o) => o.value);
      // Prepend a back-navigation option for questions after the first.
      const backOption = qi > 0 ? [{ value: '__back__', label: '← Previous question', description: 'go back and change your last answer' }] : [];
      setPlanLifecycle(scope, 'needs_answers');
      setManagedActivity(ctx, { kind: 'awaiting_input', planScope: scope, question: prompt });
      const outcome = await runAskPrompt(ctx, {
        question: prompt,
        options: [...backOption, ...options],
        pagination: questions.length > 1 ? { current: qi + 1, total: questions.length } : undefined,
        freeTextLabel: 'Skip or tell me what to ask differently',
      });
      if (!outcome || outcome.status === 'unavailable') {
        halted = `This host cannot prompt — ask the remaining question(s) inline: ${prompt}`;
        break;
      }
      if (outcome.status === 'pending') {
        pendingInteraction = outcome.interaction;
        halted = `Interaction pending (correlation=${pendingInteraction?.correlationId ?? 'unavailable'}). Wait for the durable host continuation; do not infer an answer.`;
        break;
      }
      if (outcome.status === 'cancelled') { halted = 'Interview cancelled — proceed only with what is already decided.'; break; }
      // Back navigation: remove the previously recorded answer and revisit.
      if (outcome.status === 'selected' && outcome.value === '__back__') {
        const prevPrompt = String(questions[qi - 1]!.prompt).trim();
        // Iterate backwards to avoid requiring findLastIndex.
        let lastIdx = -1;
        for (let k = recorded.length - 1; k >= 0; k--) {
          if (recorded[k]!.startsWith(prevPrompt + ' →')) { lastIdx = k; break; }
        }
        if (lastIdx >= 0) recorded.splice(lastIdx, 1);
        qi -= 1;
        continue;
      }
      const answer = outcome.status === 'text' ? String(outcome.value ?? '').trim() : String(outcome.label ?? outcome.value ?? '').trim();
      if (answer) { addPlanDecision(scope, prompt, answer); recorded.push(`${prompt} → ${answer}`); }
      setPlanLifecycle(scope, 'draft');
      setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail: 'Applying your answer' });
      qi += 1;
    }
    refreshPlanUi(ctx);
    if (recorded.length > 0 && !halted) {
      setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail: 'Applying your answers' });
    }
    const head = recorded.length ? `[PLAN] recorded ${recorded.length} decision(s):\n${recorded.map((r, i) => `${i + 1}. ${r}`).join('\n')}` : '[PLAN] no decisions recorded';
    const tail = halted ? `\n${halted}` : '\nWhen intent + approach are decision-complete, call plan(propose) — the decisions travel with the plan and render on its page.';
    return clarifyResult(`${head}${tail}`, false, pendingInteraction ? {
      pendingInteraction: {
        version: pendingInteraction.version,
        interactionId: pendingInteraction.interactionId,
        correlationId: pendingInteraction.correlationId,
        sessionId: pendingInteraction.sessionId,
      },
      continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
    } : {});
  }

  // ── RFC gate (set/propose only) ──────────────────────────────────────────
  const resolveGate = (): { rfc?: string; hasNewRfc: boolean; error?: ToolCallResult } => {
    const gateError = (text: string): ToolCallResult => ({
      content: [{ type: 'text' as const, text }],
      isError: true,
      details: { action: p.action, error: 'rfc-gate' },
    }) as unknown as ToolCallResult;
    const supplied = typeof p.rfcPath === 'string' ? p.rfcPath.trim() : '';
    if (supplied) {
      const res = resolveRfcPath(planWorkspace(scope), supplied);
      if (res.error) {
        return { hasNewRfc: false, error: gateError(`[PLAN] rfcPath did not resolve: ${res.error}. Point rfcPath at the reviewable RFC under .octocode/rfc/ (the folder or its RFC.md).`) };
      }
      return { rfc: res.path, hasNewRfc: true };
    }
    const existing = getPlanRfc(scope);
    const { consequential: inferred, signals } = inferConsequential(Array.isArray(p.steps) ? p.steps : []);
    const treatConsequential = p.consequential === true || (inferred && p.consequential !== false);
    if (treatConsequential && !existing) {
      const why = p.consequential === true
        ? 'consequential work'
        : `this looks consequential (${signals.join('; ')})`;
      return { hasNewRfc: false, error: gateError(`[PLAN] ${why} needs a reviewable RFC first. Load octocode-rfc-generator, write or update the RFC, then re-call plan with rfcPath pointing at it (…/.octocode/rfc/<name>/RFC.md). If this is genuinely trivial, pass consequential:false with a short reason.`) };
    }
    if (inferred && p.consequential === false) {
      const reason = typeof p.reason === 'string' ? p.reason.trim() : '';
      if (!reason) {
        return { hasNewRfc: false, error: gateError(`[PLAN] this looks consequential (${signals.join('; ')}) but consequential:false was set. To skip the RFC, pass reason:"…" explaining why it's safe; otherwise write an RFC and pass rfcPath.`) };
      }
      addPlanDecision(scope, `Skipped RFC despite consequential signals (${signals.join('; ')})`, reason);
    }
    return { rfc: existing, hasNewRfc: false };
  };

  switch (p.action) {
    case 'set': {
      const gate = resolveGate();
      if (gate.error) return gate.error;
      if (gate.hasNewRfc || p.consequential === true) {
        return {
          content: [{ type: 'text' as const, text: '[PLAN] consequential RFC-backed work must use plan(propose) so the user can review and Accept the exact revision, then separately Start implementation.' }],
          isError: true,
          details: { action: p.action, error: 'review-required' },
        } as unknown as ToolCallResult;
      }
      steps = setPlan(scope, Array.isArray(p.steps) ? p.steps : []);
      configurePlanScope(scope, p.scope);
      if (gate.hasNewRfc) setPlanRfc(scope, gate.rfc);
      ensureUnifiedProjection(scope, p.scope, ctx);
      steps = getPlan(scope);
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    }
    case 'propose': {
      setManagedActivity(ctx, { kind: 'planning', planScope: scope, detail: 'Preparing RFC review' });
      const gate = resolveGate();
      if (gate.error) return gate.error;
      steps = setPlan(scope, Array.isArray(p.steps) ? p.steps : [], 'draft');
      configurePlanScope(scope, p.scope);
      if (gate.hasNewRfc) setPlanRfc(scope, gate.rfc);

      if (gate.rfc) {
        updatePlanCoordination(scope, { mode: 'required', localReason: null });
        const proposed = proposePlanReview(scope);
        if (!proposed.ok) {
          return {
            content: [{ type: 'text' as const, text: `[PLAN] could not enter RFC review: ${proposed.message}` }],
            isError: true,
            details: { action: p.action, error: proposed.code, steps: proposed.steps },
          } as unknown as ToolCallResult;
        }
        steps = proposed.steps;
        const revision = proposed.state.revision!;
        const artifacts = writeCurrentPlanArtifacts(ctx, scope, 'draft');
        refreshPlanUi(ctx);
        setManagedActivity(ctx, { kind: 'reviewing', planScope: scope, revision });
        const summary = buildRfcReviewTldr(scope, steps, revision, artifacts);
        const reviewSurface = ctx
          ? await runAskPrompt(ctx, {
            question: `RFC ready for review · rev ${revision.slice(0, 8)} · ${steps.length} step${steps.length === 1 ? '' : 's'} — how would you like to review it?`,
            headerLabel: PLAN_RFC_REVIEW_HEADER,
            durable: false,
              options: [
                {
                  value: 'browser',
                  label: 'Open in browser',
                  description: 'interactive localhost page with feedback, Accept, and Start buttons',
                  recommended: true,
                  pros: ['live RFC + checklist in one view', 'one-click Accept and Request Changes', 'live-updates as plan evolves'],
                  disabled: planBrowserMessageSender ? false : 'browser-to-agent bridge unavailable in this host',
                },
                {
                  value: 'terminal',
                  label: 'Keep in terminal',
                  description: 'RFC summary and exact follow-up commands appear here in chat',
                  cons: ['no interactive buttons — use /octocode-plan accept or /octocode-plan changes'],
                },
              ],
            })
          : undefined;
        if (reviewSurface?.status === 'selected' && reviewSurface.value === 'browser') {
          const reviewUrl = planBrowserMessageSender ? await servePlanPage(ctx, scope) : undefined;
          if (reviewUrl) {
            const verdict = `[PLAN] browser review opened · rev ${revision.slice(0, 8)} — Accept binds the displayed RFC bytes but does not Start implementation.`;
            const pageNote = artifacts ? `\nPlan doc: ${artifacts.mdPath}` : '\nPlan doc could not be written.';
            return {
              content: [{ type: 'text', text: `${verdict}\n${renderList(steps)}${pageNote}\nInteractive review: ${reviewUrl}` }],
              details: { action: p.action, ...planPresentation(ctx, scope), verdict, revision, reviewUrl, reviewSurface: 'browser' },
            } as unknown as ToolCallResult;
          }
        }

        const selectedSurface = reviewSurface?.status === 'selected' ? reviewSurface.value : 'terminal';
        const verdict = selectedSurface === 'browser'
            ? '[PLAN] browser could not be opened — falling back to the terminal Summary and local files.'
            : '[PLAN] terminal review selected — browser remains closed.';
        return {
          content: [{ type: 'text', text: `${verdict}\n\n${summary}` }],
          details: {
            action: p.action,
            ...planPresentation(ctx, scope),
            verdict,
            revision,
            reviewSurface: 'terminal',
            ...(artifacts ? { artifacts } : {}),
          },
        } as unknown as ToolCallResult;
      }

      const artifacts = writeCurrentPlanArtifacts(ctx, scope, 'draft');
      refreshPlanUi(ctx);
      const outcome = ctx
        ? await runAskPrompt(ctx, {
            question: `${steps.length} step${steps.length === 1 ? '' : 's'} in the panel below — ${PLAN_PROPOSE_HINT}`,
            headerLabel: PLAN_APPROVAL_HEADER,
            options: [
              {
                value: 'approve',
                label: PLAN_APPROVE_LABEL,
                description: PLAN_APPROVE_DESC,
                recommended: true,
                pros: ['begins implementation immediately', 'locks in the exact plan text'],
              },
              {
                value: 'reject',
                label: PLAN_REJECT_LABEL,
                description: PLAN_REJECT_DESC,
                cons: ['agent stops and asks how to proceed'],
              },
            ],
          })
        : undefined;
      const approved = outcome?.status === 'selected' && outcome.value === 'approve';
      if (approved) {
        steps = activatePlan(scope);
        ensureUnifiedProjection(scope, p.scope, ctx);
        steps = getPlan(scope);
        refreshPlanUi(ctx);
      }
      const verdict = (() => {
        if (!outcome || outcome.status === 'unavailable') {
          return '[PLAN] proposed, but this host cannot prompt — present the plan inline and get approval in your reply before executing.';
        }
        if (outcome.status === 'pending') {
          return `[PLAN] approval pending (correlation=${outcome.interaction?.correlationId ?? 'unavailable'}) — do not execute until the durable host continuation records approval.`;
        }
        if (approved) {
          return '[PLAN] approved — begin executing; keep steps updated via start/complete.';
        }
        if (outcome.status === 'text' && outcome.value) {
          return `[PLAN] adjust requested: ${outcome.value}\nRevise the plan and re-propose.`;
        }
        return '[PLAN] rejected — do not execute. Ask the user how to proceed.';
      })();

      let pageNote = artifacts
        ? `\nPlan doc: ${artifacts.mdPath}`
        : '\nPlan doc could not be written — continuing with the in-terminal plan.';
      if (approved) {
        const approvedArtifacts = writeCurrentPlanArtifacts(ctx, scope, 'approved');
        if (approvedArtifacts) pageNote = `\nPlan doc: ${approvedArtifacts.mdPath}`;
        // 3-way surface choice: consistent with the RFC-backed propose flow.
        const reviewSurfaceApproved = ctx?.hasUI && ctx.mode === 'tui'
          ? await runAskPrompt(ctx, {
              question: PLAN_APPROVED_REVIEW_QUESTION,
              headerLabel: PLAN_APPROVED_HEADER,
              options: [
                {
                  value: 'browser',
                  label: 'Open in browser',
                  description: 'serve the plan page on localhost — live-updates as steps progress',
                  recommended: true,
                  pros: ['visual dependency graph', 'live progress as steps complete', 'feedback and discuss from browser'],
                },
                {
                  value: 'chat',
                  label: 'Show summary in chat',
                  description: 'print step count and file paths inline here',
                  pros: ['quick reference without leaving the terminal'],
                },
                {
                  value: 'no',
                  label: 'Not now',
                  description: '/octocode-plan html opens the page anytime later',
                },
              ],
              freeTextLabel: FREE_TEXT_TELL_DIFFERENTLY,
            })
          : undefined;
        if (reviewSurfaceApproved?.status === 'selected' && reviewSurfaceApproved.value === 'browser') {
          const url = await servePlanPage(ctx, scope);
          pageNote = url
            ? `\nLive plan page: ${url} (local server, updates as the plan changes)`
            : '\nCould not start the local plan server — /octocode-plan html retries.';
        } else if (reviewSurfaceApproved?.status === 'selected' && reviewSurfaceApproved.value === 'chat') {
          const tldrLines = [
            `${steps.length} step${steps.length === 1 ? '' : 's'} · approved`,
            ...(approvedArtifacts ? [`Plan markdown: ${approvedArtifacts.mdPath}`, `Plan HTML: ${approvedArtifacts.htmlPath}`] : []),
          ];
          pageNote = `\n${tldrLines.join('\n')}`;
        } else {
          // Not now or no UI: surface paths so the agent can relay them.
          pageNote = approvedArtifacts
            ? `\nPlan doc: ${approvedArtifacts.mdPath}\nPlan HTML: ${approvedArtifacts.htmlPath}\n/octocode-plan html opens a live visual plan page.`
            : '\n/octocode-plan html opens a live visual plan page.';
        }
      }
      return {
        content: [{ type: 'text', text: `${verdict}\n${renderList(steps)}${pageNote}` }],
        details: {
          action: p.action,
          ...planPresentation(ctx, scope),
          verdict,
          ...(outcome?.status === 'pending' && outcome.interaction ? {
            pendingInteraction: {
              version: outcome.interaction.version,
              interactionId: outcome.interaction.interactionId,
              correlationId: outcome.interaction.correlationId,
              sessionId: outcome.interaction.sessionId,
            },
            continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
          } : {}),
        },
      } as unknown as ToolCallResult;
    }
    case 'add':
      steps = addStep(scope, {
        text: String(p.text ?? ''),
        ...(p.activeForm ? { activeForm: p.activeForm } : {}),
        ...(p.dependsOn ? { dependsOn: p.dependsOn } : {}),
        ...(p.paths ? { paths: p.paths } : {}),
        ...(p.taskReasoning ? { reasoning: p.taskReasoning } : {}),
        ...(p.acceptance ? { acceptance: p.acceptance } : {}),
        ...(p.checkCommand ? { checkCommand: p.checkCommand } : {}),
      });
      if (getPlanCoordination(scope).awarenessPlanId) {
        ensureUnifiedProjection(scope, p.scope, ctx);
        steps = getPlan(scope);
      }
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    case 'start':
    case 'complete':
    case 'remove': {
      const current = getPlan(scope);
      const planError = (msg: string, error: string) => ({
        content: [{ type: 'text' as const, text: `${msg}\n${renderList(current)}` }],
        isError: true,
        details: { action: p.action, ...planPresentation(ctx, scope), error },
      }) as unknown as ToolCallResult;
      if (current.length === 0) {
        return planError(`[PLAN] no active plan — nothing to ${p.action}. Use plan set first.`, 'invalid-index');
      }
      if (p.action === 'start' && p.index === undefined && getPlanReviewState(scope).phase === 'accepted') {
        return planError('[PLAN] implementation start requires the operator command /octocode-plan start; a model tool call cannot mint human authorization.', 'authorization-required');
      }
      let idx: number;
      if (p.index === undefined || p.index === null) {
        if (p.action === 'start') {
          idx = current.findIndex((s) => s.status === 'todo' && depsMet(s, current)) + 1;
        } else {
          const doing = current.map((s, i) => ({ step: s, index: i + 1 })).filter(({ step }) => step.status === 'doing');
          if (doing.length > 1) {
            return planError(`[PLAN] ${doing.length} steps are in progress — pass index to ${p.action} a specific lane. Run plan show for indices.`, 'ambiguous-target');
          }
          idx = doing[0]?.index ?? 0;
        }
        if (idx < 1) {
          const why = p.action === 'start'
            ? '[PLAN] no runnable todo step (all done or blocked)'
            : '[PLAN] no step is in progress';
          return planError(`${why} — pass index to target a specific step. Run plan show for indices.`, 'no-target');
        }
      } else {
        idx = Number(p.index);
        if (!Number.isInteger(idx) || idx < 1 || idx > current.length) {
          return planError(`[PLAN] no such step ${p.index} — plan has ${current.length} step(s). Run plan show for indices.`, 'invalid-index');
        }
        if (p.action === 'start' && !depsMet(current[idx - 1]!, current)) {
          return planError(`[PLAN] step ${idx} is blocked by dependencies — complete its prerequisites before starting it.`, 'blocked-step');
        }
      }
      const target = current[idx - 1]!;
      if (p.action === 'remove' && target.awarenessTaskId) {
        return planError('[PLAN] mapped shared steps cannot be removed in place; abandon or revise the shared plan explicitly.', 'shared-remove');
      }
      if (p.action === 'complete' && target.awarenessTaskId) {
        const coordination = getPlanCoordination(scope);
        try {
          const shared = completeUnifiedPlanTask({
            workspace: coordination.coordinationWorkspace || planWorkspace(scope),
            taskId: target.awarenessTaskId,
            agentId: getAwarenessAgentId(ctx),
            ...(p.receipt ? { receipt: p.receipt } : {}),
          });
          if (shared.reopened) {
            return planError(`[PLAN] observed check failed; shared task ${shared.task.taskId} reopened and the local step remains in progress.`, 'check-failed');
          }
        } catch (error) {
          return planError(`[PLAN] shared completion blocked: ${error instanceof Error ? error.message : String(error)}`, 'shared-completion');
        }
      }
      const beforeStart = p.action === 'start' ? current.map((step) => ({ ...step })) : undefined;
      steps = p.action === 'start' ? startStep(scope, idx) : p.action === 'complete' ? completeStep(scope, idx) : removeStep(scope, idx);
      if ((p.action === 'start' || p.action === 'complete') && getPlanCoordination(scope).awarenessPlanId) {
        try {
          ensureUnifiedProjection(scope, p.scope, ctx);
        } catch (error) {
          if (p.action !== 'start' || !beforeStart) throw error;
          restorePlanSteps(scope, beforeStart);
          return planError(`[PLAN] step did not start; local status and Awareness mapping were restored after shared projection failed: ${error instanceof Error ? error.message : String(error)}`, 'shared-start-projection');
        }
        steps = getPlan(scope);
      }
      if (p.action === 'complete' && steps.every((step) => step.status === 'done')) {
        refreshPlanUi(ctx);
        const coordination = getPlanCoordination(scope);
        let verified = true;
        if (coordination.awarenessPlanId) {
          verified = finalizeUnifiedPlan({
            workspace: coordination.coordinationWorkspace || planWorkspace(scope),
            planId: coordination.awarenessPlanId,
          });
        }
        if (verified) finishPlanVerification(scope, true, 'All declared task checks passed');
        else setPlanLifecycle(scope, 'blocked', 'Shared tasks still have verification debt');
        steps = getPlan(scope);
      }
      writeCurrentPlanArtifacts(ctx, scope, 'active');
      break;
    }
    case 'clear': {
      const current = getPlan(scope);
      if (current.some((step) => step.awarenessTaskId) && current.some((step) => step.status !== 'done')) {
        return {
          content: [{ type: 'text' as const, text: '[PLAN] mapped shared plans cannot be cleared while work is unfinished; complete or abandon the shared work first.' }],
          isError: true,
          details: { action: p.action, error: 'shared-clear', ...planPresentation(ctx, scope) },
        } as unknown as ToolCallResult;
      }
      clearPlan(scope);
      tearDownPlanHtml(scope);
      steps = [];
      break;
    }
    case 'show':
    default:
      steps = getPlan(scope);
      break;
  }
  refreshPlanUi(ctx);
  const done = steps.filter((s) => s.status === 'done').length;
  const header = p.action === 'clear' ? '[PLAN] cleared' : `[PLAN] ${done}/${steps.length} done`;
  const artifactHint = (p.action === 'set' || p.action === 'add' || p.action === 'start' || p.action === 'complete' || p.action === 'remove') && steps.length > 0
    ? `\nPlan doc: ${path.join(planArtifactsDir(scope), 'plan.md')}`
    : '';
  // plan(set): offer the browser view via runAskPrompt instead of a text tip.
  // plan(complete) all-done: ask what to do next.
  let lifecycleNote = '';
  if (p.action === 'set' && steps.length > 0 && ctx?.hasUI && ctx.mode === 'tui') {
    const viewBrowser = await runAskPrompt(ctx, {
      question: `${steps.length} step${steps.length === 1 ? '' : 's'} ready — ${PLAN_SET_BROWSER_QUESTION}`,
      headerLabel: PLAN_READY_HEADER,
      options: [
        {
          value: 'yes',
          label: 'Open in browser',
          description: 'serve the plan page on localhost — live-updates as steps change',
          recommended: true,
          pros: ['visual dependency graph', 'live progress tracking', 'feedback and discuss from browser'],
        },
        {
          value: 'no',
          label: 'Not now',
          description: '/octocode-plan html opens the page anytime later',
        },
      ],
      freeTextLabel: FREE_TEXT_TELL_DIFFERENTLY,
    });
    if (viewBrowser?.status === 'selected' && viewBrowser.value === 'yes') {
      const url = await servePlanPage(ctx, scope);
      lifecycleNote = url
        ? `\nLive plan page: ${url} (local server, live-updates as the plan changes)`
        : '\nCould not start the local plan server — /octocode-plan html retries.';
    } else if (viewBrowser?.status === 'text' && viewBrowser.value) {
      lifecycleNote = `\nUser note: ${viewBrowser.value}`;
    } else {
      lifecycleNote = artifactHint
        ? `${artifactHint}\n/octocode-plan html opens a live visual plan page.`
        : '\n/octocode-plan html opens a live visual plan page.';
    }
  } else if (p.action === 'complete' && steps.length > 0 && steps.every((s) => s.status === 'done') && ctx?.hasUI && ctx.mode === 'tui') {
    const decisionCount = getPlanDecisions(scope).length;
    const statsLabel = decisionCount > 0
      ? `${done}/${steps.length} step${steps.length === 1 ? '' : 's'} done · ${decisionCount} decision${decisionCount === 1 ? '' : 's'}`
      : `${done}/${steps.length} step${steps.length === 1 ? '' : 's'} done`;
    const nextStep = await runAskPrompt(ctx, {
      question: `${statsLabel} — ${PLAN_COMPLETE_QUESTION}`,
      headerLabel: PLAN_COMPLETE_HEADER,
      options: [
        {
          value: 'browser',
          label: 'Open plan report in browser',
          description: 'review the completed plan with dependency graph, decisions, and history',
          pros: ['full session history and stats', 'dependency visualization', 'save or share the report'],
        },
        {
          value: 'continue',
          label: 'Continue to next task',
          description: 'tell me what to work on next',
          recommended: true,
          pros: ['immediate next step', 'keeps momentum'],
        },
        {
          value: 'summary',
          label: 'Show summary in chat',
          description: 'get a quick TL;DR of what was accomplished inline here',
        },
      ],
      freeTextLabel: FREE_TEXT_TELL_DIFFERENTLY,
    });
    if (nextStep?.status === 'selected' && nextStep.value === 'browser') {
      const url = await servePlanPage(ctx, scope);
      lifecycleNote = url
        ? `\nCompleted plan page: ${url}`
        : `\nCompleted plan: ${path.join(planArtifactsDir(scope), 'plan.html')}`;
    } else if (nextStep?.status === 'selected' && nextStep.value === 'summary') {
      const summaryLines = [
        `Plan complete · ${done}/${steps.length} step${steps.length === 1 ? '' : 's'} done`,
        ...steps.map((s, i) => `  ${i + 1}. ✓ ${s.text}`),
        ...(decisionCount > 0 ? [`${decisionCount} decision${decisionCount === 1 ? '' : 's'} recorded during planning`] : []),
      ];
      if (artifactHint) summaryLines.push(artifactHint.trimStart());
      lifecycleNote = `\n${summaryLines.join('\n')}`;
    } else if (nextStep?.status === 'text' && nextStep.value) {
      lifecycleNote = `\nUser direction: ${nextStep.value}`;
    }
  }

  const baseNote = lifecycleNote || (p.action !== 'set' ? artifactHint : '');
  return {
    content: [{ type: 'text', text: `${header}\n${renderList(steps)}${baseNote}` }],
    details: { action: p.action, ...planPresentation(ctx, scope) },
  } as unknown as ToolCallResult;
}

export function registerPlanTool(
  pi: {
    registerTool?(def: ToolDefinition): void;
    sendUserMessage?(message: string, options?: { deliverAs?: 'steer' | 'followUp'; expandPromptTemplates?: boolean }): void | Promise<void>;
  },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  planBrowserMessageSender = pi.sendUserMessage
    ? (message) => pi.sendUserMessage!(message, {
        deliverAs: 'followUp',
        ...(message.startsWith('/octocode-plan ') ? { expandPromptTemplates: true } : {}),
      })
    : undefined;
  registerFn(pi, registeredToolNames, {
    name: 'plan',
    label: 'Plan',
    description: [
      'Record and track the task breakdown from the think-first gate as a visible, compaction-durable checklist and reviewable plan document.',
      'The plan is injected at session start (<active_plan>) and re-delivered as a context message when it changes mid-session, so it survives compaction — set it once, then start/complete steps as you go. Mutations also write plan.md/plan.html under the Octocode temp plan directory; when an RFC is linked (rfcPath), the plan.html renders that RFC document itself above the derived checklist and dependency diagram.',
      'Use for non-trivial multi-step work (multiple files/phases/risky edits). Skip for obvious single-step tasks. For consequential work, load octocode-rfc-generator, make the RFC reviewable, then call propose with consequential:true and rfcPath. Propose enters revision-bound review; explicit user Accept records the exact displayed bytes but keeps mutation blocked, and a separate user Start authorizes implementation. A consequential set/propose with no RFC is blocked. Set scope:"shared" for persistent multi-agent execution; Start projects stable steps and dependencies onto Awareness automatically.',
      'Actions: clarify (record bounded material questions) · set (replace steps for already-authorized/obvious work) · propose (enter review; when an RFC is linked, user Accept and user Start are distinct interactions) · add · start (mark an execution-phase step doing; multiple independent steps may be doing in parallel, but this agent-callable action cannot accept an RFC or authorize implementation) · complete · remove · show · clear. The user command `/octocode-plan start` separately starts an accepted current RFC revision.',
      'index is optional for start/complete/remove: complete/remove default to the single current doing step; when multiple steps are doing, pass index. start defaults to the next runnable todo. Completing a mapped shared step requires receipt {command,status,message} from the declared check that actually ran.',
    ].join('\n'),
    promptSnippet: 'Track a durable task checklist (clarify/set/propose/add/start/complete/remove/show/clear).',
    promptGuidelines: [
      'Explore first, ask second: when intent/scope/trade-offs stay open after a research pass, run plan(clarify) with ≤3 high-impact multiple-choice questions the repo can’t answer (mark a recommended default) — each answer is recorded in the durable decision log and renders on the plan page. Skip clarify for obvious work.',
      'When execution is already authorized/obvious, record steps with plan(set). For consequential or preference-dependent work, create a reviewable RFC and call plan(propose) with consequential:true and rfcPath. The user—not the agent—accepts the exact displayed revision, and mutation remains blocked until a separate user Start. The tool auto-detects consequential-looking plans and blocks them without an RFC; consequential:false requires a logged reason. Once executing, keep working: finish the active step and call plan(complete) with no index for the single current step, then continue the next runnable step until the whole plan is done.',
      'Keep the checklist truthful as scope shifts: plan(add) newly discovered document-backed steps, plan(remove) obsolete ones, and plan(clear) once the task is done or abandoned. Shared task projection, ownership, dependencies, check receipts, and finalization are internal to plan; there is no separate public task tool.',
      'For independent lanes, encode ordering with dependsOn, start runnable lanes with plan(start:N) before batching/spawning, and pass explicit indices when completing parallel steps.',
      'Optionally give each step an activeForm (present-continuous label, e.g. "Editing file") — it is shown in the live plan panel while that step runs; propose also shows the full checklist below the editor before the approval prompt. The plan widget/doc should make the flow gate visible: RFC/research → review exact revision → Accept → separate Start → execute → verify.',
      'Plan lifecycle prompts: after plan(set) the tool automatically uses askUser to offer the local browser view (plan.html, live-updating) — do not add a separate askUser call for this. After plan(complete) marks every step done, the tool automatically asks what to do next; if the user says continue, pick up the next task without prompting again. The tool outputs plan.md and plan.html paths in the result — surface them to the user when the askUser prompt is unavailable.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, Type.Object({
      action: Type.Unsafe({ type: 'string', enum: ['set', 'propose', 'clarify', 'add', 'start', 'complete', 'remove', 'clear', 'show'], description: 'Plan lifecycle operation; use the matching action branch and fields.' }),
      scope: Type.Optional(Type.Unsafe({ type: 'string', enum: ['auto', 'session', 'shared'], description: 'Projection policy. auto stays local unless safely adopting existing shared ownership.' })),
      receipt: Type.Optional(Type.Object({
        command: Type.String({ minLength: 1, description: 'The exact declared check command that was actually run.' }),
        status: Type.Unsafe({ type: 'string', enum: ['SUCCESS', 'FAILED'], description: 'Observed check result.' }),
        message: Type.String({ minLength: 1, description: 'Concise observed result, such as test counts or failure cause.' }),
      }, { additionalProperties: false, description: 'For action:complete on shared tasks, the observed check receipt recorded atomically with completion.' })),
      steps: Type.Optional(
        Type.Array(
          Type.Union([
            Type.String(),
            Type.Object({
              text: Type.String(),
              activeForm: Type.Optional(Type.String({ description: 'Present-continuous label shown while this step runs, e.g. "Editing file".' })),
              dependsOn: Type.Optional(Type.Array(Type.Integer({ minimum: 1 }), { description: '1-based indices of steps that must be done first; converted to stable step identities when stored.' })),
              paths: Type.Optional(Type.Array(Type.String(), { description: 'Workspace-relative paths this task may change.' })),
              reasoning: Type.Optional(Type.String({ description: 'Why this task exists or may omit paths.' })),
              acceptance: Type.Optional(Type.String({ description: 'Observable done state for this task.' })),
              checkCommand: Type.Optional(Type.String({ description: 'Command that verifies this task after DONE.' })),
            }),
          ]),
          { minItems: 1, maxItems: 100, description: 'Non-empty replacement checklist for set/propose; strings are shorthand for {text}.' },
        ),
      ),
      text: Type.Optional(Type.String({ description: 'Step text for action:add.' })),
      activeForm: Type.Optional(Type.String({ description: 'Optional present-continuous label for action:add (e.g. "Editing file").' })),
      dependsOn: Type.Optional(Type.Array(Type.Integer({ minimum: 1 }), { description: 'For action:add — 1-based indices of steps that must be done first; converted to stable step identities when stored.' })),
      paths: Type.Optional(Type.Array(Type.String(), { description: 'For action:add — workspace-relative paths this task may change.' })),
      taskReasoning: Type.Optional(Type.String({ description: 'For action:add — why the task exists or may omit paths.' })),
      acceptance: Type.Optional(Type.String({ description: 'For action:add — observable done state.' })),
      checkCommand: Type.Optional(Type.String({ description: 'For action:add — command that verifies the task.' })),
      index: Type.Optional(Type.Integer({ minimum: 1, description: '1-based step number for start/complete/remove. Omit to target the current doing step (complete/remove) or the next runnable todo (start).' })),
      consequential: Type.Optional(Type.Boolean({ description: 'For set/propose: mark the work consequential. When true—or inferred from the steps—a reviewable RFC path is required; acceptance happens inside the user review flow. Pass false only for genuinely trivial work.' })),
      reason: Type.Optional(Type.String({ description: 'Required with consequential:false when the steps still look consequential — a short justification for skipping the RFC (recorded in the decision log).' })),
      rfcPath: Type.Optional(Type.String({ description: 'For set/propose: a reviewable `.octocode/rfc/<name>/` folder or RFC.md. Propose hashes its exact bytes and enters review; the path must stay under the workspace RFC tree.' })),
      questions: Type.Optional(Type.Array(
        Type.Object({
          prompt: Type.String({ description: 'A high-impact question the repo cannot answer (skip anything answerable by reading code).' }),
          options: Type.Optional(Type.Array(Type.Object({
            label: Type.String(),
            value: Type.Optional(Type.String()),
            description: Type.Optional(Type.String()),
            recommended: Type.Optional(Type.Boolean({ description: 'Marks the recommended default; lands the cursor here.' })),
            pros: Type.Optional(Type.Array(Type.String())),
            cons: Type.Optional(Type.Array(Type.String())),
          }), { description: 'Multiple-choice options; omit for a free-text question. A free-text escape is always offered.' })),
        }),
        { minItems: 1, maxItems: 3, description: 'For clarify: 1–3 material questions that repository research cannot answer.' },
      )),
    }, {
      oneOf: [
        { title: 'set', properties: { action: { const: 'set' } }, required: ['action', 'steps'] },
        { title: 'propose', properties: { action: { const: 'propose' } }, required: ['action', 'steps'] },
        { title: 'clarify', properties: { action: { const: 'clarify' } }, required: ['action', 'questions'] },
        { title: 'add', properties: { action: { const: 'add' } }, required: ['action', 'text'] },
        { title: 'start', properties: { action: { const: 'start' } }, required: ['action'] },
        { title: 'complete', properties: { action: { const: 'complete' } }, required: ['action'] },
        { title: 'remove', properties: { action: { const: 'remove' } }, required: ['action'] },
        { title: 'clear', properties: { action: { const: 'clear' } }, required: ['action'] },
        { title: 'show', properties: { action: { const: 'show' } }, required: ['action'] },
      ],
    }), { reasoningDescription: 'Why this plan transition is necessary.' }),

    async execute(toolCallId: string, rawArgs: Record<string, unknown>, signal?: AbortSignal, onUpdate?: (update: ToolCallResult) => void, ctx?: PiContext) {
      return executeQueryBatch({
        toolCallId,
        raw: rawArgs,
        signal,
        onUpdate,
        ctx,
        passthroughSingle: true,
        preflight(query) {
          const action = String(query['action'] ?? '');
          const VALID_ACTIONS: PlanAction[] = ['set', 'propose', 'clarify', 'add', 'start', 'complete', 'remove', 'clear', 'show'];
          if (!VALID_ACTIONS.includes(action as PlanAction)) {
            throw new Error(`unknown plan action: "${action}". Must be one of: ${VALID_ACTIONS.join(', ')}.`);
          }
          assertPlanActionFields(query, action as PlanAction);
          if (query['scope'] !== undefined && !['auto', 'session', 'shared'].includes(String(query['scope']))) {
            throw new Error(`invalid plan scope: ${String(query['scope'])}. Must be auto, session, or shared.`);
          }
          if (action === 'set' || action === 'propose') {
            if (!Array.isArray(query['steps'])) throw new Error(`action:${action} — steps must be an array.`);
            if (query['steps'].length === 0) throw new Error(`action:${action} — steps must not be empty; use action:clear to remove a plan.`);
          }
          if (action === 'add') {
            const text = typeof query['text'] === 'string' ? query['text'].trim() : '';
            if (!text) throw new Error('action:add requires a non-empty text field.');
          }
          if (query['receipt'] !== undefined) {
            const receipt = query['receipt'];
            if (action !== 'complete' || !receipt || typeof receipt !== 'object' || Array.isArray(receipt)) {
              throw new Error('receipt is only valid as an object for action:complete.');
            }
            const record = receipt as Record<string, unknown>;
            if (typeof record['command'] !== 'string' || !record['command'].trim()) throw new Error('receipt.command is required.');
            if (record['status'] !== 'SUCCESS' && record['status'] !== 'FAILED') throw new Error('receipt.status must be SUCCESS or FAILED.');
            if (typeof record['message'] !== 'string' || !record['message'].trim()) throw new Error('receipt.message is required.');
          }
          if ((action === 'start' || action === 'complete' || action === 'remove') && query['index'] != null) {
            const idx = Number(query['index']);
            if (!Number.isInteger(idx) || idx < 1) {
              throw new Error(`action:${action} — index must be a positive integer when provided (got ${String(query['index'])}).`);
            }
          }
        },
        async execute(query, _queryIndex, _itemId, _sig, _upd, queryCtx) {
          return executePlanQuery(query as PlanParams, queryCtx);
        },
        summarize(result, query) {
          const action = String(query['action'] ?? 'unknown');
          const firstLine = (result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text?.split('\n').find(Boolean)?.trim();
          return firstLine ?? (result.isError ? `plan(${action}) failed` : `plan(${action}) ok`);
        },
      });
    },

    renderCall(raw: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(raw, theme, (singleArgs) => {
        const queries = Array.isArray(singleArgs['queries'])
          ? singleArgs['queries'] as Record<string, unknown>[]
          : [];
        const q = (queries[0] ?? {}) as unknown as PlanParams;
        const extra = q.action === 'set' || q.action === 'propose'
          ? ` (${(q.steps ?? []).length} steps)`
          : q.index ? ` #${q.index}` : '';
        return buildToolView({
          name: 'plan',
          state: 'request',
          segments: [
            { text: q.action, token: 'bright' },
            ...(extra ? [{ text: extra.trim().replace(/^\(|\)$/g, ''), token: 'count' as const }] : []),
          ],
        }, theme);
      });
    },


    renderResult(result: ToolCallResult, opts: RenderResultOptions, theme?: PiTheme) {
      // Partial: spinner while the plan operation is executing.
      if (opts.isPartial) {
        return buildToolView(() => ({ name: 'plan', state: 'running', status: CLI_STATUS_TEXT.running }), theme);
      }
      const r = result as ToolCallResult & { details?: { steps?: PlanStep[]; action?: string; results?: unknown[] } };
      // Multi-query batch result: details.results is an array
      if (Array.isArray(r?.details?.results)) {
        const count = r.details!.results!.length;
        return buildToolView({ name: 'plan', state: 'success', segments: [{ text: `${count} operation${count === 1 ? '' : 's'}`, token: 'count' }] }, theme);
      }
      // Single-query passthrough: original shape
      const steps = r?.details?.steps ?? [];
      if (r?.details?.action === 'clear' || steps.length === 0) {
        return buildToolView({ name: 'plan', state: 'success', segments: [{ text: 'cleared', token: 'dim' }] }, theme);
      }
      const done = steps.filter((s) => s.status === 'done').length;
      const current = steps.find((s) => s.status === 'doing') ?? steps.find((s) => s.status === 'todo');
      return buildToolView({
        name: 'plan',
        state: done === steps.length ? 'success' : 'neutral',
        segments: [
          { text: `${done}/${steps.length}`, token: 'count' },
          ...(current ? [{ text: stepLabel(current), token: 'bright' as const }] : []),
        ],
      }, theme);
    },
  });
}
