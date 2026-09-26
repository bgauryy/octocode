import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { checkSkill, checkStartup, digest, executableName, rustHostTarget, verifyExecutable } from './artifact-checks.mjs';

export function packSkill(root, { hostTarget, target = process.env.CARGO_BUILD_TARGET ?? hostTarget, timeoutMs = 10000 }) {
  const skill = root;
  const bin = join(skill, 'scripts');
  if (!existsSync(bin)) throw new Error('Build the skill before packaging it.');
  const targets = [target];
  const checkHashes = base => {
    for (const target of targets) {
      const name = executableName(target);
      for (const entry of readdirSync(base, { withFileTypes: true })) {
        if (entry.name.endsWith('.tmp') || entry.name === 'bin') {
          throw new Error(`Unexpected runtime artifact: ${target}/${entry.name}; finish the build before packaging.`);
        }
      }
      const expected = `${digest(join(base, name))}  ${name}\n`;
      if (readFileSync(join(base, 'SHA256SUMS'), 'utf8') !== expected) throw new Error(`Checksum mismatch: ${target}`);
    }
  };
  checkHashes(bin);
  const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  const out = join(root, 'out');
  mkdirSync(out, { recursive: true });
  const name = `octocode-agents-communication-${version}-${target}.tar.gz`;
  const archive = join(out, name);
  const staging = mkdtempSync(join(out, '.communication-pack-'));
  try {
    const candidate = join(staging, name);
    const extracted = join(staging, 'extracted');
    mkdirSync(extracted);
    const payload = join(staging, 'payload');
    const shipped = join(payload, 'octocode-agents-communication');
    mkdirSync(shipped, { recursive: true });
    cpSync(join(skill, 'SKILL.md'), join(shipped, 'SKILL.md'));
    cpSync(join(skill, 'scripts'), join(shipped, 'scripts'), { recursive: true });
    execFileSync('tar', ['-czf', candidate, '-C', payload, 'octocode-agents-communication'], { timeout: 60000, killSignal: 'SIGKILL' });
    execFileSync('tar', ['-xzf', candidate, '-C', extracted], { timeout: 60000, killSignal: 'SIGKILL' });
    const extractedSkill = join(extracted, 'octocode-agents-communication');
    const extractedBin = join(extractedSkill, 'scripts');
    checkHashes(extractedBin);
    const verification = Object.fromEntries(targets.map(target => {
      const executable = join(extractedBin, executableName(target));
      const checks = verifyExecutable(executable, { target, hostTarget, timeoutMs });
      checks.skill = target === hostTarget
        ? checkSkill(executable, join(extractedSkill, 'SKILL.md'), timeoutMs)
        : { passed: null, reason: 'foreign-target-needs-native-CI' };
      return [target, checks];
    }));
    const launcher = process.platform !== 'win32' && targets.includes(hostTarget)
      ? checkStartup(join(extractedSkill, 'scripts/agents-communication'), timeoutMs)
      : { passed: null, reason: 'launcher-needs-native-CI' };
    renameSync(candidate, archive);
    return { archive, sha256: digest(archive), targets, verification, launcher };
  } finally { rmSync(staging, { recursive: true, force: true }); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  console.log(JSON.stringify(packSkill(root, { hostTarget: rustHostTarget() })));
}
