import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const skill = join(root, 'skills/octocode-agents-communication');
const bin = join(skill, 'scripts/bin');
if (!existsSync(bin)) throw new Error('Build the skill before packaging it.');
const targets = readdirSync(bin, { withFileTypes: true }).filter(entry => entry.isDirectory()).map(entry => entry.name).sort();
if (!targets.length) throw new Error('No platform binaries to package.');
for (const target of targets) {
  const name = `octocode-agents-communication${target.includes('windows') ? '.exe' : ''}`;
  const bytes = readFileSync(join(bin, target, name));
  const expected = `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`;
  if (readFileSync(join(bin, target, 'SHA256SUMS'), 'utf8') !== expected) throw new Error(`Checksum mismatch: ${target}`);
}
const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
mkdirSync(join(root, 'out'), { recursive: true });
const archive = join(root, 'out', `octocode-agents-communication-${version}-${targets.length === 1 ? targets[0] : 'multi-platform'}.tar.gz`);
execFileSync('tar', ['-czf', archive, '-C', join(root, 'skills'), 'octocode-agents-communication']);
console.log(JSON.stringify({ archive, targets }));
