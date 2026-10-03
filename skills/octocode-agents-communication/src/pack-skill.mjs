import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { checkSkill, checkStartup, digest, payloadFiles, payloadDigest, python, runRuntime, runtimeInfo } from './artifact-checks.mjs';

export function packSkill(root, { timeoutMs = 10000 } = {}) {
  const runtime = runtimeInfo();
  const scripts = join(root, 'scripts');
  if (!existsSync(join(scripts, 'communication.py'))) throw Error('Portable Python runtime is missing; build the skill before packaging.');
  const { version } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
  const out = join(root, 'out'); mkdirSync(out, { recursive: true });
  const name = `octocode-agents-communication-${version}-portable.tar.gz`, archive = join(out, name);
  const staging = mkdtempSync(join(out, '.communication-pack-'));
  try {
    const shipped = join(staging, 'payload/octocode-agents-communication');
    mkdirSync(shipped, { recursive: true });
    copyFileSync(join(root, 'SKILL.md'), join(shipped, 'SKILL.md'));
    for (const source of payloadFiles(scripts)) {
      const destination = join(shipped, 'scripts', relative(scripts, source));
      mkdirSync(dirname(destination), { recursive: true }); copyFileSync(source, destination);
    }
    const candidate = join(staging, name), extracted = join(staging, 'extracted');
    mkdirSync(extracted);
    execFileSync(python(), ['-B', '-c', 'import sys,tarfile; archive,payload,extracted=sys.argv[1:];\nwith tarfile.open(archive,"w:gz") as t: t.add(payload,arcname="octocode-agents-communication")\nwith tarfile.open(archive,"r:gz") as t: t.extractall(extracted)', candidate, shipped, extracted], { timeout: 60000, killSignal: 'SIGKILL' });
    const extractedSkill = join(extracted, 'octocode-agents-communication');
    const entry = join(extractedSkill, 'scripts/communication.py');
    const runtimeSha256 = payloadDigest(join(shipped, 'scripts'));
    if (payloadDigest(join(extractedSkill, 'scripts')) !== runtimeSha256) throw Error('Extracted portable runtime differs from packaged payload.');
    const verification = { startup: checkStartup(entry, timeoutMs), skill: checkSkill(entry, join(extractedSkill, 'SKILL.md'), timeoutMs) };
    const schema = JSON.parse(runRuntime(entry, ['schema'], { timeout: timeoutMs }));
    if (!schema.commands?.length || !schema.tools?.length || !schema.database?.sql) throw Error('Extracted runtime schema is incomplete.');
    verification.schema = { passed: true, commands: schema.commands.length, tools: schema.tools.length };
    const workspace = join(staging, 'workspace'); mkdirSync(workspace);
    const database = join(workspace, 'state.sqlite');
    const flags = ['--workspace', workspace, '--database', database];
    const call = args => JSON.parse(runRuntime(entry, [...args, ...flags], { timeout: timeoutMs }));
    const agent = call(['join', '{"name":"package-check","vendor":"generic"}']);
    if (!agent.id || call(['peers']).items?.[0]?.id !== agent.id || call(['db', 'info']).compatible !== true) throw Error('Extracted runtime database smoke failed.');
    call(['leave', '--session', agent.id]);
    verification.database = { passed: true };
    const launcher = process.platform === 'win32'
      ? checkStartup('powershell.exe', timeoutMs, ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', join(extractedSkill, 'scripts/agents-communication.ps1'), '--help'])
      : checkStartup('/bin/sh', timeoutMs, [join(extractedSkill, 'scripts/agents-communication'), '--help']);
    renameSync(candidate, archive);
    return { archive, sha256: digest(archive), format: 'portable-python', runtime, runtimeSha256, verifiedPlatform: process.platform, verification, launcher };
  } finally { rmSync(staging, { recursive: true, force: true }); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  console.log(JSON.stringify(packSkill(dirname(dirname(fileURLToPath(import.meta.url))))));
}
