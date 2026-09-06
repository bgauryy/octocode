/**
 * Tests for the plan's local HTML/Markdown surface — pure builders (mermaid,
 * markdown, page body), artifact writing, and the live-sync toggle.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import {
  buildPlanMermaidFromModel,
  buildPlanMarkdownFromModel,
  buildPlanPageHtmlFromModel,
  writeCurrentPlanArtifacts,
  writePlanReadModelArtifacts,
  enablePlanHtmlSync,
  resetPlanHtmlSync,
  syncCurrentPlanHtmlIfEnabled,
  openPlanHtml,
  setPlanOpenerForTests,
  type RfcDoc,
} from '../src/tools/plan-html.js';
import { setPlan, setPlanRfc, setPlanDecisions, getPlan, completeStep, clearPlan, type PlanStep, type ReviewState } from '../src/tools/active-plan.js';
import { buildPlanReadModel } from '../src/tools/plan-read-model.js';
import { extensionHome } from '../src/extension-paths.js';

const ORIGINAL_HOME = process.env['OCTOCODE_HOME'];
afterEach(() => {
  resetPlanHtmlSync();
  setPlanOpenerForTests(undefined);
  if (ORIGINAL_HOME === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = ORIGINAL_HOME;
});

const STEPS: PlanStep[] = [
  { id: 'schema', text: 'Design schema', status: 'done' },
  { id: 'core', text: 'Build "core" <module>', status: 'doing' },
  { id: 'ship', text: 'Ship it', status: 'todo', dependsOnStepIds: ['schema', 'core'] },
];

function modelFor(steps: PlanStep[], review?: ReviewState, decisions: ReviewState['decisions'] = []) {
  return buildPlanReadModel({
    steps,
    review: review ?? { phase: 'executing', branchSnapshotId: 'html-test', generation: 0, decisions, blockingQuestions: [], comments: [] },
    coordination: { mode: 'local', sourcePlanKey: 'html-test', coordinationWorkspace: '' },
  });
}

const renderMermaid = (steps: PlanStep[]) => buildPlanMermaidFromModel(modelFor(steps));
const renderMarkdown = (steps: PlanStep[], opts: Parameters<typeof buildPlanMarkdownFromModel>[1] = {}, decisions: ReviewState['decisions'] = []) => buildPlanMarkdownFromModel(modelFor(steps, undefined, decisions), opts);
const renderPage = (steps: PlanStep[], rfc?: RfcDoc, decisions: ReviewState['decisions'] = [], review?: ReviewState) => buildPlanPageHtmlFromModel(modelFor(steps, review, decisions), rfc);
const writeModelArtifacts = (scope: string, steps: PlanStep[], opts: Parameters<typeof writePlanReadModelArtifacts>[2] = {}) => {
  const persisted = getPlan(scope).length > 0;
  if (persisted) return writeCurrentPlanArtifacts(undefined, scope, opts);
  return writePlanReadModelArtifacts(scope, modelFor(steps), opts);
};

test('buildPlanMermaidFromModel draws status-classed nodes and dependency edges', () => {
  const m = renderMermaid(STEPS);
  assert.match(m, /^flowchart TD/);
  assert.match(m, /S1\["1\. Design schema"\]:::done/);
  assert.match(m, /S2\[.*:::doing/);
  assert.match(m, /S3\[.*:::blocked/, 'unmet deps render as blocked');
  assert.match(m, /S1 --> S3/);
  assert.match(m, /S2 --> S3/);
  assert.match(m, /#quot;core#quot;/, 'quotes escaped for mermaid labels');
});

test('buildPlanMarkdownFromModel renders flow gates, progress, checkboxes, and the mermaid fence', () => {
  const md = renderMarkdown(STEPS);
  assert.match(md, /Status: active/);
  assert.match(md, /Generated: \d{4}-\d{2}-\d{2}T/);
  assert.match(md, /Progress: 1\/3 done/);
  assert.match(md, /## Flow gates/);
  assert.match(md, /Research current code and contracts with Octocode tools/);
  assert.match(md, /Show the plan overview and ask once: Start implementation or Request changes/);
  assert.match(md, /Use Start to approve the exact RFC revision and begin the first runnable step/);
  assert.match(md, /OCTOCODE_PLAN_CHECKLIST_START/);
  assert.match(md, /- \[x\] 1\. Design schema/);
  assert.match(md, /- \[ \] ▸ 2\..*_\(in progress\)_/);
  assert.match(md, /- \[ \] ⊘ 3\. Ship it.*needs 1, 2/);
  assert.match(md, /```mermaid\nflowchart TD/);
});

test('buildPlanPageHtmlFromModel escapes dynamic checklist text and embeds the diagram', () => {
  const html = renderPage([
    { id: 'lead', text: 'Design schema', status: 'todo', dependsOnStepIds: ['ship', '<script>'] },
    ...STEPS,
  ]);
  assert.match(html, /&lt;module&gt;/, 'HTML in step text is escaped');
  assert.doesNotMatch(html, /<module>/);
  assert.match(html, /needs 4/, 'known stable dependency IDs resolve to current display indices');
  assert.doesNotMatch(html, /<script>/, 'unknown dependency IDs are never rendered');
  assert.match(html, /<details><summary>Planning workflow<\/summary>/);
  assert.match(html, /Show the plan overview and ask once: Start implementation or Request changes/);
  assert.match(html, /<pre class="mermaid">/);
  assert.match(html, /1\/4 done/);
});

// ─── RFC embed (plan page = RFC) ──────────────────────────────────────────────

const RFC_MD = [
  '# RFC: Unify plan and RFC',
  '',
  'Status: Accepted',
  '',
  '## Summary',
  'Embed the RFC in the plan page and <b>render</b> it, ignoring <script>alert(1)</script>.',
  '',
  '| Option | Verdict |',
  '|---|---|',
  '| Unify | chosen |',
  '',
].join('\n');

test('buildPlanPageHtmlFromModel renders a linked RFC as the lead section, sanitized, with a status badge', () => {
  const rfc: RfcDoc = { path: '/ws/.octocode/rfc/unify/RFC.md', markdown: RFC_MD, status: 'Accepted' };
  const html = renderPage(STEPS, rfc);
  // The decision document leads the optional workflow explanation.
  assert.ok(html.indexOf('section class="rfc"') < html.indexOf('<summary>Planning workflow</summary>'), 'RFC section comes first');
  assert.match(html, /rfc-status">Accepted</, 'status badge shown');
  assert.match(html, /<h2>Summary<\/h2>/, 'RFC markdown headings are rendered');
  assert.match(html, /<table>/, 'RFC tables render');
  assert.doesNotMatch(html, /<script>alert/, 'raw script in the RFC is neutralized');
  assert.match(html, /&lt;script&gt;alert/, 'the script is escaped as text');
  assert.doesNotMatch(html, /<b>render<\/b>/, 'inline raw HTML is neutralized too');
  assert.match(html, /Source: \/ws\/\.octocode\/rfc\/unify\/RFC\.md/);
});

test('buildPlanPageHtmlFromModel notes a missing linked RFC instead of failing', () => {
  const html = renderPage(STEPS, { path: '/ws/.octocode/rfc/x/RFC.md', markdown: '', missing: true });
  assert.match(html, /Linked RFC not found: \/ws\/\.octocode\/rfc\/x\/RFC\.md/);
});

test('buildPlanPageHtmlFromModel without an RFC is unchanged (no rfc section)', () => {
  const html = renderPage(STEPS);
  assert.doesNotMatch(html, /section class="rfc"/);
});

test('buildPlanMarkdownFromModel adds an RFC pointer line (link + status), not a duplicate of the RFC', () => {
  const md = renderMarkdown(STEPS, { rfc: { path: '/p/.octocode/rfc/x/RFC.md', markdown: RFC_MD, status: 'Accepted' } });
  assert.match(md, /RFC: \/p\/\.octocode\/rfc\/x\/RFC\.md \(Status: Accepted\)/);
});

test('writeCurrentPlanArtifacts embeds the linked RFC and live-sync re-reads it fresh', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-home-'));
  process.env['OCTOCODE_HOME'] = home;
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'rfc-embed-ws-'));
  const rfcDir = path.join(ws, '.octocode', 'rfc', 'unify');
  fs.mkdirSync(rfcDir, { recursive: true });
  const rfcFile = path.join(rfcDir, 'RFC.md');
  fs.writeFileSync(rfcFile, RFC_MD);
  const scope = ws;
  try {
    setPlan(scope, ['do the work']);
    setPlanRfc(scope, rfcFile);
    const art = writeModelArtifacts(scope, getPlan(scope), { status: 'active', workspace: ws })!;
    const page = fs.readFileSync(art.htmlPath, 'utf8');
    assert.match(page, /section class="rfc"/, 'linked RFC is embedded in the page');
    assert.match(page, /Embed the RFC in the plan page/, 'RFC summary text is rendered');
    const md = fs.readFileSync(art.mdPath, 'utf8');
    assert.match(md, new RegExp(`RFC: ${rfcFile.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')} \\(Status: Accepted\\)`));

    // Live sync: edit the RFC on disk, then a plan mutation refresh must pick it up.
    fs.writeFileSync(rfcFile, RFC_MD + '\n\nUPDATED_RFC_MARKER\n');
    enablePlanHtmlSync(scope);
    syncCurrentPlanHtmlIfEnabled(undefined, scope);
    assert.match(fs.readFileSync(art.htmlPath, 'utf8'), /UPDATED_RFC_MARKER/, 'the open page reflects fresh RFC edits');
  } finally {
    clearPlan(scope);
    fs.rmSync(ws, { recursive: true, force: true });
    fs.rmSync(home, { recursive: true, force: true });
  }
});

// ─── Phase timeline + decisions ───────────────────────────────────────────────

test('buildPlanPageHtmlFromModel renders the phase timeline with the current phase marked', () => {
  // A step in flight → Build is current; Research/RFC/Approve read as done.
  const html = renderPage([{ id: 'a', text: 'a', status: 'doing' }, { id: 'b', text: 'b', status: 'todo' }]);
  assert.match(html, /class="phase-timeline"/);
  assert.match(html, /class="ph now"><span class="ph-g">▸<\/span>Execute/);
  assert.match(html, /class="ph done"><span class="ph-g">✓<\/span>Research/);
  assert.match(html, /class="ph todo"><span class="ph-g">○<\/span>Verify/);
});

test('buildPlanPageHtmlFromModel uses the persisted review phase and revision for smart actions', () => {
  const review: ReviewState = {
    phase: 'in_review',
    branchSnapshotId: 'snapshot',
    generation: 3,
    revision: 'abcdef1234567890',
    decisions: [],
    blockingQuestions: [],
    comments: [],
  };
  const html = renderPage(STEPS, undefined, [], review);
  assert.match(html, /class="ph now"><span class="ph-g">▸<\/span>Review/);
  assert.match(html, /data-plan-action="start" data-revision="abcdef1234567890"/);
  assert.match(html, /Start implementation/);
  assert.match(html, /data-plan-action="changes"/);
  assert.doesNotMatch(html, /Approve revision/);
});

test('accepted recovery state still offers Start without a second approval action', () => {
  const review: ReviewState = {
    phase: 'accepted',
    branchSnapshotId: 'snapshot',
    generation: 4,
    revision: 'abcdef1234567890',
    acceptedRevision: 'abcdef1234567890',
    acceptedAt: new Date().toISOString(),
    decisions: [],
    blockingQuestions: [],
    comments: [],
  };
  const html = renderPage(STEPS, undefined, [], review);
  assert.match(html, /class="ph now"><span class="ph-g">▸<\/span>Accepted/);
  assert.match(html, /data-plan-action="start" data-revision="abcdef1234567890"/);
  assert.match(html, /Start implementation/);
  assert.doesNotMatch(html, /Approve revision/);
});

test('buildPlanPageHtmlFromModel renders a Decisions section only when decisions are present', () => {
  const none = renderPage(STEPS);
  assert.doesNotMatch(none, /<h2>Decisions<\/h2>/);
  const withD = renderPage(STEPS, undefined, [{ q: 'Storage?', a: 'SQLite' }, { q: 'Auth?', a: 'Reuse' }]);
  assert.match(withD, /<h2>Decisions<\/h2>/);
  assert.match(withD, /class="dq">Storage\?<\/span><span class="da">SQLite/);
});

test('buildPlanMarkdownFromModel renders a ## Decisions block when present', () => {
  const md = renderMarkdown(STEPS, {}, [{ q: 'Storage?', a: 'SQLite' }]);
  assert.match(md, /## Decisions/);
  assert.match(md, /- \*\*Storage\?\*\* — SQLite/);
});

test('writeCurrentPlanArtifacts embeds the plan decision log', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-home-'));
  process.env['OCTOCODE_HOME'] = home;
  const scope = '/dec/workspace';
  try {
    setPlan(scope, ['do it']);
    setPlanDecisions(scope, [{ q: 'Which backend?', a: 'SQLite (chosen)' }]);
    const art = writeModelArtifacts(scope, getPlan(scope), { status: 'active' })!;
    assert.match(fs.readFileSync(art.htmlPath, 'utf8'), /<h2>Decisions<\/h2>/);
    assert.match(fs.readFileSync(art.htmlPath, 'utf8'), /Which backend\?/);
    assert.match(fs.readFileSync(art.mdPath, 'utf8'), /## Decisions/);
  } finally {
    clearPlan(scope);
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('openPlanHtml returns a user-visible fallback when the browser opener fails', async () => {
  setPlanOpenerForTests((target) => ({ ok: false, message: `Could not open ${target}` }));
  const result = await openPlanHtml('http://127.0.0.1:1234/plan/');
  assert.equal(result.ok, false);
  assert.match(result.message ?? '', /Could not open/);
  assert.match(result.message ?? '', /127\.0\.0\.1/);
});

test('plan HTML includes a direct, acceptance-aware browser reply widget', () => {
  const html = renderPage(STEPS);
  assert.match(html, /Reply to the agent/);
  assert.match(html, /__octocode\/message/);
  assert.match(html, /Send feedback/);
  assert.match(html, /role="status" aria-live="polite" aria-atomic="true"/);
  assert.match(html, /Connecting to the running agent/);
  assert.match(html, /messageBridge/);
  assert.match(html, /run \/configuration and choose Review plan to reopen the live page/);
  assert.match(html, /Your feedback remains saved/);
  assert.match(html, /const consumesNotes = !action \|\| action === 'changes'/,
    'Start/Accept never clear feedback text they did not send');
  assert.match(html, /white-space:normal; overflow:visible/,
    'handler errors wrap in full instead of being visually truncated');
  assert.doesNotMatch(html, /data-plan-action=/, 'no state-changing action is shown without persisted review state');
});

test('plan HTML preserves long widget content without clipping or source truncation', () => {
  const longText = `Long task ${'0123456789'.repeat(1_200)}`;
  const html = renderPage([{ id: 'long', text: longText, status: 'todo' }]);
  assert.ok(html.includes(longText), 'the entire plan step is present in the UI bytes');
  assert.doesNotMatch(html, /text-overflow\s*:\s*ellipsis|line-clamp|max-height\s*:/i);
  assert.doesNotMatch(html, /\.slice\([^)]*\).*task|task.*\.slice\(/i);
});

test('canonical artifact adapters write html + md under the octocode home; live sync rewrites on change', () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-home-'));
  process.env['OCTOCODE_HOME'] = home;
  const scope = '/some/workspace\0/tmp/session-abc';
  const artifacts = writeModelArtifacts(scope, STEPS, { status: 'draft', workspace: '/some/workspace', generatedAt: new Date('2026-01-02T03:04:05.000Z') })!;
  assert.ok(fs.existsSync(artifacts.htmlPath));
  assert.ok(fs.existsSync(artifacts.mdPath));
  assert.ok(artifacts.htmlPath.startsWith(path.join(home, 'extension', 'tmp', 'plan')), 'html lives under the private extension tmp/plan dir');
  const page = fs.readFileSync(artifacts.htmlPath, 'utf8');
  assert.match(page, /<!doctype html>/);
  assert.doesNotMatch(page, /http-equiv="refresh"/, 'live updates do not blindly reload while the user is reviewing');
  assert.match(page, /name="octocode-refresh-token"/, 'page exposes a stable change token');
  assert.match(page, /setInterval/, 'page polls for a changed token');
  assert.match(page, /activeElement.*textarea/, 'polling defers reload while feedback is being typed');
  assert.match(page, /cdn\.jsdelivr\.net\/npm\/mermaid/);
  const md = fs.readFileSync(artifacts.mdPath, 'utf8');
  assert.match(md, /Status: draft/);
  assert.match(md, /Workspace: \/some\/workspace/);
  assert.match(md, /Generated: 2026-01-02T03:04:05\.000Z/);

  // Distinct scopes never share a file.
  const other = writeModelArtifacts('/other/scope', STEPS)!;
  assert.notEqual(path.dirname(other.htmlPath), path.dirname(artifacts.htmlPath));

  // Live sync: disabled → no rewrite; enabled → rewrite reflects new state.
  fs.rmSync(artifacts.htmlPath);
  syncCurrentPlanHtmlIfEnabled(undefined, scope);
  assert.equal(fs.existsSync(artifacts.htmlPath), false, 'sync is a no-op until armed');
  setPlan(scope, ['Only step']);
  completeStep(scope, 1);
  enablePlanHtmlSync(scope);
  syncCurrentPlanHtmlIfEnabled(undefined, scope);
  assert.match(fs.readFileSync(artifacts.htmlPath, 'utf8'), /1\/1 done/);
  clearPlan(scope);
});

test('writePlanReadModelArtifacts saves session review files privately under the workspace manifest', () => {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-plan-session-'));
  try {
    const artifacts = writeModelArtifacts(workspace, STEPS, { status: 'draft', workspace })!;
    const sessionRoot = extensionHome(process.env['OCTOCODE_HOME']);
    assert.ok(artifacts.htmlPath.startsWith(sessionRoot));
    assert.ok(artifacts.mdPath.startsWith(sessionRoot));
    assert.equal(fs.statSync(artifacts.htmlPath).mode & 0o777, 0o600);
    assert.equal(fs.statSync(artifacts.mdPath).mode & 0o777, 0o600);

    const manifestPath = path.join(path.dirname(path.dirname(artifacts.htmlPath)), 'manifest.json');
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8')) as {
      producers?: { plan?: { paths?: string[] } };
    };
    assert.deepEqual(manifest.producers?.plan?.paths, ['plan/plan.html', 'plan/plan.md']);
  } finally {
    clearPlan(workspace);
    fs.rmSync(workspace, { recursive: true, force: true });
  }
});
