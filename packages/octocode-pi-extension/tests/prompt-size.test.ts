import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import octocode from '../src/index.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

let saved: NodeJS.ProcessEnv;
beforeEach(() => {
  saved = { ...process.env };
  process.env['OCTOCODE_HOME'] = tmp();
  // Hermetic: a run inside a Pi subagent inherits its identity, which changes the tools and the prompt.
  for (const key of ['OCTOCODE_SUBAGENT', 'OCTOCODE_BROWSER_VISIBLE', 'OCTOCODE_AGENT_COLLABORATE']) delete process.env[key];
});
afterEach(() => {
  process.env = saved;
});

interface Surface {
  /** The extension's `octocode` prompt section. */
  prompt: string;
  /** Per active tool: its description, JSON schema and prompt guidelines. */
  tools: Record<string, { description: string; parameters: string; guidelines: string[] }>;
}

/**
 * Extension-owned descriptions, JSON schemas, prompt guidelines and the `octocode` section. This fake host excludes
 * Pi's base prompt/tools, MCP, skill metadata, repository instructions and snippets. `exclude` mirrors a subagent
 * profile's `excludeTools`.
 */
async function surface(options: { subagent: boolean; exclude?: string[] }): Promise<Surface> {
  if (options.subagent) process.env['OCTOCODE_SUBAGENT'] = '1';
  const fake = fakePi();
  Object.assign(fake.pi, { getSessionName: () => undefined });
  await octocode(fake.pi);
  const ctx = fakeCtx({ cwd: tmp(), hasUI: !options.subagent });
  Object.assign(ctx.ui, { onTerminalInput: () => () => undefined });
  await fake.fire('session_start', { type: 'session_start', reason: 'startup' }, ctx);
  const active = fake.pi.getActiveTools().filter((name) => !options.exclude?.includes(name));
  const sections: Record<string, string> = {};
  await fake.fire('before_agent_start', { type: 'before_agent_start', prompt: 'hi', systemPromptOptions: { sections, selectedTools: active } }, ctx);
  const tools: Surface['tools'] = {};
  for (const tool of fake.pi.getAllTools().filter((candidate) => active.includes(candidate.name))) {
    const { description, parameters, promptGuidelines } = tool as { description: string; parameters: unknown; promptGuidelines?: string[] };
    tools[tool.name] = { description, parameters: JSON.stringify(parameters), guidelines: promptGuidelines ?? [] };
  }
  return { prompt: sections['octocode'] ?? '', tools };
}

const size = (s: Surface): number =>
  Object.values(s.tools).reduce((sum, tool) => sum + tool.description.length + tool.parameters.length + tool.guidelines.join('').length, s.prompt.length);

/** Prose sentences of at least `min` characters, each with every place it appears. */
function repeatedSentences(s: Surface, min = 40): Array<[string, string[]]> {
  const seen = new Map<string, string[]>();
  const add = (where: string, text: string) => {
    for (const sentence of text.split(/(?<=[.!?])\s+|\n+/)) {
      const key = sentence.replace(/^[-*\s]+/, '').trim();
      if (key.length >= min) seen.set(key, [...(seen.get(key) ?? []), where]);
    }
  };
  add('prompt', s.prompt);
  for (const [name, tool] of Object.entries(s.tools)) {
    add(`${name}.description`, tool.description);
    tool.guidelines.forEach((guideline, index) => add(`${name}.guidelines[${index}]`, guideline));
  }
  return [...seen].filter(([, places]) => places.length > 1);
}

/**
 * A loose ceiling on the extension-owned surface every request pays for. It is set well above the measured size and
 * is never raised: a prompt that needs more must first lose duplicated or tool-owned text.
 */
const SURFACE_CEILING = 25_000;

describe('prompt surface', () => {
  it('states no sentence twice across the prompt section and the tool descriptions and guidelines', async () => {
    for (const options of [{ subagent: false }, { subagent: true, exclude: ['file', 'browser'] }]) {
      const s = await surface(options);
      expect(s.prompt.length, 'the octocode prompt section is present').toBeGreaterThan(0);
      expect(repeatedSentences(s), JSON.stringify(options)).toEqual([]);
    }
  });

  it("gives a subagent a subset of the main session's tools", async () => {
    const main = Object.keys((await surface({ subagent: false })).tools);
    const child = Object.keys((await surface({ subagent: true })).tools);
    expect(child.length).toBeGreaterThan(0);
    // The main session leaves `browser` to the subagent profiles that carry it (sessionTools); nothing else is child-only.
    expect(child.filter((name) => !main.includes(name) && name !== 'browser')).toEqual([]);
  });

  it('keeps the main session surface under its fixed ceiling', async () => {
    const s = await surface({ subagent: false });
    const total = size(s);
    const perTool = Object.fromEntries(Object.entries(s.tools).map(([name, tool]) => [name, tool.description.length + tool.parameters.length + tool.guidelines.join('').length]));
    expect(total, `main session measured ${total}: ${JSON.stringify(perTool)}`).toBeLessThan(SURFACE_CEILING);
  });
});
