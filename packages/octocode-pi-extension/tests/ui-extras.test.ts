import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  formatCompact,
  formatDurationShort,
  buildWorkingIndicator,
  buildFooterSegments,
  buildShortcutHintsRow,
  buildCommandsRow,
  getFooterDensity,
  setFooterDensity,
  parseFooterDensity,
  resolveSystemTheme,
  resolveSystemThemeName,
  deriveSessionName,
  OCTOCODE_SPINNER_FRAMES,
  OCTOCODE_THEME_DARK,
  OCTOCODE_THEME_LIGHT,
  buildAgentFooterRows,
  type CommandEntry,
} from '../src/ui-extras.js';

test('buildShortcutHintsRow formats bound shortcuts and drops blank/unbound keys', () => {
  const row = buildShortcutHintsRow([
    { key: 'shift+tab', label: 'think' },
    { key: 'ctrl+shift+a', label: 'permissions' },
    { key: '', label: 'palette' },
    { key: 'ctrl+l', label: '' },
  ]);
  assert.equal(row, 'shift+tab think · ctrl+shift+a permissions');
});

test('buildShortcutHintsRow color-codes keycaps and action labels', () => {
  const calls: Array<[string, string]> = [];
  const theme = {
    fg: (color: string, text: string) => {
      calls.push([color, text]);
      return `<${color}:${text}>`;
    },
    bold: (text: string) => `**${text}**`,
  };

  const row = buildShortcutHintsRow([
    { key: 'shift+tab', label: 'think', token: 'link', keyToken: 'brand' },
    { key: 'ctrl+shift+a', label: 'perm', token: 'warning', keyToken: 'warning' },
    { key: 'ctrl+l', label: 'model', token: 'brand', keyToken: 'brandAlt' },
    { key: 'esc', label: 'stop', token: 'error', keyToken: 'error' },
  ], theme);

  assert.equal(row, '**<accent:shift+tab>** <mdLink:think> · **<warning:ctrl+shift+a>** <warning:perm> · **<syntaxOperator:ctrl+l>** <accent:model> · **<error:esc>** <error:stop>');
  assert.deepEqual(calls, [
    ['accent', 'shift+tab'],
    ['mdLink', 'think'],
    ['warning', 'ctrl+shift+a'],
    ['warning', 'perm'],
    ['syntaxOperator', 'ctrl+l'],
    ['accent', 'model'],
    ['error', 'esc'],
    ['error', 'stop'],
  ]);
});

test('buildShortcutHintsRow returns empty string when nothing is bound', () => {
  assert.equal(buildShortcutHintsRow([{ key: '', label: 'think' }, { key: '  ', label: 'perm' }]), '');
});

test('buildCommandsRow renders /name desc for each entry, joined by SEP', () => {
  // Without a theme, paint() is a passthrough, so the output is plain text.
  const entries: CommandEntry[] = [
    { name: 'harness', desc: 'inspect',  token: 'symbol'   },
    { name: 'plan',    desc: 'goal',     token: 'link'     },
    { name: 'agents',  desc: 'workers',  token: 'brandAlt' },
  ];
  const row = buildCommandsRow(entries);
  assert.ok(row.includes('/harness'), 'contains /harness');
  assert.ok(row.includes('inspect'),  'contains harness desc');
  assert.ok(row.includes('/plan'),    'contains /plan');
  assert.ok(row.includes('goal'),     'contains plan desc');
  assert.ok(row.includes('/agents'),  'contains /agents');
  assert.ok(row.includes('workers'),  'contains agents desc');
  // Commands are separated by SEP (· with spaces).
  assert.ok(row.includes('·'), 'uses SEP separator');
});

test('buildCommandsRow returns empty string for empty list', () => {
  assert.equal(buildCommandsRow([]), '');
});

test('buildFooterSegments renders green/red GitHub auth states at every density', () => {
  const base = {
    tokens: 0,
    contextWindow: 0,
    completedTurns: 0,
    sessionMs: 0,
    activeWorkers: 0,
    dirty: false,
  };

  const authenticated = buildFooterSegments({ ...base, githubAuth: 'authenticated' }, 'compact');
  assert.deepEqual(authenticated.find((segment) => segment.text.startsWith('github ')), {
    text: 'github ✓',
    token: 'success',
  });

  const missing = buildFooterSegments({ ...base, githubAuth: 'missing' }, 'default');
  assert.deepEqual(missing.find((segment) => segment.text.startsWith('github ')), {
    text: 'github ✗ login required',
    token: 'error',
    attention: true,
  });

  const failed = buildFooterSegments({ ...base, githubAuth: 'error' }, 'full');
  assert.deepEqual(failed.find((segment) => segment.text.startsWith('github ')), {
    text: 'github check failed',
    token: 'error',
    attention: true,
  });
});

test('buildCommandsRow applies theme colors: dim slash, per-token name, dim desc', () => {
  // TOKEN_FG_MAP: 'dim'→'dim', 'symbol'→'syntaxType', 'brand'→'accent'
  const calls: Array<[string, string]> = [];
  const theme = {
    fg: (color: string, text: string) => { calls.push([color, text]); return `[${color}:${text}]`; },
    bold: (text: string) => text,
  };
  const entries: CommandEntry[] = [
    { name: 'harness', desc: 'inspect',  token: 'symbol' }, // symbol → syntaxType
    { name: 'now',     desc: 'snapshot', token: 'brand'  }, // brand  → accent
  ];
  const row = buildCommandsRow(entries, theme);
  const dimCalls      = calls.filter(([c]) => c === 'dim');
  const symbolCalls   = calls.filter(([c]) => c === 'syntaxType');
  const brandCalls    = calls.filter(([c]) => c === 'accent');
  assert.ok(dimCalls.some(([, t]) => t === '/'),         'slash is dim');
  assert.ok(dimCalls.some(([, t]) => t === 'inspect'),   'harness desc is dim');
  assert.ok(symbolCalls.some(([, t]) => t === 'harness'),'harness name uses symbol → syntaxType');
  assert.ok(brandCalls.some(([, t]) => t === 'now'),     'now name uses brand → accent');
  assert.ok(row.includes('harness'), 'harness appears in row');
});

test('formatCompact abbreviates thousands and millions', () => {
  assert.equal(formatCompact(950), '950');
  assert.equal(formatCompact(1234), '1.2k');
  assert.equal(formatCompact(45_000_000), '45M');
});

test('formatDurationShort renders s / m s / h m', () => {
  assert.equal(formatDurationShort(0), '0s');
  assert.equal(formatDurationShort(12_000), '12s');
  assert.equal(formatDurationShort(63_000), '1m 3s');
  assert.equal(formatDurationShort(3_600_000 + 120_000), '1h 2m');
  assert.equal(formatDurationShort(undefined), '—');
});

test('spinner frames are non-empty and richer than a static pulse', () => {
  assert.ok(OCTOCODE_SPINNER_FRAMES.length >= 6);
  assert.ok(new Set(OCTOCODE_SPINNER_FRAMES).size >= 4);
});

test('buildWorkingIndicator paints spinner frames with a fast semantic color pulse', () => {
  const calls: Array<[string, string]> = [];
  const indicator = buildWorkingIndicator({
    fg: (color: string, text: string) => {
      calls.push([color, text]);
      return `<${color}:${text}>`;
    },
    bold: (text: string) => text,
  });

  assert.equal(indicator.intervalMs, 120);
  assert.equal(indicator.frames.length, OCTOCODE_SPINNER_FRAMES.length);
  // Brand-metallic pulse: teal tick then lavender→white shimmer — never
  // warning/success, which would read as status changes.
  assert.equal(indicator.frames[0], '<accent:✦>');
  assert.equal(indicator.frames[2], '<text:✶>');
  assert.equal(indicator.frames[3], '<mdLink:✺>');
  assert.deepEqual(calls.slice(0, 4), [
    ['accent', '✦'],
    ['mdLink', '✧'],
    ['text', '✶'],
    ['mdLink', '✺'],
  ]);
});

test('buildFooterSegments keeps agent counts separate from per-agent activity rows', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0, activeTurnMs: 0,
    sessionMs: 0, activeWorkers: 2, workerTotal: 3, agentDoing: 'Editing agent-tools.ts', dirty: false,
  }, 'default');
  const seg = segs.find((s) => s.text.startsWith('agents '))!;
  assert.equal(seg.text, 'agents 3 (2 live)');
  assert.doesNotMatch(seg.text, /now:|Editing/);
});

test('buildFooterSegments keeps tracked idle agents visible in the toolbar', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0, activeTurnMs: 0,
    sessionMs: 0, activeWorkers: 0, workerTotal: 2, dirty: false,
  }, 'default');
  const seg = segs.find((s) => s.text.startsWith('agents '))!;
  assert.equal(seg.text, 'agents 2');
});

test('buildFooterSegments composes one actionable context gauge, timing, workers, and git without plan duplication', () => {
  const segs = buildFooterSegments({
    tokens: 16_000, contextWindow: 200_000,
    completedTurns: 3, activeTurnMs: 9000, lastTurnMs: undefined,
    sessionMs: 120_000, activeWorkers: 2, workerTotal: 2, awarenessPeers: 4, peerDirty: 3,
    branch: 'main', dirty: true,
  }, 'default');
  const joined = segs.map((s) => s.text).join(' | ');
  const ctx = segs.find((s) => s.text.startsWith('context '))!;
  assert.match(joined, /8%/);          // 16000/200000
  assert.doesNotMatch(joined, /16\.0k\/200k/, 'default density does not repeat the percentage as an exact ratio');
  assert.match(joined, /context [▓░]{8} 8%/); // visual gauge precedes the percentage
  assert.equal(ctx.token, 'success');
  assert.match(joined, /turn 4 · 9s/); // live: current (4th) turn + elapsed, one segment
  assert.match(joined, /agents 2/);
  assert.doesNotMatch(joined, /peers 4/);
  assert.doesNotMatch(joined, /peer-edits 3/);
  assert.doesNotMatch(joined, /plan \d/);
  assert.match(joined, /main \(dirty\)/);      // dirty marker
  const full = buildFooterSegments({
    tokens: 16_000, contextWindow: 200_000,
    completedTurns: 3, activeTurnMs: 9000, sessionMs: 120_000,
    activeWorkers: 0, dirty: false,
  }, 'full').map((segment) => segment.text).join(' | ');
  assert.match(full, /16\.0k\/200k/, 'full density retains exact token inspection');
});

test('buildFooterSegments never turns unknown context usage into a fake zero-percent measurement', () => {
  const segs = buildFooterSegments({
    tokens: undefined,
    contextWindow: 200_000,
    completedTurns: 0,
    sessionMs: 0,
    activeWorkers: 0,
    dirty: false,
  });
  const body = segs.map((segment) => segment.text).join(' | ');
  assert.doesNotMatch(body, /context|0\/200k|0%/);
});

test('buildFooterSegments always renders labeled harness context total; breakdown only at full density', () => {
  const overhead = { totalChars: 48_000, sysChars: 32_000, mcpServers: 2, mcpTools: 38, skills: 3 };
  const base = {
    tokens: 0, contextWindow: 0, completedTurns: 0, sessionMs: 0,
    activeWorkers: 0, dirty: false, overhead,
  };
  // default: labeled total estimate only (~48000/4 = 12000 → 12.0k)
  const def = buildFooterSegments(base, 'default').map((s) => s.text).join(' | ');
  assert.match(def, /initial ~12\.0k/);
  assert.doesNotMatch(def, /sys /);
  // full: adds the sys/mcp/skills breakdown
  const fullSegments = buildFooterSegments(base, 'full').map((s) => s.text);
  const full = fullSegments.join(' | ');
  assert.match(full, /initial ~12\.0k \(sys 8\.0k · mcp 2\/38 · skills 3\)/);
  assert.equal(fullSegments.filter((text) => text.includes('mcp 2')).length, 1, 'full density renders MCP/skill counts only in the prompt breakdown');
  // compact: diagnostic harness overhead stays behind /octocode-harness
  const compact = buildFooterSegments(base, 'compact').map((s) => s.text).join(' | ');
  assert.doesNotMatch(compact, /initial/);
  assert.doesNotMatch(compact, /sys /);
});

test('buildFooterSegments colors the context gauge by fill severity', () => {
  const base = {
    completedTurns: 0,
    activeTurnMs: 0,
    sessionMs: 0,
    activeWorkers: 0,
    dirty: false,
  };

  assert.equal(buildFooterSegments({ ...base, tokens: 70, contextWindow: 100 })[0]?.token, 'success');
  assert.equal(buildFooterSegments({ ...base, tokens: 75, contextWindow: 100 })[0]?.token, 'warning');
  assert.equal(buildFooterSegments({ ...base, tokens: 90, contextWindow: 100 })[0]?.token, 'error');
});

test('buildFooterSegments flags blocked/failed workers with warning/error colour', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 1,
    activeTurnMs: 1000, sessionMs: 5000,
    activeWorkers: 3, blockedWorkers: 1, failedWorkers: 2,
    dirty: false,
  });
  const blocked = segs.find((s) => s.text.startsWith('blocked '));
  const failed = segs.find((s) => s.text.startsWith('failed '));
  assert.equal(blocked?.text, 'blocked 1');
  assert.equal(blocked?.token, 'warning');
  assert.equal(failed?.text, 'failed 2');
  assert.equal(failed?.token, 'error');
});

test('buildFooterSegments never renders plan state in the bottom toolbar', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0,
    activeTurnMs: 0, sessionMs: 0, activeWorkers: 0,
    dirty: false,
  });
  assert.equal(segs.find((s) => s.text.startsWith('plan ')), undefined);
});

test('buildFooterSegments shows a branded dial segment without depending on plan placement', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0,
    activeTurnMs: 0, sessionMs: 0, activeWorkers: 0,
    dial: 'deep', dirty: false,
  }, 'default');
  const dial = segs.find((s) => s.text.startsWith('dial '))!;
  assert.equal(dial.text, 'dial deep');
  assert.equal(dial.token, 'brand');

  const without = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0,
    activeTurnMs: 0, sessionMs: 0, activeWorkers: 0,
    dirty: false,
  });
  assert.equal(without.find((s) => s.text.startsWith('dial ')), undefined);
});

test('buildFooterSegments omits optional segments cleanly', () => {
  const segs = buildFooterSegments({
    tokens: 0, contextWindow: 0, completedTurns: 0,
    activeTurnMs: undefined, lastTurnMs: 1200, sessionMs: 5000,
    activeWorkers: 0, branch: undefined, dirty: false,
  }, 'default');
  const joined = segs.map((s) => s.text).join(' | ');
  assert.doesNotMatch(joined, /agents/);
  assert.doesNotMatch(joined, /plan \d/);
  assert.match(joined, /last 1s/);
  assert.match(joined, /session 5s/); // session uptime now rides default density
});

test('buildFooterSegments hides turns/timing placeholders before the first turn', () => {
  const segs = buildFooterSegments({
    tokens: 1000, contextWindow: 200_000, completedTurns: 0,
    activeTurnMs: undefined, lastTurnMs: undefined, sessionMs: 0,
    activeWorkers: 0, dirty: false,
  }, 'default');
  const joined = segs.map((s) => s.text).join(' | ');
  assert.match(joined, /context /);
  assert.doesNotMatch(joined, /turns /, 'no `turns 0` placeholder');
  assert.doesNotMatch(joined, /last —/, 'no dataless `last —` placeholder');
  assert.match(joined, /session 0s/, 'uptime always has real data');
});

// ─── footer density modes (review follow-up: reduce duplicate state / noise) ──

const DENSITY_INPUT = {
  tokens: 16_000, contextWindow: 200_000,
  completedTurns: 3, activeTurnMs: 9000, lastTurnMs: undefined,
  sessionMs: 120_000, activeWorkers: 2, agentDoing: 'Editing agent-tools.ts',
  awarenessPeers: 4, blockedWorkers: 1, failedWorkers: 1,
  dial: 'deep', branch: 'main', dirty: true,
};

test('footer density: compact keeps only high-signal segments (ctx, workers, attention flags, git)', () => {
  const joined = buildFooterSegments(DENSITY_INPUT, 'compact').map((s) => s.text).join(' | ');
  assert.match(joined, /context /);
  assert.match(joined, /agents 2/);
  assert.doesNotMatch(joined, /now: /, 'per-worker activity belongs to the dedicated agent rows');
  assert.match(joined, /blocked 1/);
  assert.match(joined, /failed 1/);
  assert.match(joined, /main \(dirty\)/);
  assert.doesNotMatch(joined, /turn/);
  assert.doesNotMatch(joined, /last /);
  assert.doesNotMatch(joined, /session /);
  assert.doesNotMatch(joined, /peers /);
  assert.doesNotMatch(joined, /◉/);
});

test('footer density: default and full omit redundant peer counts', () => {
  const def = buildFooterSegments(DENSITY_INPUT, 'default').map((s) => s.text).join(' | ');
  assert.match(def, /turn 4 · 9s/);
  assert.doesNotMatch(def, /now: /);
  assert.doesNotMatch(def, /peers 4/);
  assert.match(def, /dial deep/);
  assert.match(def, /session 2m 0s/, 'session uptime shows at default density');

  const full = buildFooterSegments(DENSITY_INPUT, 'full').map((s) => s.text).join(' | ');
  assert.match(full, /session 2m 0s/);
  assert.match(full, /turn 4 · 9s/);
  assert.doesNotMatch(full, /now: /);
  assert.doesNotMatch(full, /peers 4/);
  assert.match(full, /dial deep/);
});

test('footer density: module-level mode drives the default parameter; parse rejects junk', () => {
  try {
    assert.equal(getFooterDensity(), 'compact');
    setFooterDensity('compact');
    assert.equal(getFooterDensity(), 'compact');
    const joined = buildFooterSegments(DENSITY_INPUT).map((s) => s.text).join(' | ');
    assert.doesNotMatch(joined, /turns/, 'implicit density follows the module mode');
  } finally {
    setFooterDensity('default');
  }
  assert.equal(parseFooterDensity('full'), 'full');
  assert.equal(parseFooterDensity(' Compact '), 'compact');
  assert.equal(parseFooterDensity('bogus'), undefined);
  assert.equal(parseFooterDensity(''), undefined);
});

test('theme name constants are the shipped theme ids', () => {
  assert.equal(OCTOCODE_THEME_DARK, 'octocode-dark');
  assert.equal(OCTOCODE_THEME_LIGHT, 'octocode-light');
});

test('resolveSystemTheme maps macOS appearance to our themes', () => {
  assert.equal(resolveSystemTheme('Dark'), OCTOCODE_THEME_DARK);
  assert.equal(resolveSystemTheme(''), OCTOCODE_THEME_LIGHT);   // AppleInterfaceStyle unset => light
  assert.equal(resolveSystemTheme(null), OCTOCODE_THEME_LIGHT);
});

test('resolveSystemThemeName: macOS resolves from AppleInterfaceStyle (always decidable)', () => {
  assert.equal(resolveSystemThemeName({ platform: 'darwin', appleInterfaceStyle: 'Dark' }), OCTOCODE_THEME_DARK);
  assert.equal(resolveSystemThemeName({ platform: 'darwin', appleInterfaceStyle: '' }), OCTOCODE_THEME_LIGHT);
  assert.equal(resolveSystemThemeName({ platform: 'darwin', appleInterfaceStyle: undefined }), OCTOCODE_THEME_LIGHT);
});

test('resolveSystemThemeName: non-macOS uses COLORFGBG background heuristic', () => {
  // COLORFGBG is "fg;bg"; bg 0-6 = dark terminal, 7/15 = light.
  assert.equal(resolveSystemThemeName({ platform: 'linux', colorfgbg: '15;0' }), OCTOCODE_THEME_DARK);
  assert.equal(resolveSystemThemeName({ platform: 'linux', colorfgbg: '0;15' }), OCTOCODE_THEME_LIGHT);
  assert.equal(resolveSystemThemeName({ platform: 'linux', colorfgbg: '0;7' }), OCTOCODE_THEME_LIGHT);
});

test('resolveSystemThemeName: returns null when undetectable (keep current theme)', () => {
  assert.equal(resolveSystemThemeName({ platform: 'linux' }), null);
  assert.equal(resolveSystemThemeName({ platform: 'win32', colorfgbg: 'garbage' }), null);
  assert.equal(resolveSystemThemeName({ platform: 'linux', colorfgbg: '' }), null);
});

test('deriveSessionName cleans and truncates the first line', () => {
  assert.equal(deriveSessionName('  Fix the auth  bug\nmore'), 'Fix the auth bug');
  assert.equal(deriveSessionName(''), '');
  const long = deriveSessionName('x'.repeat(80));
  assert.ok(long.length <= 48);
  assert.match(deriveSessionName('add feature'.repeat(20)), /…$/);
});

test('buildFooterSegments always shows the permission mode, color-coded by risk', () => {
  const base = { tokens: 0, contextWindow: 0, completedTurns: 0, sessionMs: 0, activeWorkers: 0, dirty: false };
  const def = buildFooterSegments({ ...base, permissionLevel: 'default' })
    .find((s) => s.text === 'perm default')!;
  assert.equal(def.token, 'dim', 'default mode is always visible, calmly dim');
  const relaxed = buildFooterSegments({ ...base, permissionLevel: 'relaxed' }, 'compact')
    .find((s) => s.text === 'perm relaxed')!;
  assert.equal(relaxed.token, 'warning', 'loosened gate paints warning, even in compact');
  const strict = buildFooterSegments({ ...base, permissionLevel: 'strict' })
    .find((s) => s.text === 'perm strict')!;
  assert.equal(strict.token, 'dim');
});

test('buildFooterSegments shows always-allow grant count with and without a level', () => {
  const base = { tokens: 0, contextWindow: 0, completedTurns: 0, sessionMs: 0, activeWorkers: 0, dirty: false };
  const grants = buildFooterSegments({ ...base, permissionLevel: 'default', approvedClassCount: 2 })
    .find((s) => s.text.startsWith('perm'))!;
  assert.equal(grants.text, 'perm default +2');
  assert.equal(grants.token, 'dim');
  const both = buildFooterSegments({ ...base, permissionLevel: 'relaxed', approvedClassCount: 1 })
    .find((s) => s.text.startsWith('perm'))!;
  assert.equal(both.text, 'perm relaxed +1');
  assert.equal(both.token, 'warning');
});

test('buildFooterSegments marks only act-on-me segments for attention', () => {
  const segs = buildFooterSegments({
    tokens: 95, contextWindow: 100, completedTurns: 2, activeTurnMs: 500, sessionMs: 5000,
    activeWorkers: 2, workerTotal: 2, blockedWorkers: 1, failedWorkers: 1, awarenessUnread: 3,
    dial: 'deep', permissionLevel: 'relaxed', branch: 'main', dirty: false,
  });
  const emphasized = segs.filter((s) => s.attention).map((s) => s.text.replace(/\d+.*$/, '').trim());
  assert.deepEqual(emphasized.sort(), ['context ▓▓▓▓▓▓▓▓', 'mail', 'blocked', 'failed'].map((s) => s.replace(/\d+.*$/, '').trim()).sort());
  // Steady states are never emphasized — bold must always mean "act on me".
  for (const s of segs) {
    if (!s.attention) continue;
    assert.ok(['warning', 'error', 'link'].includes(s.token ?? ''), `${s.text} emphasis rides an attention token`);
  }
  const calm = buildFooterSegments({
    tokens: 10, contextWindow: 100, completedTurns: 2, activeTurnMs: 500, sessionMs: 5000,
    activeWorkers: 1, workerTotal: 1, dial: 'deep', permissionLevel: 'relaxed', branch: 'main', dirty: false,
  });
  assert.equal(calm.some((s) => s.attention), false, 'no attention state → nothing is emphasized');
});


test('buildFooterSegments appends the changed-file delta to a dirty branch', () => {
  const base = { tokens: 0, contextWindow: 0, completedTurns: 1, lastTurnMs: 800, sessionMs: 1000, activeWorkers: 0 };
  const dirty = buildFooterSegments({ ...base, branch: 'main', dirty: true, dirtyFiles: 5 });
  assert.equal(dirty.at(-1)!.text, 'main (5 changed)');
  const clean = buildFooterSegments({ ...base, branch: 'main', dirty: false, dirtyFiles: 0 });
  assert.equal(clean.at(-1)!.text, 'main');
});

test('buildAgentFooterRows: one row per subagent, live first, state colour + elapsed + activity', () => {
  const t0 = Date.parse('2026-08-22T10:00:00Z');
  const { rows } = buildAgentFooterRows([
    { agentId: 'abcdef123', name: 'researcher', status: 'done', startedAt: new Date(t0).toISOString(), updatedAt: new Date(t0 + 5000).toISOString() },
      { agentId: '123456789', name: 'builder', status: 'running', model: 'gpt-5.6', task: 'Build footer components', planStep: 'Render agent rows', startedAt: new Date(t0).toISOString(), updatedAt: new Date(t0 + 1000).toISOString(), deltaSummary: 'editing src/index.ts' },
    { agentId: 'fffff0000', name: 'tester', status: 'blocked', startedAt: new Date(t0).toISOString(), updatedAt: new Date(t0 + 2000).toISOString() },
  ], t0 + 14_000);
  assert.deepEqual(rows.map((r) => r.label), ['agent tester (fffff0)', 'agent builder (123456)', 'agent researcher (abcdef)']);
  assert.deepEqual(rows.map((r) => r.state), ['blocked', 'running', 'done']);
  assert.deepEqual(rows.map((r) => r.token), ['warning', 'brand', 'success']);
  assert.deepEqual(rows.map((r) => r.attention), [true, false, false]);
  assert.equal(rows[1]!.elapsed, '14s', 'live worker elapsed runs against now');
  assert.equal(rows[2]!.elapsed, '5s', 'finished worker elapsed is frozen at its last update');
    assert.equal(rows[1]!.doing, 'editing src/index.ts');
    assert.equal(rows[1]!.model, 'gpt-5.6');
    assert.equal(rows[1]!.task, 'Build footer components');
    assert.equal(rows[1]!.planStep, 'Render agent rows');
  assert.equal(rows[2]!.doing, undefined, 'finished workers carry no live activity');
  const communicating = buildAgentFooterRows([
    {
      agentId: 'feedface1',
      name: 'worker',
      status: 'running',
      startedAt: new Date(t0).toISOString(),
      updatedAt: new Date(t0 + 3000).toISOString(),
      pendingMessages: 2,
      lastMessage: {
        direction: 'to-agent',
        action: 'follow-up',
        preview: 'check the remaining callers',
        timestamp: t0 + 2500,
      },
      activeTool: 'MCPTool',
      toolCallCount: 3,
      toolNames: ['MCPTool', 'bash'],
      deltaSummary: 'checking references',
    },
  ], t0 + 6000);
  assert.match(communicating.rows[0]!.doing ?? '', /msg→ follow-up \(2 queued\): check/);
  assert.match(communicating.rows[0]!.doing ?? '', /tool MCPTool/);
  assert.match(communicating.rows[0]!.doing ?? '', /checking refe/);
  const replied = buildAgentFooterRows([
    {
      agentId: 'c0ffee123',
      name: 'reviewer',
      status: 'idle',
      startedAt: new Date(t0).toISOString(),
      updatedAt: new Date(t0 + 5000).toISOString(),
      lastMessage: {
        direction: 'from-agent',
        action: 'reply',
        preview: '[DONE] review complete',
        timestamp: t0 + 5000,
      },
    },
  ], t0 + 6000);
  assert.match(replied.rows[0]!.doing ?? '', /msg← reply: \[DONE\] review complete/);
  const normalized = buildAgentFooterRows([
    {
      agentId: 'deadbeef1',
      name: 'architect',
      status: 'idle',
      normalizedStatus: 'done',
      startedAt: new Date(t0).toISOString(),
      updatedAt: new Date(t0 + 5000).toISOString(),
      deltaSummary: '[DONE] architecture handback ready',
    },
    {
      agentId: 'baadf00d1',
      name: 'reviewer',
      status: 'idle',
      normalizedStatus: 'blocked',
      startedAt: new Date(t0).toISOString(),
      updatedAt: new Date(t0 + 6000).toISOString(),
      deltaSummary: '[BLOCKED] waiting on answer',
    },
  ], t0 + 60_000);
  assert.deepEqual(normalized.rows.map((row) => row.state), ['blocked', 'done']);
  assert.equal(normalized.rows[1]!.elapsed, '5s', 'normalized done freezes elapsed instead of looking idle for an hour');
  assert.equal(normalized.rows[1]!.token, 'success');
  const many = buildAgentFooterRows(Array.from({ length: 6 }, (_, i) => ({
    agentId: `id${i}`, name: `w${i}`, status: 'running', startedAt: new Date(t0).toISOString(), updatedAt: new Date(t0 + i).toISOString(),
  })), t0);
            assert.equal(many.rows.length, 6);
          });
