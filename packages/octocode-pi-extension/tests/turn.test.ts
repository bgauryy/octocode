import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { explicitTools, registerTurnSetup, withFileTool } from '../src/turn.js';
import { fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

describe('withFileTool', () => {
  const base = ['read', 'bash', 'edit', 'write', 'file'];

  it('replaces Pi edit/write with file only when file is active and the user did not choose them', () => {
    expect(withFileTool(base, new Set())).toEqual(['read', 'bash', 'file']);
    expect(withFileTool(['read', 'edit', 'write'], new Set())).toBeUndefined();
    expect(withFileTool(base, new Set(['edit']))).toEqual(['read', 'bash', 'edit', 'file']);
    expect(withFileTool(base, new Set(['edit', 'write']))).toBeUndefined();
    expect(withFileTool(['read', 'file'], new Set())).toBeUndefined();
  });
});

describe('explicitTools', () => {
  it('reads --tools/-t lists and plain or +name defaultTools entries', () => {
    expect([...explicitTools(['node', 'pi', '--tools', 'read, edit', '-p', 'x'], undefined)]).toEqual(['read', 'edit']);
    expect([...explicitTools(['pi', '-t', 'write'], ['+grep', 'bash'])]).toEqual(['write', 'grep', 'bash']);
    expect(explicitTools(['pi', '--tools'], []).size).toBe(0);
  });
});

describe('registerTurnSetup', () => {
  const setup = () => {
    const fake = fakePi();
    for (const name of ['read', 'bash', 'edit', 'write', 'file']) fake.tools.set(name, { name });
    const turn = registerTurnSetup(fake.pi, { isSubagent: false, profiles: () => new Map() });
    return { fake, turn };
  };
  const run = async (fake: ReturnType<typeof fakePi>, selectedTools?: string[]) => {
    const event = { systemPromptOptions: { sections: {} as Record<string, string>, ...(selectedTools ? { selectedTools } : {}) } };
    await fake.emit('before_agent_start', event, {});
    return event.systemPromptOptions.sections['octocode']!;
  };
  const argv = process.argv;
  let agentDir = '';
  // Pi's own mcp.json (in the agent directory) can replace the registration: keep the developer's out of the tests.
  beforeEach(() => {
    agentDir = tmp();
    vi.stubEnv('PI_CODING_AGENT_DIR', agentDir);
  });
  afterEach(() => {
    process.argv = argv;
    vi.unstubAllEnvs();
  });

  it('swaps edit/write for file once at session start and leaves the active set alone per turn', async () => {
    const { fake } = setup();
    process.argv = ['node', 'pi'];
    await fake.emit('session_start', {}, {});
    expect(fake.pi.getActiveTools()).toEqual(['read', 'bash', 'file']);
    const event = { systemPromptOptions: { sections: {} as Record<string, string>, selectedTools: ['read', 'edit'] } };
    await fake.emit('before_agent_start', event, {});
    expect(event.systemPromptOptions.selectedTools).toEqual(['read', 'edit']);
  });

  it('keeps edit/write the user enabled with --tools', async () => {
    const { fake } = setup();
    process.argv = ['node', 'pi', '--tools', 'read,edit,write,file'];
    await fake.emit('session_start', {}, {});
    expect(fake.pi.getActiveTools()).toEqual(['read', 'bash', 'edit', 'write', 'file']);
  });

  it('prefers Octocode MCP from the first turn when it is registered, though its tools connect after the snapshot', async () => {
    const { fake } = setup();
    // Pi snapshots selectedTools before before_agent_start; its MCP host connects Octocode inside that hook.
    const first = await run(fake, ['read']);
    expect(first).toContain('Prefer Octocode MCP (`mcp__octocode__*`)');
    expect(first).toContain('`tool_search`');
    fake.tools.set('mcp__octocode__localSearch', { name: 'mcp__octocode__localSearch' });
    // The section stays byte-identical once the tools appear, so the provider cache holds.
    expect(await run(fake, ['read', 'mcp__octocode__localSearch'])).toBe(first);
  });

  it('declares every Octocode tool directly with OCTOCODE_MCP_DIRECT=1', async () => {
    vi.stubEnv('OCTOCODE_MCP_DIRECT', '1');
    const { fake } = setup();
    const text = await run(fake, ['read']);
    expect(text).toContain('Prefer Octocode MCP');
    expect(text).not.toContain('`tool_search`');
  });

  it('without the built-in server, follows a user-configured octocode server once its tools appear', async () => {
    vi.stubEnv('OCTOCODE_MCP', '0');
    const { fake } = setup();
    expect(await run(fake, ['read'])).not.toContain('Octocode MCP');
    fake.tools.set('mcp__octocode__localSearch', { name: 'mcp__octocode__localSearch' });
    const withTools = await run(fake, ['read']);
    expect(withTools).toContain('Prefer Octocode MCP');
    expect(withTools).not.toContain('`tool_search`');
    // Stable afterwards, even if the selection snapshot misses the tools.
    fake.tools.delete('mcp__octocode__localSearch');
    expect(await run(fake, ['read'])).toBe(withTools);
  });

  it('follows an octocode entry in Pi\'s mcp.json over the registration', async () => {
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'false', enabled: false } } }));
    expect(await run(setup().fake, ['read'])).not.toContain('Octocode MCP');
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'octocode-mcp' } } }));
    const own = await run(setup().fake, ['read']);
    expect(own).toContain('Prefer Octocode MCP');
    // Its exposure is the user's: no claim that GitHub and package registry tools need tool_search.
    expect(own).not.toContain('`tool_search`');
  });

  it('uses available Octocode tools regardless of who registered the server', async () => {
    vi.stubEnv('OCTOCODE_MCP', '0');
    const { fake } = setup();
    expect(await run(fake, ['read', 'mcp__octocode__localSearch'])).toContain('Octocode MCP');
  });
});
