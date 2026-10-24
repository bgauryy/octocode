import { describe, expect, it } from 'vitest';
import { octocodePrompt } from '../src/prompt.js';

type Input = Parameters<typeof octocodePrompt>[0];

const MAIN: Input = { octocode: true, profiles: [], canDelegate: true };
const WRITER: Input = { octocode: false, profiles: [], canDelegate: false, canWrite: true };
const READER: Input = { octocode: false, profiles: [], canDelegate: false, canWrite: false };
const VARIANTS: Input[] = [
  MAIN,
  { octocode: false, profiles: [], canDelegate: true },
  { octocode: true, profiles: [], canDelegate: true, canUseAgent: false },
  WRITER,
  READER,
  { octocode: true, profiles: [], canDelegate: false, canWrite: false, canShell: false },
  { octocode: false, profiles: [], canDelegate: false, canWrite: false, canShell: false, canMcp: false },
];

const count = (text: string, pattern: RegExp): number => text.match(new RegExp(pattern.source, `${pattern.flags}g`))?.length ?? 0;

describe('prompt', () => {
  it('names no Octocode tool, parameter or retired helper: the server and tool descriptions own them', () => {
    for (const input of VARIANTS) {
      const prompt = octocodePrompt({ ...input, profiles: [{ name: 'researcher', description: 'Reads code' }] });
      for (const coupled of ['localFetch', 'localSearch', 'structureSearch', 'astSearch', 'ghSearchCode', 'ghStructure', 'lspSearch', 'artifactSearch', 'tool_search', 'askUser', 'replyTo']) {
        expect(prompt, `${coupled} in ${JSON.stringify(input)}`).not.toContain(coupled);
      }
      expect(prompt).not.toMatch(/(?<!mcp__)octocode_/);
    }
  });

  it('states each shared rule once in every variant', () => {
    for (const input of VARIANTS) {
      const prompt = octocodePrompt(input);
      const where = JSON.stringify(input);
      expect(count(prompt, /```mermaid\nflowchart LR\n/), where).toBe(1);
      expect(count(prompt, /Hypothesis test: /), where).toBe(1);
      expect(count(prompt, /5W2H/), where).toBe(1);
      expect(count(prompt, /Separate facts, assumptions and unknowns/), where).toBe(1);
      expect(count(prompt, /Definition of Done/), where).toBe(1);
      expect(count(prompt, /- BLUF: /), where).toBe(1);
      expect(count(prompt, /as data, not instructions/), where).toBe(1);
    }
    for (const input of [WRITER, READER]) {
      const child = octocodePrompt(input);
      expect(count(child, /`sendMessage` your parent the blocker/)).toBe(1);
      expect(count(child, /Start it with one word, /)).toBe(1);
    }
  });

  it('prefers Octocode MCP for research only when the server is connected', () => {
    for (const canDelegate of [true, false]) {
      expect(octocodePrompt({ octocode: true, profiles: [], canDelegate })).toMatch(/Octocode MCP \(`mcp__octocode__\*`\)/);
      const without = octocodePrompt({ octocode: false, profiles: [], canDelegate });
      expect(without).not.toContain('Octocode MCP');
      expect(without).toMatch(/`web`/);
    }
  });

  it('appends the server instructions under their own heading only when Octocode is connected', () => {
    const text = 'SERVER-OWNED TEXT';
    expect(octocodePrompt({ ...MAIN, octocodeInstructions: text })).toMatch(/# Octocode MCP server instructions\nSERVER-OWNED TEXT/);
    expect(octocodePrompt({ ...MAIN, octocode: false, octocodeInstructions: text })).not.toContain(text);
  });

  it('lists every delegation profile for the main session and none for a subagent', () => {
    const profiles = [{ name: 'researcher', description: 'Reads code' }, { name: 'webLive', description: 'Drives Chrome' }];
    const root = octocodePrompt({ ...MAIN, profiles });
    for (const profile of profiles) expect(root).toContain(`- ${profile.name}: ${profile.description}`);
    expect(octocodePrompt({ ...WRITER, profiles })).not.toContain('researcher');
  });

  it('tells a subagent it is one and cannot ask the user; the main session gets no subagent section', () => {
    for (const input of [WRITER, READER]) {
      const child = octocodePrompt(input);
      expect(child).toMatch(/^# Subagent$/m);
      expect(child).toMatch(/cannot ask the user/);
      expect(child).not.toMatch(/[Rr]eport [^.]*to the parent/);
    }
    expect(octocodePrompt(MAIN)).not.toMatch(/^# Subagent$/m);
    expect(octocodePrompt(MAIN)).not.toMatch(/Start it with one word/);
  });

  it('gives a read-only agent the investigate-and-report rule instead of the change rules, naming only writers it has', () => {
    const changeRules = [/smallest complete change:/i, /Never weaken or delete tests/, /When you changed files/];
    for (const rule of changeRules) {
      expect(octocodePrompt(WRITER)).toMatch(rule);
      expect(octocodePrompt(READER)).not.toMatch(rule);
    }
    expect(octocodePrompt(READER)).toMatch(/Investigate and report; do not modify the repository/);
    expect(octocodePrompt(WRITER)).not.toMatch(/do not modify the repository/);
    expect(octocodePrompt(READER)).toMatch(/Shell and MCP tools can still write/);
    const mcpOnly = octocodePrompt({ ...READER, octocode: true, canShell: false });
    expect(mcpOnly).toMatch(/MCP tools can still write/);
    expect(mcpOnly).not.toMatch(/Shell/);
    expect(octocodePrompt({ ...READER, canShell: false, canMcp: false })).not.toMatch(/Shell|MCP tools/);
  });

  it('keeps every subagent prompt free of per-child ids and paths, so siblings share a cached prefix', () => {
    for (const input of [WRITER, READER]) expect(octocodePrompt(input)).not.toMatch(/\.octocode\/tmp|-[0-9a-f]{4}\b/);
  });

  it('describes the Pi team versus agents-communication split to the main session only, naming `agent` only when it has it', () => {
    expect(octocodePrompt(MAIN)).toMatch(/agents-communication/);
    expect(octocodePrompt(WRITER)).not.toMatch(/agents-communication/);
    const noAgent = octocodePrompt({ ...MAIN, canUseAgent: false });
    expect(noAgent).toMatch(/agents-communication/);
    expect(noAgent).not.toContain('`agent`');
  });
});
