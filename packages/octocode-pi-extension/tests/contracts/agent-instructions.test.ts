import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { discoverAgentInstructionFiles } from '../../src/contracts/capability-sources.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
function fixture() { const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-instructions-')); roots.push(root); const homeDir = path.join(root, 'home'); const cwd = path.join(root, 'repo', 'app'); fs.mkdirSync(path.join(root, 'repo', '.git'), { recursive: true }); fs.mkdirSync(cwd); return { root, homeDir, cwd, octocodeHome: path.join(root, 'octocode') }; }
function write(file: string, content: string) { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, content); }

it('discovers native instruction files in order with exact text and workspace trust', () => {
  const options = fixture();
  const files = [path.join(options.homeDir, '.agents', 'AGENTS.md'), path.join(options.octocodeHome, 'AGENTS.md'), path.join(options.root, 'repo', '.agents', 'AGENTS.md'), path.join(options.cwd, '.agents', 'AGENTS.md')];
  files.forEach((file, index) => write(file, `# Instruction ${index}\n\nExact  spacing.\n`));
  const result = discoverAgentInstructionFiles(options.cwd, options);
  expect(result.files.map(file => file.path)).toEqual(files);
  expect(result.files[0]).toMatchObject({ content: '# Instruction 0\n\nExact  spacing.\n', scope: 'user', sourceId: expect.stringMatching(/^sha256:/), revision: expect.stringMatching(/^sha256:/) });
  expect(result.diagnostics).toEqual([]);
  expect(discoverAgentInstructionFiles(options.cwd, { ...options, trusted: false }).files.map(file => file.path)).toEqual(files.slice(0, 2));
});

it('deduplicates native links against Pi instruction files and reports oversized files', () => {
  const options = fixture();
  const piFile = path.join(options.homeDir, '.pi', 'agent', 'AGENTS.md');
  write(piFile, 'Pi already loads these instructions.');
  const duplicate = path.join(options.homeDir, '.agents', 'AGENTS.md');
  fs.mkdirSync(path.dirname(duplicate), { recursive: true }); fs.symlinkSync(piFile, duplicate);
  const oversized = path.join(options.cwd, '.agents', 'AGENTS.md');
  write(oversized, 'x'.repeat(512 * 1024 + 1));
  const result = discoverAgentInstructionFiles(options.cwd, options);
  expect(result.files).toEqual([]);
  expect(result.diagnostics).toEqual([expect.objectContaining({ path: oversized, message: expect.stringContaining('exceeds') })]);
});
