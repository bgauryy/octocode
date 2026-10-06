import { describe, expect, it } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { contextBar, footerLines, type FooterState } from '../src/ui/footer.js';

const theme = { fg: (_color: string, text: string) => text, bold: (text: string) => text } as never;
/** Records which colour token painted which text. */
const painted: Array<[string, string]> = [];
const recording = { fg: (color: string, text: string) => (painted.push([color, text]), text), bold: (text: string) => text } as never;

const state: FooterState = {
  model: 'claude-opus-5-5',
  thinking: 'high',
  context: { percent: 42, window: 200_000 },
  usage: { input: 12_400, output: 3_100, cost: 0.84 },
  cwd: '/Users/me/code/app',
  home: '/Users/me',
  sessionName: 'fix login',
};
const statuses = new Map([
  ['mcp', 'mcp 3/3'],
  ['octocode-team', 'agents 1/2 ↑1k ↓2k'],
  ['octocode-bash', '1 bash job'],
  ['octocode-review', 'review on'],
  ['octocode-speed', '58 tok/s'],
  ['other-ext', 'lint ok'],
]);

describe('footer', () => {
  it('shows the model, context, mode and running work first, and where you are second', () => {
    const [top, bottom] = footerLines(state, 'main', statuses, theme, 160);
    expect(top).toMatch(/^claude-opus-5-5 · high · ctx ▰▰▰▱▱▱▱▱ 42% of 200k · review on · agents 1\/2 ↑1k ↓2k · 1 bash job +58 tok\/s · \$0\.84$/);
    expect(bottom).toBe('~/code/app ⎇ main · fix login · ↑12.4k ↓3.1k · mcp 3/3 · lint ok');
    expect(visibleWidth(top!)).toBe(160);
  });

  it('colours context by how full it is', () => {
    expect(contextBar(30)).toEqual({ bar: '▰▰▱▱▱▱▱▱', color: 'success' });
    expect(contextBar(65).color).toBe('warning');
    expect(contextBar(91).color).toBe('error');
    painted.length = 0;
    footerLines({ ...state, context: { percent: 91, window: 200_000 } }, 'main', statuses, recording, 160);
    expect(painted.some(([color, text]) => color === 'error' && text.includes('91%'))).toBe(true);
  });

  it('draws a session name without its escape sequences', () => {
    const [, bottom] = footerLines({ ...state, sessionName: 'fix \u001b]0;pwned\u0007login\u001b[2J' }, null, new Map(), theme, 160);
    expect(bottom).toContain('fix login');
    expect(bottom).not.toContain('\u001b');
  });

  it('says so when context is unknown (right after compaction) and hides empty parts', () => {
    const [top, bottom] = footerLines({ ...state, thinking: undefined, context: { percent: null, window: 200_000 }, usage: { input: 0, output: 0, cost: 0 }, sessionName: undefined }, null, new Map(), theme, 120);
    expect(top).toBe('claude-opus-5-5 · ctx ? of 200k');
    expect(bottom).toBe('~/code/app');
  });

  it('drops the least important parts first on narrow terminals, never the model, context, review or running work', () => {
    const [top, bottom] = footerLines(state, 'feature/a-very-long-branch-name-for-testing', new Map([...statuses, ['other-ext', 'lint ok · 12 warnings in 4 files']]), theme, 75);
    expect(visibleWidth(top!)).toBeLessThanOrEqual(75);
    expect(visibleWidth(bottom!)).toBeLessThanOrEqual(75);
    expect(bottom).toContain('feature/a-very-long-bra…');
    for (const kept of ['claude-opus-5-5', '42%', 'review on', 'agents 1/2', '1 bash job']) expect(top).toContain(kept);
    expect(top).not.toContain('tok/s');
    expect(bottom).not.toContain('lint ok');
    const tiny = footerLines(state, 'main', statuses, theme, 20);
    expect(tiny.every((line) => visibleWidth(line) <= 20)).toBe(true);
  });
});

describe('footer wiring', () => {
  it('replaces Pi\'s footer, sums the session usage and samples context on events', async () => {
    const { fakeCtx, fakePi } = await import('./fake-pi.js');
    const { registerFooter } = await import('../src/ui/footer.js');
    const fake = fakePi();
    (fake.pi as unknown as { getThinkingLevel: () => string }).getThinkingLevel = () => 'medium';
    registerFooter(fake.pi);
    const ctx = fakeCtx({ cwd: '/work/app' }) as unknown as Record<string, any>;
    let percent = 10;
    ctx['model'] = { id: 'model-x', reasoning: true, contextWindow: 100_000 };
    ctx['getContextUsage'] = () => ({ tokens: percent * 1000, contextWindow: 100_000, percent });
    ctx['sessionManager'].getEntries = () => [
      { type: 'message', message: { role: 'assistant', usage: { input: 1_000, output: 200, cost: { total: 0.1 } } } },
      { type: 'message', message: { role: 'toolResult', usage: { input: 500, output: 50, cost: { total: 0.05 } } } },
      { type: 'message', message: { role: 'user' } },
    ];
    await fake.emit('session_start', {}, ctx);
    let renders = 0;
    const data = { getGitBranch: () => 'main', getExtensionStatuses: () => new Map([['mcp', 'mcp 1/1']]), onBranchChange: () => () => undefined };
    const component = ctx['ui'].footer({ requestRender: () => (renders += 1) }, theme, data);
    let [top, bottom] = component.render(120) as string[];
    expect(top).toMatch(/^model-x · medium · ctx ▱{8} 10% of 100k +\$0\.15$/);
    expect(bottom).toBe('/work/app ⎇ main · ↑1.5k ↓250 · mcp 1/1');
    percent = 64;
    await fake.emit('message_end', { message: { role: 'assistant', usage: { input: 100, output: 10, cost: { total: 0.01 } } } }, ctx);
    expect(renders).toBe(1);
    [top] = component.render(120) as string[];
    expect(top).toContain('64%');
    expect(top).toContain('$0.16');
    // A rename (/name, or the automatic title) shows at once.
    let name: string | undefined;
    ctx['sessionManager'].getSessionName = () => name;
    name = 'auth flow';
    await fake.emit('session_info_changed', { type: 'session_info_changed', name }, ctx);
    expect(renders).toBe(2);
    expect((component.render(120) as string[]).join('\n')).toContain('auth flow');
    component.dispose();
    await fake.emit('agent_end', {}, ctx);
    expect(renders).toBe(2);
  });
});
