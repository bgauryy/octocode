import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';

export type CapabilityKind = 'skill' | 'mcp' | 'hook';
export interface CapabilityPathOptions {
  homeDir?: string;
  octocodeHome?: string;
  env?: NodeJS.ProcessEnv;
}

/** Paths only: discovery never creates directories or loads executable configuration. */
export function capabilitySourcePaths(cwd: string, options: CapabilityPathOptions = {}) {
  const env = options.env ?? process.env;
  const homeDir = path.resolve(options.homeDir ?? os.homedir());
  const globalRoot = getOctocodeHome({
    ...env,
    ...(options.octocodeHome ? { OCTOCODE_HOME: options.octocodeHome } : {}),
    // A supplied home is a host/test policy, not another implementation of home resolution.
    ...(!options.octocodeHome && !env.OCTOCODE_HOME && options.homeDir ? { OCTOCODE_HOME: path.join(homeDir, '.octocode') } : {}),
  });
  const workspaceRoot = path.join(path.resolve(cwd), '.agents');
  const agentDir = path.resolve(env.PI_CODING_AGENT_DIR?.trim() || path.join(homeDir, '.pi', 'agent'));
  return {
    homeDir,
    codexHome: path.resolve(env.CODEX_HOME?.trim() || path.join(homeDir, '.codex')),
    native: {
      globalRoot, workspaceRoot,
      mcpFile: path.join(globalRoot, 'mcp.json'),
      modelsFile: path.join(globalRoot, 'models.json'),
      skillsDir: path.join(globalRoot, 'skills'),
      hooksDir: path.join(globalRoot, 'hooks'),
      workspaceMcpFile: path.join(workspaceRoot, 'mcp.json'),
      workspaceModelsFile: path.join(workspaceRoot, 'models.json'),
      workspaceSkillsDir: path.join(workspaceRoot, 'skills'),
      workspaceHooksDir: path.join(workspaceRoot, 'hooks'),
      instructionsFile: path.join(workspaceRoot, 'AGENTS.md'),
    },
    pi: { agentDir, modelsFile: path.join(agentDir, 'models.json') },
  };
}

export type CapabilitySourcePaths = ReturnType<typeof capabilitySourcePaths>;

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value === undefined) return null;
  if (!value || typeof value !== 'object') return value;
  if (value instanceof Date) return value.toISOString();
  return Object.fromEntries(Object.entries(value as Record<string, unknown>)
    .filter(([, child]) => child !== undefined)
    .sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
    .map(([key, child]) => [key, canonical(child)]));
}

/** Order-independent definition hash. Include resolved link targets when they affect execution. */
export function capabilityDefinitionRevision(value: unknown): string {
  return `sha256:${createHash('sha256').update(JSON.stringify(canonical(value))).digest('hex')}`;
}

/** Location identity stays stable when definition bytes or a link target change. */
export function stableCapabilitySourceId(source: { kind: CapabilityKind | 'instruction'; host: string; scope: string; path: string; name?: string }): string {
  return capabilityDefinitionRevision({ ...source, path: path.resolve(source.path) });
}

/** Repository root through the active directory, in increasing precedence. */
export function repositoryCapabilityDirectories(cwd: string): string[] {
  const resolved = path.resolve(cwd);
  const descending = [resolved];
  let current = resolved;
  for (;;) {
    if (fs.existsSync(path.join(current, '.git'))) return descending.reverse();
    const parent = path.dirname(current);
    if (parent === current) return [resolved];
    descending.push(parent);
    current = parent;
  }
}

export interface AgentInstructionFile {
  path: string;
  content: string;
  scope: 'user' | 'workspace';
  sourceId: string;
  revision: string;
}
export interface AgentInstructionDiscovery {
  files: AgentInstructionFile[];
  diagnostics: Array<{ path: string; message: string }>;
}

/** Native instructions only. Pi's own AGENTS files are already loaded by its resource loader. */
export function discoverAgentInstructionFiles(cwd: string, options: CapabilityPathOptions & { trusted?: boolean; excludePaths?: readonly string[] } = {}): AgentInstructionDiscovery {
  const paths = capabilitySourcePaths(cwd, options);
  const directories = repositoryCapabilityDirectories(cwd);
  const seen = new Set<string>();
  for (const file of [path.join(paths.pi.agentDir, 'AGENTS.md'), ...directories.flatMap(directory => [path.join(directory, 'AGENTS.md'), path.join(directory, 'CLAUDE.md')]), ...(options.excludePaths ?? [])]) {
    try { seen.add(fs.realpathSync(file)); } catch { /* Absent Pi resources cannot duplicate a native file. */ }
  }
  const candidates: Array<{ path: string; scope: 'user' | 'workspace' }> = [
    { path: path.join(paths.homeDir, '.agents', 'AGENTS.md'), scope: 'user' },
    { path: path.join(paths.native.globalRoot, 'AGENTS.md'), scope: 'user' },
    ...(options.trusted === false ? [] : directories.map(directory => ({ path: path.join(directory, '.agents', 'AGENTS.md'), scope: 'workspace' as const }))),
  ];
  const result: AgentInstructionDiscovery = { files: [], diagnostics: [] };
  for (const candidate of candidates) {
    if (!fs.existsSync(candidate.path)) continue;
    try {
      const realPath = fs.realpathSync(candidate.path);
      if (seen.has(realPath)) continue;
      const stat = fs.statSync(realPath);
      if (!stat.isFile()) throw new Error('Instruction source must be a regular file');
      if (stat.size > 512 * 1024) throw new Error('Instruction source exceeds 524288 bytes');
      const content = fs.readFileSync(realPath, 'utf8');
      const after = fs.statSync(realPath);
      if (fs.realpathSync(candidate.path) !== realPath || stat.dev !== after.dev || stat.ino !== after.ino || stat.size !== after.size || stat.mtimeMs !== after.mtimeMs) throw new Error('Instruction source changed during discovery');
      seen.add(realPath);
      result.files.push({ ...candidate, content, sourceId: stableCapabilitySourceId({ kind: 'instruction', host: 'octocode', scope: candidate.scope, path: candidate.path }), revision: capabilityDefinitionRevision({ content, realPath }) });
    } catch (error) { result.diagnostics.push({ path: candidate.path, message: (error as Error).message }); }
  }
  return result;
}
