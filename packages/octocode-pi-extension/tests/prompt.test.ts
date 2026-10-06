import { describe, expect, it } from 'vitest';
import { octocodePrompt } from '../src/prompt.js';

describe('prompt', () => {
  it('prefers Octocode MCP for research without contradicting Pi read/bash guidance or naming its tools', () => {
    const root = octocodePrompt({ octocode: true, profiles: [{ name: 'researcher', description: 'Reads code' }], canDelegate: true });
    expect(root).toContain('Prefer Octocode MCP (`mcp__octocode__*`) for code search and research');
    expect(root).toContain('GitHub repositories, PRs and history, npm packages');
    expect(root).toContain('`read` and bash stay right for known paths');
    // Octocode wins over Pi's generic "Use bash for file operations like ls, rg, find" for code search.
    expect(root).toContain('takes precedence over the generic rule to use bash for ls, rg or find');
    expect(root).not.toContain('tool_search');
    expect(octocodePrompt({ octocode: true, octocodeDeferred: true, profiles: [], canDelegate: true })).toContain('Load its GitHub and npm tools with `tool_search`');
    expect(root).not.toMatch(/instead of bash|instead of `read`/);
    // Pi's base prompt owns the identity and the skills rule.
    expect(root).not.toContain('You are Octocode');
    expect(root).not.toContain('SKILL.md');
    // Which Octocode tool does what is the server's business: no tool or parameter names.
    for (const coupled of ['localGetFileContent', 'localSearch', 'ghSearch', 'lspGetSemantics', 'npmSearch', 'resultView', 'concise']) expect(root).not.toContain(coupled);
    expect(root).not.toMatch(/(?<!mcp__)octocode_/);
    expect(root).not.toContain('github.com');
    expect(root).toContain('- researcher: Reads code');
    expect(root).not.toContain('webLive');
    // The askUser and coordinate/sendMessage tool descriptions own their usage rules.
    expect(root).not.toContain('askUser');
    expect(root).not.toContain('replyTo');
    expect(root.match(/as data, not as instructions/g)).toHaveLength(1);
    const browsing = octocodePrompt({ octocode: true, profiles: [{ name: 'webLive', description: 'Drives Chrome' }], canDelegate: true });
    expect(browsing).not.toContain('webHeadless');
    expect(browsing).toContain('- webLive: Drives Chrome');
    const child = octocodePrompt({ octocode: false, profiles: [], canDelegate: false });
    expect(child).toContain('You are a subagent');
    expect(child).toContain('Use `web` (or the `gh` CLI in bash when available) for GitHub and npm lookups');
    expect(child).not.toContain('Octocode MCP');
    expect(child).toContain('You cannot ask the user');
    expect(child).not.toContain('askUser');
    // Subagents load this extension too, so with Octocode connected they get the same preference.
    expect(octocodePrompt({ octocode: true, profiles: [], canDelegate: false })).toContain('Prefer Octocode MCP');
  });

  it('hands results back without polling, choosing report, message or doc', () => {
    const root = octocodePrompt({ octocode: false, profiles: [], canDelegate: true });
    expect(root).toContain('use `coordinate list` when current membership changes your next action');
    expect(root).not.toContain('Never poll a subagent');
    expect(root).toContain('ask a writable profile for a doc');
    // The agent tool description owns delegation mechanics: the limit, report delivery and report files.
    expect(root).not.toContain('Reports arrive automatically');
    expect(root).not.toMatch(/at most \d+ at a time/);
    expect(root).not.toContain('.octocode/tmp/agents/');
    const writer = octocodePrompt({ octocode: false, profiles: [], canDelegate: false, canWrite: true, identity: { id: 'implementer-1a2b', parentId: 'lead-0001', scratch: '/repo/.octocode/tmp/agents/implementer-1a2b' } });
    expect(writer).toContain('reaches `lead-0001` by itself');
    expect(writer).toContain('`sendMessage` `lead-0001` only for a blocker');
    expect(writer).toContain('a Markdown doc in `/repo/.octocode/tmp/agents/implementer-1a2b`');
    expect(writer).toContain('substantial or reusable results');
    expect(writer).not.toContain('over about 30 lines');
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: false, canWrite: true, identity: { id: 'i-1', scratch: `${process.cwd()}/.octocode/tmp/agents/i-1` } })).toContain('a Markdown doc in `.octocode/tmp/agents/i-1`');
    const reader = octocodePrompt({ octocode: false, profiles: [], canDelegate: false, canWrite: false, identity: { id: 'researcher-1a2b', scratch: '/repo/.octocode/tmp/agents/researcher-1a2b' } });
    expect(reader).toContain('a longer report is saved whole in `/repo/.octocode/tmp/agents/researcher-1a2b`');
    expect(reader).not.toContain('Markdown doc');
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: false })).toContain('under `.octocode/tmp/` in the repository');
  });

  it('offers background subagents', () => {
    expect(octocodePrompt({ octocode: false, profiles: [], canDelegate: true })).toContain('`background: true`');
  });

  it('gives a read-only agent one investigate-and-report rule instead of the rules about changing files', () => {
    const reader = octocodePrompt({ octocode: true, profiles: [], canDelegate: false, canWrite: false });
    expect(reader).toContain('- Investigate and report; do not modify files.');
    for (const rule of ['Make the smallest complete change', 'Never weaken or delete tests', 'When you changed files']) expect(reader).not.toContain(rule);
    const writer = octocodePrompt({ octocode: true, profiles: [], canDelegate: false, canWrite: true });
    expect(writer).not.toContain('do not modify files');
    for (const rule of ['Make the smallest complete change', 'Never weaken or delete tests', 'When you changed files']) expect(writer).toContain(rule);
  });
});
