import type { PlanPhase } from './plan-domain.js';
/**
 * plan-html — the plan's local HTML/Markdown surface.
 *
 * `/octocode-plan html` (and plan action:"propose" inside `queries[]`) writes `plan.html` (branded
 * page: status checklist + mermaid dependency diagram + raw markdown) and
 * `plan.md` (shareable) under the global Octocode home
 * (`~/.octocode/tmp/plan/<scope-hash>/`), opens the page, and arms LIVE SYNC:
 * every subsequent plan mutation rewrites both files, and the page's
 * meta-refresh picks the change up — so the user discusses in the terminal
 * while the browser shows the evolving plan. Keying by scope hash keeps parallel
 * sessions/repos from clobbering one another in the shared home dir. First rider
 * on the shared html-page shell; diff previews and worker timelines are designed
 * to reuse the same base.
 */

import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { extensionTmpRoot } from '../extension-paths.js';
import { getPlanRfc, planPhaseIndex, artifactContextForScope, type DisplayStatus, type PlanDecision } from './active-plan.js';
import { getCurrentPlanReadModel, type PlanReadModelV1 } from './plan-read-model.js';
import type { PiContext } from '../types.js';
import { escapeHtml, renderOctocodePage } from '../tui/html-page.js';
import { renderMarkdown } from '../tui/markdown.js';
import { openLocalUrl } from './local-url-opener.js';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/agent-contracts/permissions';

const REFRESH_SECONDS = 3;

// ─── Accepted RFC embed ───────────────────────────────────────────────────────

export interface RfcDoc {
  /** Absolute path to the RFC.md the plan derives from. */
  path: string;
  /** Raw RFC markdown (empty when `missing`). */
  markdown: string;
  /** The RFC's `Status:` header value, when present (Draft/In Review/Accepted/…). */
  status?: string;
  /** The linked RFC file could not be read (deleted/moved after linking). */
  missing?: boolean;
}

/** Value of a plain-text `Field: value` header line near the top of an RFC. */
function rfcHeaderField(markdown: string, field: string): string | undefined {
  const m = markdown.match(new RegExp(`^${field}:\\s*(.+?)\\s*$`, 'mi'));
  return m ? m[1] : undefined;
}

/**
 * Read the RFC linked to a plan scope FRESH from disk (so an open plan page picks
 * up RFC edits on the next mutation/refresh). Returns undefined when no RFC is
 * linked, and a `missing` doc when the linked file can no longer be read.
 */
export function readRfcDoc(scope: string): RfcDoc | undefined {
  const rfcPath = getPlanRfc(scope);
  if (!rfcPath) return undefined;
  try {
    const markdown = fs.readFileSync(rfcPath, 'utf8');
    return { path: rfcPath, markdown, status: rfcHeaderField(markdown, 'Status') };
  } catch {
    return { path: rfcPath, markdown: '', missing: true };
  }
}

const MD_MARK: Record<DisplayStatus, string> = {
  done: '- [x]',
  doing: '- [ ] ▸',
  todo: '- [ ]',
  blocked: '- [ ] ⊘',
};

const FLOW_GATES = [
  'Research current code and contracts with Octocode tools; ask only when a decision is unresolved.',
  'Write or update the RFC and derive dependency-ordered implementation steps.',
  'Show the plan overview and ask once: Start implementation or Request changes.',
  'Use Start to approve the exact RFC revision and begin the first runnable step.',
  'Verify with the command/check named by the approved plan.',
];

/** Escape a label for a quoted mermaid node: `S1["…"]`. */
function mermaidLabel(text: string): string {
  return text.replace(/"/g, '#quot;').replace(/[\n\r]+/g, ' ');
}

export interface PlanMarkdownOptions {
  workspace?: string;
  status?: 'draft' | 'approved' | 'active';
  generatedAt?: Date;
  /** The accepted RFC this plan derives from — linked (not duplicated) in plan.md. */
  rfc?: RfcDoc;
}

/** A one-line RFC pointer for plan.md meta (link + status, or a not-found note). */
function rfcMdMeta(rfc: RfcDoc): string {
  if (rfc.missing) return `RFC: ${rfc.path} (linked file not found)`;
  return `RFC: ${rfc.path}${rfc.status ? ` (Status: ${rfc.status})` : ''}`;
}

/** Pure Markdown projection of the canonical model. */
export function buildPlanMarkdownFromModel(model: PlanReadModelV1, opts: PlanMarkdownOptions = {}): string {
  const rows = model.tasks.map((task) => {
    const label = task.status === 'doing' && task.activeText ? task.activeText : task.text;
    const deps = task.status === 'blocked' && task.dependsOn.length ? ` _(needs ${task.dependsOn.join(', ')})_` : '';
    const doing = task.status === 'doing' ? ' _(in progress)_' : '';
    return `${MD_MARK[task.status]} ${task.index}. ${label}${doing}${deps}`;
  });
  const meta = [
    `Status: ${opts.status ?? 'active'}`,
    opts.workspace ? `Workspace: ${opts.workspace}` : undefined,
    opts.rfc ? rfcMdMeta(opts.rfc) : undefined,
    `Read model: v${model.version}`,
    `Phase: ${model.phase}`,
    model.revision ? `Revision: ${model.revision}` : undefined,
    `Generated: ${(opts.generatedAt ?? new Date()).toISOString()}`,
  ].filter(Boolean) as string[];
  const decisionsBlock = model.review.decisions.length
    ? ['## Decisions', ...model.review.decisions.map((d) => `- **${d.q}** — ${d.a}`), '']
    : [];
  return [
    '# Octocode plan',
    '',
    ...meta,
    `Progress: ${model.summary.done}/${model.summary.total} done`,
    '',
    '## Flow gates',
    ...FLOW_GATES.map((gate, i) => `${i + 1}. ${gate}`),
    '',
    ...decisionsBlock,
    '<!-- OCTOCODE_PLAN_CHECKLIST_START -->',
    ...rows,
    '<!-- OCTOCODE_PLAN_CHECKLIST_END -->',
    '',
    '```mermaid',
    buildPlanMermaidFromModel(model),
    '```',
    '',
  ].join('\n');
}

export function buildPlanMermaidFromModel(model: PlanReadModelV1): string {
  const lines = ['flowchart TD'];
  for (const task of model.tasks) {
    lines.push(`  S${task.index}["${task.index}. ${mermaidLabel(task.text)}"]:::${task.status}`);
  }
  for (const task of model.tasks) for (const dep of task.dependsOn) lines.push(`  S${dep} --> S${task.index}`);
  lines.push('  classDef done fill:#1B4332,stroke:#2EA043,color:#E6EDF3');
  lines.push('  classDef doing fill:#1F3A5F,stroke:#58A6FF,color:#E6EDF3');
  lines.push('  classDef todo fill:#161B22,stroke:#30363D,color:#C9D1D9');
  lines.push('  classDef blocked fill:#161B22,stroke:#F2C14E,color:#F2C14E,stroke-dasharray:4');
  return lines.join('\n');
}

/**
 * The RFC section: the rendered RFC document (sanitized) as the plan page's lead,
 * so the surface the user reviews IS the accepted RFC. Empty string when no RFC is
 * linked; a short note when the linked file has gone missing.
 */
function rfcSectionHtml(rfc: RfcDoc | undefined): string {
  if (!rfc) return '';
  if (rfc.missing) {
    return `<section class="rfc"><h2>RFC</h2><div class="sub">Linked RFC not found: ${escapeHtml(rfc.path)}</div></section>`;
  }
  const badge = rfc.status ? ` <span class="rfc-status">${escapeHtml(rfc.status)}</span>` : '';
  return [
    `<section class="rfc"><h2>RFC${badge}</h2>`,
    `<div class="rfc-body">${renderMarkdown(rfc.markdown)}</div>`,
    `<div class="sub">Source: ${escapeHtml(rfc.path)}</div>`,
    '</section>',
  ].join('\n');
}

/** The horizontal phase timeline — where the plan is in the flow (done ✓ / now ▸ / upcoming ○). */
const REVIEW_PHASE_TIMELINE: ReadonlyArray<{ phase: Exclude<PlanPhase, 'abandoned'>; label: string }> = [
  { phase: 'researching', label: 'Research' },
  { phase: 'needs_answers', label: 'Clarify' },
  { phase: 'draft', label: 'Draft' },
  { phase: 'in_review', label: 'Review' },
  { phase: 'accepted', label: 'Accepted' },
  { phase: 'executing', label: 'Execute' },
  { phase: 'verifying', label: 'Verify' },
  { phase: 'complete', label: 'Complete' },
];

function phaseTimelineHtml(phase: PlanPhase, outcomeReason?: string): string {
  const labels = REVIEW_PHASE_TIMELINE.map((item) => item.label);
  const cur = phase === 'abandoned'
      ? -1
      : planPhaseIndex(phase);
  const items = labels.map((label, i) => {
    const cls = i < cur ? 'done' : i === cur ? 'now' : 'todo';
    const glyph = i < cur ? '✓' : i === cur ? '▸' : '○';
    return `<li class="ph ${cls}"><span class="ph-g">${glyph}</span>${escapeHtml(label)}</li>`;
  });
  const note = phase === 'abandoned'
    ? '<p class="phase-note abandoned">This plan was abandoned.</p>'
    : phase === 'blocked' || phase === 'failed'
      ? `<p class="phase-note abandoned">${escapeHtml(outcomeReason ?? `Plan ${phase}.`)}</p>`
      : `<p class="phase-note">Current state: <strong>${escapeHtml(phase.replace(/_/g, ' '))}</strong></p>`;
  return `<section class="timeline"><h2>Flow</h2><ol class="phase-timeline">${items.join('')}</ol>${note}</section>`;
}

/** The Decisions section — the clarify-phase interview answers that shaped the plan. Empty when none. */
function decisionsSectionHtml(decisions: PlanDecision[] | undefined): string {
  if (!decisions || decisions.length === 0) return '';
  const rows = decisions.map((d) => `<li><span class="dq">${escapeHtml(d.q)}</span><span class="da">${escapeHtml(d.a)}</span></li>`);
  return `<section><h2>Decisions</h2><ul class="decisions">${rows.join('')}</ul></section>`;
}

/** Smart stats section: progress bar + done/total/blocked/running/decision breakdown. */
function planStatsSectionHtml(model: PlanReadModelV1): string {
  const { done, total, running, blocked } = model.summary;
  const todo = Math.max(0, total - done - running - blocked);
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;
  const decisionCount = model.review.decisions.length;
  type Stat = { val: number; label: string; cls: string };
  const stats: Stat[] = [
    { val: done, label: 'done', cls: done === total && total > 0 ? 'stat stat-done stat-all-done' : 'stat stat-done' },
    { val: total, label: 'total', cls: 'stat stat-total' },
    ...(running > 0 ? [{ val: running, label: 'in progress', cls: 'stat stat-running' }] : []),
    ...(blocked > 0 ? [{ val: blocked, label: 'blocked', cls: 'stat stat-blocked' }] : []),
    ...(todo > 0 && (running > 0 || blocked > 0) ? [{ val: todo, label: 'to do', cls: 'stat stat-todo' }] : []),
    ...(decisionCount > 0 ? [{ val: decisionCount, label: decisionCount === 1 ? 'decision' : 'decisions', cls: 'stat stat-decisions' }] : []),
  ];
  const statItems = stats.map((s) =>
    `<div class="${escapeHtml(s.cls)}"><span class="stat-val">${escapeHtml(String(s.val))}</span><span class="stat-label">${escapeHtml(s.label)}</span></div>`,
  ).join('');
  const phaseLabel = model.phase === 'complete' ? 'complete' : model.phase.replace(/_/g, ' ');
  return `<section class="plan-stats">
<style>
  .plan-stats .progress-bar { height:8px; background:var(--line); border-radius:999px; overflow:hidden; margin:0 0 .9rem; }
  .plan-stats .progress-fill { height:100%; background:linear-gradient(90deg,var(--cyan),var(--violet)); border-radius:999px; transition:width .4s ease; }
  .stats-grid { display:flex; flex-wrap:wrap; gap:.55rem; align-items:flex-start; }
  .stat { display:flex; flex-direction:column; align-items:flex-start; padding:.45rem 1.2rem .45rem 0; min-width:3.8rem; }
  .stat-val { font-size:1.45rem; font-weight:850; line-height:1; letter-spacing:-.04em; color:var(--ink); }
  .stat-label { font-size:.68rem; color:var(--muted); text-transform:uppercase; letter-spacing:.09em; margin-top:.18rem; font-weight:650; white-space:nowrap; }
  .stat-done .stat-val { color:var(--cyan); }
  .stat-all-done { color:var(--cyan); }
  .stat-blocked { border-color:rgba(249,115,22,.45); }
  .stat-blocked .stat-val { color:#EA580C; }
  .stat-running .stat-val { color:var(--gold); }
  .stat-decisions .stat-val { color:var(--violet); }
  .phase-badge { display:inline-flex; align-items:center; gap:.35rem; margin-left:.3rem; padding:.28rem .62rem; border-radius:999px; font-size:.72rem; font-weight:700; letter-spacing:.07em; text-transform:uppercase; background:color-mix(in srgb,var(--violet) 12%,transparent); color:var(--violet); border:1px solid color-mix(in srgb,var(--violet) 30%,transparent); }
</style>
<h2>Progress <span class="phase-badge">${escapeHtml(phaseLabel)}</span></h2>
<div class="progress-bar" title="${escapeHtml(pct + '% complete')}"><div class="progress-fill" style="width:${escapeHtml(pct + '%')}"></div></div>
<div class="stats-grid">${statItems}</div>
</section>`;
}

/** Same-origin browser controls that feed review decisions back into the active agent task. */
function browserReplySectionHtml(model: PlanReadModelV1): string {
  const revision = model.revision ?? model.acceptedRevision;
  const contextualActions = model.phase === 'in_review' && revision
    ? `<button type="button" data-reply-command="/octocode-plan start ${escapeHtml(revision)}" class="primary">Start implementation · ${escapeHtml(revision.slice(0, 8))}</button>
    <button type="button" data-reply-command="/octocode-plan changes">Request changes</button>`
    : model.phase === 'accepted' && revision
      ? `<button type="button" data-reply-command="/octocode-plan start ${escapeHtml(revision)}" class="primary">Start implementation</button>\n    <button type="button" data-reply-command="/octocode-plan changes">Reopen review</button>`
      : '';
  const help = model.phase === 'in_review'
    ? 'Start approves the exact displayed revision and begins implementation in one action. You can also request changes or send a note.'
    : model.phase === 'accepted'
      ? 'A previous Start attempt did not complete. Start the already accepted revision again or reopen review.'
      : 'Send a note directly to the running agent task.';
  return `<section class="browser-reply" data-browser-reply>
  <h2>Reply to the agent</h2>
  <p class="reply-help">${escapeHtml(help)}</p>
  <label for="octocode-reply">Feedback</label>
  <textarea id="octocode-reply" maxlength="8000" rows="4" placeholder="What should change, or what should the agent know?"></textarea>
  <div class="reply-actions">
    <button type="button" data-reply-action="send">Send feedback</button>
    ${contextualActions}
  </div>
  <p class="reply-status" role="status" aria-live="polite" aria-atomic="true"></p>
  <style>
    .reply-status[data-state="pending"] { color:var(--muted); }
    .reply-status[data-state="success"] { color:var(--teal); }
    .reply-status[data-state="error"] { color:var(--red); white-space:normal; overflow:visible; }
  </style>
</section>
<script type="module">
(() => {
  const root = document.querySelector('[data-browser-reply]');
  if (!root) return;
  const input = root.querySelector('textarea');
  const status = root.querySelector('.reply-status');
  const buttons = Array.from(root.querySelectorAll('button'));
  const storageKey = 'octocode-plan-reply';
  const endpoint = new URL('__octocode/message', location.href);
  const liveLoopback = location.protocol === 'http:'
    && (location.hostname === '127.0.0.1' || location.hostname === 'localhost' || location.hostname === '::1');
  const setStatus = (message, state = 'idle') => {
    status.textContent = message;
    status.dataset.state = state;
  };
  const setButtonsDisabled = (disabled) => {
    buttons.forEach((button) => { button.disabled = disabled; });
  };
  try { input.value = sessionStorage.getItem(storageKey) || ''; } catch {}
  input.addEventListener('input', () => { try { sessionStorage.setItem(storageKey, input.value); } catch {} });
  if (!liveLoopback) {
    setButtonsDisabled(true);
    setStatus('Interactive actions need the live localhost page. Return to the terminal and run /octocode-plan html; your feedback remains saved here.', 'error');
  } else {
    setButtonsDisabled(true);
    setStatus('Connecting to the running agent…', 'pending');
    void fetch(endpoint, { method: 'GET', cache: 'no-store' }).then(async (response) => {
      if (!response.ok) throw new Error(await response.text());
      const health = await response.json();
      if (!health.messageBridge) throw new Error('browser-to-agent bridge unavailable in this host');
      setButtonsDisabled(false);
      setStatus('Connected to the running agent.', 'success');
    }).catch(() => {
      setButtonsDisabled(true);
      setStatus('The agent bridge is offline. Return to the terminal and run /octocode-plan html to reopen the live page; your feedback remains saved.', 'error');
    });
  }
  const send = async (button) => {
    const notes = input.value.trim();
    const command = button.dataset.replyCommand || '';
    const consumesNotes = !command || command === '/octocode-plan changes';
    const message = command
      ? command === '/octocode-plan changes' && notes ? command + ' ' + notes : command
      : notes;
    if (!message) { setStatus('Write feedback before sending.', 'error'); input.focus(); return; }
    setButtonsDisabled(true);
    setStatus(command ? 'Sending the selected plan action…' : 'Sending feedback…', 'pending');
    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ message }),
      });
      if (!response.ok) throw new Error(await response.text());
      if (consumesNotes) {
        input.value = '';
        try { sessionStorage.removeItem(storageKey); } catch {}
      }
      setStatus(command
        ? 'Plan action sent to the agent. The live page will update when it is applied.' + (!consumesNotes && notes ? ' Your unsent feedback is still in the box.' : '')
        : 'Feedback sent to the agent.', 'success');
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      const networkHint = detail === 'Failed to fetch'
        ? 'The agent bridge went offline. Return to the terminal and run /octocode-plan html to reopen the live page.'
        : detail;
      setStatus('Could not send: ' + networkHint + ' Your feedback remains saved.', 'error');
    } finally {
      if (liveLoopback) setButtonsDisabled(false);
    }
  };
  buttons.forEach((button) => {
    button.addEventListener('click', () => void send(button));
  });
})();
</script>`;
}

/** CSS for the richer step rendering. Scoped to the steps list. */
const STEP_DETAIL_CSS = `
<style>
  ul.steps > li { display:grid; grid-template-columns:1.6rem 1fr; gap:0 .5rem; padding:.65rem 0; margin-bottom:.28rem; border-bottom:1px solid var(--line); }
  ul.steps li .glyph { grid-column:1; grid-row:1; font-size:.9rem; line-height:1.55; }
  ul.steps li.done .glyph { color:var(--cyan); }
  ul.steps li.doing .glyph { color:var(--gold); }
  ul.steps li.blocked .glyph { color:#EA580C; }
  ul.steps li .step-main { grid-column:2; grid-row:1; line-height:1.5; font-size:.92rem; }
  ul.steps li .step-detail { grid-column:2; grid-row:2; margin-top:.35rem; }
  ul.steps li .step-detail summary { font-size:.73rem; color:var(--muted); cursor:pointer; letter-spacing:.04em; text-transform:uppercase; margin-bottom:.35rem; }
  ul.steps li .step-detail .check-cmd { display:block; font-family:monospace; font-size:.78rem; background:var(--surface); border:1px solid var(--line); border-radius:5px; padding:.25rem .5rem; color:var(--cyan); margin-bottom:.25rem; overflow:auto; white-space:pre; }
  ul.steps li .step-detail .acceptance { font-size:.8rem; color:var(--muted); margin:.2rem 0; border-left:2px solid var(--violet); padding-left:.5rem; }
  ul.steps li .step-detail .step-paths { list-style:none; margin:.2rem 0 0; padding:0; }
  ul.steps li .step-detail .step-paths li { font-family:monospace; font-size:.75rem; color:var(--muted); padding:.1rem 0; }
  ul.steps li .deps { font-size:.78rem; color:#EA580C; margin-left:.4rem; }
  .plan-meta { display:flex; flex-wrap:wrap; gap:1.5rem; align-items:center; margin-bottom:1.2rem; }
  .plan-meta-item { display:flex; flex-direction:column; }
  .plan-meta-label { font-size:.65rem; color:var(--muted); text-transform:uppercase; letter-spacing:.09em; font-weight:700; }
  .plan-meta-val { font-family:monospace; font-size:.8rem; color:var(--ink); }
  .plan-meta-val.plan-id-pill { color:var(--violet); font-size:.78rem; font-weight:700; }
  [data-plan-read-model] .steps.gates > li { display:block; }
  [data-plan-read-model] ol.phase-timeline .ph { border:0; border-radius:0; background:none; padding:.25rem .65rem .25rem 0; }
  [data-plan-read-model] ol.phase-timeline .ph.now { box-shadow:none; text-decoration:underline; text-underline-offset:.4rem; }
  [data-plan-read-model] ol.phase-timeline .ph.done { text-decoration:none; }
  [data-plan-read-model] ul.decisions li { border:0; background:none; padding:.4rem 0; }
</style>`;

/** Build the plan identity header: plan ID badge + workspace + session context. */
function planMetaHeaderHtml(model: PlanReadModelV1): string {
  const shortId = model.planId.startsWith('pi-plan-') ? model.planId.slice(8, 16) : model.planId.slice(0, 8);
  const items = [
    `<div class="plan-meta-item"><span class="plan-meta-label">Plan ID</span><span class="plan-meta-val plan-id-pill" title="${escapeHtml(model.planId)}">${escapeHtml(shortId)}…</span></div>`,
    model.coordination.workspace
      ? `<div class="plan-meta-item"><span class="plan-meta-label">Workspace</span><span class="plan-meta-val" title="${escapeHtml(model.coordination.workspace)}">${escapeHtml(shortenPath(model.coordination.workspace))}</span></div>`
      : '',
    model.revision
      ? `<div class="plan-meta-item"><span class="plan-meta-label">Revision</span><span class="plan-meta-val">${escapeHtml(model.revision.slice(0, 8))}</span></div>`
      : '',
    `<div class="plan-meta-item"><span class="plan-meta-label">Steps</span><span class="plan-meta-val">${escapeHtml(String(model.summary.done))}/${escapeHtml(String(model.summary.total))}</span></div>`,
  ].filter(Boolean);
  return `${STEP_DETAIL_CSS}<div class="plan-meta">${items.join('')}</div>`;
}

/** Shorten an absolute path to the last 2 components for display. */
function shortenPath(p: string): string {
  const parts = p.replace(/\\/g, '/').split('/').filter(Boolean);
  return parts.length <= 2 ? p : '\u2026/' + parts.slice(-2).join('/');
}

/** Build a rich step list item with expandable detail (checkCommand, acceptance, paths). */
function stepItemHtml(task: { id: string; index: number; text: string; activeText?: string; status: DisplayStatus; dependsOn: number[]; paths?: string[]; reasoning?: string; acceptance?: string; checkCommand?: string }): string {
  const glyph: Record<DisplayStatus, string> = { done: '\u2713', doing: '\u25b8', todo: '\u25cb', blocked: '\u2298' };
  const label = task.status === 'doing' && task.activeText ? task.activeText : task.text;
  const deps = task.status === 'blocked' && task.dependsOn.length
    ? `<span class="deps">(needs ${task.dependsOn.map(String).map(escapeHtml).join(', ')})</span>`
    : '';
  const hasDetail = task.checkCommand || task.acceptance || (task.paths && task.paths.length > 0);
  const detailHtml = hasDetail ? [
    '<details class="step-detail">',
    '<summary>Details</summary>',
    task.checkCommand ? `<code class="check-cmd">$ ${escapeHtml(task.checkCommand)}</code>` : '',
    task.acceptance ? `<p class="acceptance">${escapeHtml(task.acceptance)}</p>` : '',
    task.paths?.length ? `<ul class="step-paths">${task.paths.map((p) => `<li>${escapeHtml(p)}</li>`).join('')}</ul>` : '',
    '</details>',
  ].filter(Boolean).join('') : '';
  return `<li data-task-id="${escapeHtml(task.id)}" class="${task.status}"><span class="glyph">${glyph[task.status]}</span><span class="step-main">${task.index}. ${escapeHtml(label)}${deps}</span>${detailHtml}</li>`;
}

/** Pure browser projection of the canonical model. */
export function buildPlanPageHtmlFromModel(model: PlanReadModelV1, rfc?: RfcDoc): string {
  const items = model.tasks.map((task) => stepItemHtml(task));
  const gates = FLOW_GATES.map((gate, i) => `<li>${i + 1}. ${escapeHtml(gate)}</li>`);
  const reviewing = ['researching', 'needs_answers', 'draft', 'in_review', 'accepted'].includes(model.phase);
  return [
    // Root section: carries plan-id + revision so the auto-refresh and live-sync
    // can detect stale renders without a full DOM diff.
    `<section data-plan-read-model="${model.version}" data-plan-id="${escapeHtml(model.planId)}" data-revision="${escapeHtml(model.revision ?? '')}">`,
    // Identity header: plan ID, workspace, revision — always visible at a glance.
    planMetaHeaderHtml(model),
    // Phase timeline up top: where the plan sits in the flow at a glance.
    phaseTimelineHtml(model.phase, model.review.outcomeReason),
    // RFC next: the plan the user reviews leads with the accepted decision doc,
    // then the interview decisions, the derived checklist, and dependency flow.
    rfcSectionHtml(rfc),
    decisionsSectionHtml(model.review.decisions),
    planStatsSectionHtml(model),
    reviewing ? browserReplySectionHtml(model) : '',
    // ul.steps (not ol): the stylesheet only resets list-style on ul.steps, so an
    // ol here would stack a browser decimal marker on top of the manual "1." prefix.
    `<section><h2>Steps \u00b7 ${model.summary.done}/${model.summary.total} done</h2><ul class="steps">`,
    ...items,
    '</ul></section>',
    reviewing ? '' : browserReplySectionHtml(model),
    '<details><summary>Planning workflow</summary><ul class="steps gates">',
    ...gates,
    '</ul></details>',
    '<details><summary>Dependency flow</summary>',
    `<pre class="mermaid">${escapeHtml(buildPlanMermaidFromModel(model))}</pre>`,
    '<div class="sub">Diagram needs network once (mermaid CDN); the checklist above always renders.</div>',
    '</details>',
    `<details><summary>Plan markdown</summary><pre>${escapeHtml(buildPlanMarkdownFromModel(model, { ...(rfc ? { rfc } : {}) }))}</pre></details>`,
    '</section>',
  ].filter(Boolean).join('\n');
}

export interface PlanArtifactOptions extends PlanMarkdownOptions {}

export interface PlanArtifacts {
  htmlPath: string;
  mdPath: string;
}

/**
 * Directory for a plan scope's HTML/MD.
 *
 * Primary: `$OCTOCODE_HOME/extension/sessions/<session-key>/plan/` — scoped to the
 * session artifact tree so all tool outputs land under one session root.
 * Fallback: `$OCTOCODE_HOME/extension/tmp/plan/<scope-hash>/` — used when the workspace is
 * not yet initialised or the session artifact dir cannot be created.
 */
export function planArtifactsDir(scope: string): string {
  try {
    return artifactContextForScope(scope).resolve('plan');
  } catch {
    // Fallback: global home keyed by scope hash.
    const hash = createHash('sha256').update(scope || 'default').digest('hex').slice(0, 16);
    return path.join(extensionTmpRoot(), 'plan', hash);
  }
}

/** Stateful artifact entry point: load once, then pass immutable bytes to pure renderers. */
export function writeCurrentPlanArtifacts(ctx: PiContext | undefined, scope: string, opts: PlanArtifactOptions = {}): PlanArtifacts | undefined {
  try {
    return writePlanReadModelArtifacts(scope, getCurrentPlanReadModel(ctx, scope), opts);
  } catch {
    return undefined;
  }
}

/** Write an already-loaded canonical snapshot; browser controls stay bound to these exact bytes. */
export function writePlanReadModelArtifacts(scope: string, model: PlanReadModelV1, opts: PlanArtifactOptions = {}): PlanArtifacts | undefined {
  return writeProjectedPlanArtifacts(scope, model, opts, readRfcDoc(scope));
}

function writeProjectedPlanArtifacts(scope: string, model: PlanReadModelV1, opts: PlanArtifactOptions, rfc?: RfcDoc): PlanArtifacts | undefined {
  try {
    // Create the artifact context ONCE — used for both dir resolution and manifest
    // registration so we pay the dir-walk + manifest lock overhead only one time.
    let artifactCtx: ReturnType<typeof artifactContextForScope> | undefined;
    let dir: string;
    try {
      artifactCtx = artifactContextForScope(scope);
      dir = artifactCtx.resolve('plan');
    } catch {
      // Fallback when workspace is not initialised or session context is absent.
      const hash = createHash('sha256').update(scope || 'default').digest('hex').slice(0, 16);
      dir = path.join(extensionTmpRoot(), 'plan', hash);
    }
    ensurePrivateDirectory(dir);
    const htmlPath = path.join(dir, 'plan.html');
    const mdPath = path.join(dir, 'plan.md');
    const planShortId = model.planId.startsWith('pi-plan-') ? model.planId.slice(8, 16) : model.planId.slice(0, 8);
    const html = renderOctocodePage({
      title: `Octocode plan \u00b7 ${planShortId}`,
      bodyHtml: buildPlanPageHtmlFromModel(model, rfc),
      refreshSeconds: REFRESH_SECONDS,
      refreshToken: `${model.review.branchSnapshotId}:${model.review.generation}:${model.revision ?? ''}:${model.planId}`,
      mermaid: true,
      layout: 'document',
      footerHtml: 'The plan follows this session. Review the proposal, choose Start once, then track work and verification here.',
    });
    const markdown = buildPlanMarkdownFromModel(model, { ...opts, ...(rfc ? { rfc } : {}) });
    if (artifactCtx) {
      // Session artifacts use the shared atomic/private writer. The fallback
      // remains a best-effort global-home projection for pre-session hosts.
      artifactCtx.writeText('plan/plan.html', html);
      artifactCtx.writeText('plan/plan.md', markdown);
    } else {
      fs.writeFileSync(htmlPath, html, { encoding: 'utf8', mode: PRIVATE_FILE_MODE });
      fs.writeFileSync(mdPath, markdown, { encoding: 'utf8', mode: PRIVATE_FILE_MODE });
      hardenPrivateFile(htmlPath);
      hardenPrivateFile(mdPath);
    }
    // Register both files in the session artifact manifest using the already-open context.
    if (artifactCtx) {
      try {
        artifactCtx.registerProducer('plan', 'plan/plan.html');
        artifactCtx.registerProducer('plan', 'plan/plan.md');
      } catch { /* best-effort — never break the plan tool */ }
    }
    return { htmlPath, mdPath };
  } catch {
    return undefined; // Best-effort surface — a write failure must never break the plan tool.
  }
}

// ─── Live sync ────────────────────────────────────────────────────────────────

/**
 * Scope the user opened the page for; mutations rewrite the files while set.
 * One active scope per process: arming a new scope replaces the previous one
 * (the plan tool is single-scope per session). Cleared by resetPlanHtmlSync on
 * plan clear (see plan-tool tearDownPlanHtml).
 */
let liveSyncScope: string | undefined;

export function enablePlanHtmlSync(scope: string): void {
  liveSyncScope = scope;
}

export function resetPlanHtmlSync(): void {
  liveSyncScope = undefined;
}

/** Production live-sync path; canonical state is loaded exactly once. */
export function syncCurrentPlanHtmlIfEnabled(ctx: PiContext | undefined, scope: string): void {
  if (liveSyncScope === undefined) return;
  const workspace = liveSyncScope.split('\0')[0] || liveSyncScope;
  writeCurrentPlanArtifacts(ctx, scope, { status: 'active', workspace });
}

// ─── Opening ──────────────────────────────────────────────────────────────────

export interface PlanOpenResult {
  ok: boolean;
  message?: string;
}

type Opener = (target: string) => PlanOpenResult | Promise<PlanOpenResult>;

const defaultOpener: Opener = async (target) => {
  const result = await openLocalUrl(target);
  return { ok: result.ok, message: result.message };
};

let opener: Opener = defaultOpener;

export function setPlanOpenerForTests(next: Opener | undefined): void {
  opener = next ?? defaultOpener;
}

export async function openPlanHtml(htmlPath: string): Promise<PlanOpenResult> {
  return opener(htmlPath);
}
