import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const PACKAGE_NAME = '@octocodeai/pi-extension';
export const SYSTEM_PROMPT_MARKER = '<!-- octocode-pi-extension:system-prompt -->';
export const MANAGED_BLOCK_START = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_START -->';
export const MANAGED_BLOCK_END = '<!-- OCTOCODE_PI_EXTENSION_APPEND_SYSTEM_END -->';

const extensionDir = path.dirname(fileURLToPath(import.meta.url));

export function getAssetPaths(baseDir = extensionDir) {
  return {
    baseDir,
    docsDir: path.join(baseDir, 'docs'),
    skillsDir: path.join(baseDir, 'skills'),
    systemPrompt: path.join(baseDir, 'system', 'APPEND_SYSTEM.md'),
  };
}

export function getAwarenessScriptPath(baseDir = extensionDir) {
  return path.join(getAssetPaths(baseDir).skillsDir, 'octocode-awareness', 'scripts', 'awareness.py');
}

export function readTextIfExists(filePath) {
  try {
    return fs.readFileSync(filePath, 'utf8');
  } catch (error) {
    if (error && error.code === 'ENOENT') {
      return '';
    }
    throw error;
  }
}

export function listBundledSkills(baseDir = extensionDir) {
  const { skillsDir } = getAssetPaths(baseDir);
  if (!fs.existsSync(skillsDir)) {
    return [];
  }

  return fs
    .readdirSync(skillsDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((skillName) => fs.existsSync(path.join(skillsDir, skillName, 'SKILL.md')))
    .sort();
}

export function shouldAppendSystemPrompt(systemPrompt, octocodePrompt) {
  const trimmedPrompt = octocodePrompt.trim();
  if (trimmedPrompt.length === 0) {
    return false;
  }

  if (systemPrompt.includes(SYSTEM_PROMPT_MARKER)) {
    return false;
  }

  const proofSlice = trimmedPrompt.slice(0, Math.min(160, trimmedPrompt.length));
  return !systemPrompt.includes(proofSlice);
}

export function renderSystemPromptAddendum(octocodePrompt) {
  return `${SYSTEM_PROMPT_MARKER}\n${octocodePrompt.trim()}\n${SYSTEM_PROMPT_MARKER}`;
}

export function renderManagedAppendSystem(octocodePrompt) {
  return `${MANAGED_BLOCK_START}\n${octocodePrompt.trim()}\n${MANAGED_BLOCK_END}\n`;
}

export function mergeManagedAppendSystem(existingContent, octocodePrompt) {
  const block = renderManagedAppendSystem(octocodePrompt);
  const startIndex = existingContent.indexOf(MANAGED_BLOCK_START);
  const endIndex = existingContent.indexOf(MANAGED_BLOCK_END);

  if (startIndex !== -1 && endIndex !== -1 && endIndex > startIndex) {
    const afterEnd = endIndex + MANAGED_BLOCK_END.length;
    return `${existingContent.slice(0, startIndex)}${block}${existingContent.slice(afterEnd).replace(/^\n+/, '')}`;
  }

  const prefix = existingContent.trimEnd();
  return prefix.length > 0 ? `${prefix}\n\n${block}` : block;
}

export function splitArgs(input) {
  const args = [];
  const pattern = /"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)'|[^\s]+/g;
  let match;

  while ((match = pattern.exec(input)) !== null) {
    const value = match[1] ?? match[2] ?? match[0];
    args.push(value.replace(/\\(["'\\])/g, '$1'));
  }

  return args;
}

export function parseSetupScope(args) {
  const tokens = splitArgs(args);
  if (tokens.includes('--global') || tokens.includes('global')) {
    return 'global';
  }
  return 'project';
}

export function getAppendSystemTarget(scope, cwd = process.cwd(), homeDir = os.homedir()) {
  if (scope === 'global') {
    return path.join(homeDir, '.pi', 'agent', 'APPEND_SYSTEM.md');
  }
  return path.join(cwd, '.pi', 'APPEND_SYSTEM.md');
}

export function getInstallSource(baseDir = extensionDir) {
  const packageRoot = path.dirname(baseDir);
  // npm installs land inside node_modules/@octocodeai/pi-extension
  if (packageRoot.includes(path.join('node_modules', '@octocodeai', 'pi-extension'))) {
    return 'npm:@octocodeai/pi-extension';
  }
  return packageRoot;
}

export function getAwarenessBridgeStatus(baseDir = extensionDir) {
  return fs.existsSync(getAwarenessScriptPath(baseDir)) ? 'available' : 'missing';
}

export function formatStatus(baseDir = extensionDir) {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const promptStatus = fs.existsSync(paths.systemPrompt) ? 'found' : 'missing';
  const awarenessStatus = getAwarenessBridgeStatus(baseDir);

  return [
    'Octocode Pi extension',
    `system prompt: ${promptStatus}`,
    `skills: ${skills.length}${skills.length > 0 ? ` (${skills.join(', ')})` : ''}`,
    `awareness file locks: ${awarenessStatus}`,
    `package assets: ${baseDir}`,
  ].join('\n');
}

function addPathValue(paths, value) {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) {
      addPathValue(paths, item);
    }
  }
}

export function extractWriteTargetPaths(toolName, input = {}) {
  if (toolName !== 'write' && toolName !== 'edit') {
    return [];
  }

  const paths = [];
  addPathValue(paths, input.path);
  addPathValue(paths, input.filePath);
  addPathValue(paths, input.file_path);
  addPathValue(paths, input.paths);
  addPathValue(paths, input.filePaths);
  addPathValue(paths, input.file_paths);

  return [...new Set(paths)];
}

export function getAwarenessAgentId(ctx) {
  if (process.env.OCTOCODE_AGENT_ID) {
    return process.env.OCTOCODE_AGENT_ID;
  }

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) {
    return `pi:${path.basename(sessionFile, path.extname(sessionFile))}`;
  }

  return `pi:${process.pid}`;
}

function defaultRunCommand(command, args, options = {}) {
  return new Promise((resolve) => {
    execFile(command, args, options, (error, stdout, stderr) => {
      resolve({
        error,
        status: error ? (Number.isInteger(error.code) ? error.code : 1) : 0,
        stderr: stderr ?? '',
        stdout: stdout ?? '',
      });
    });
  });
}

function targetFileArgs(files) {
  return files.flatMap((file) => ['--target-file', file]);
}

function formatAwarenessConflict(result) {
  const detail = [result.stderr, result.stdout].filter(Boolean).join('\n').trim();
  return detail.length > 0 ? `Octocode awareness blocked this edit:\n${detail}` : 'Octocode awareness blocked this edit.';
}

function notifyAwarenessWarning(ctx, result) {
  const detail = [result.stderr, result.stdout].filter(Boolean).join('\n').trim();
  const suffix = detail.length > 0 ? `: ${detail}` : '';
  notify(ctx, `Octocode awareness warning; continuing${suffix}`, 'warning');
}

async function runAwareness(args, ctx, options = {}) {
  const baseDir = options.baseDir ?? extensionDir;
  const scriptPath = getAwarenessScriptPath(baseDir);
  if (!fs.existsSync(scriptPath)) {
    return { skipped: true, status: 0, stdout: '', stderr: `Missing ${scriptPath}` };
  }

  return (options.runCommand ?? defaultRunCommand)(process.env.PYTHON ?? 'python3', [scriptPath, ...args], {
    cwd: ctx?.cwd ?? process.cwd(),
    env: process.env,
    timeout: 20000,
  });
}

export function createAwarenessBridge(options = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map();

  return {
    pendingToolFiles,

    async handleToolCall(event, ctx) {
      const targetFiles = extractWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) {
        return undefined;
      }

      const agentId = getAwarenessAgentId(ctx);
      const result = await runAwareness(
        [
          'pre-flight-intent',
          '--agent-id',
          agentId,
          '--workspace',
          ctx?.cwd ?? process.cwd(),
          '--rationale',
          'auto: Pi write/edit tool call via octocode-pi-extension',
          '--test-plan',
          'post-edit verification',
          '--ttl-minutes',
          '15',
          ...targetFileArgs(targetFiles),
        ],
        ctx,
        options
      );

      if (result.status === 2) {
        return { block: true, reason: formatAwarenessConflict(result) };
      }

      if (result.status !== 0) {
        notifyAwarenessWarning(ctx, result);
        return undefined;
      }

      if (!result.skipped && event?.toolCallId) {
        pendingToolFiles.set(event.toolCallId, targetFiles);
      }

      return undefined;
    },

    async handleToolResult(event, ctx) {
      const targetFiles = pendingToolFiles.get(event?.toolCallId);
      if (!targetFiles) {
        return undefined;
      }

      pendingToolFiles.delete(event.toolCallId);
      const result = await runAwareness(
        [
          'release-file-lock',
          '--agent-id',
          getAwarenessAgentId(ctx),
          '--status',
          'PENDING',
          ...targetFileArgs(targetFiles),
        ],
        ctx,
        options
      );

      if (result.status !== 0) {
        notifyAwarenessWarning(ctx, result);
      }

      return undefined;
    },
  };
}

function notify(ctx, message, level = 'info') {
  if (ctx?.ui?.notify) {
    ctx.ui.notify(message, level);
  }
}

async function confirm(ctx, title, message) {
  if (!ctx?.ui?.confirm) {
    return false;
  }
  return Boolean(await ctx.ui.confirm(title, message));
}

async function installAppendSystem(args, ctx) {
  const paths = getAssetPaths();
  const prompt = readTextIfExists(paths.systemPrompt);
  if (prompt.trim().length === 0) {
    notify(ctx, `Missing Octocode system prompt at ${paths.systemPrompt}`, 'error');
    return;
  }

  const scope = parseSetupScope(args);
  const targetPath = getAppendSystemTarget(scope, ctx?.cwd ?? process.cwd());
  const ok = await confirm(
    ctx,
    'Install Octocode APPEND_SYSTEM.md?',
    `Write the managed Octocode harness block to ${targetPath}?`
  );

  if (!ok) {
    notify(ctx, 'Octocode setup cancelled.', 'info');
    return;
  }

  const existing = readTextIfExists(targetPath);
  const nextContent = mergeManagedAppendSystem(existing, prompt);
  try {
    fs.mkdirSync(path.dirname(targetPath), { recursive: true });
    fs.writeFileSync(targetPath, nextContent, 'utf8');
    notify(ctx, `Octocode APPEND_SYSTEM.md installed at ${targetPath}`, 'info');
  } catch (error) {
    notify(ctx, `Failed to write ${targetPath}: ${error?.message ?? String(error)}`, 'error');
  }
}

function existingDirectory(filePath) {
  return fs.existsSync(filePath) ? filePath : null;
}

export default function octocodePiExtension(pi) {
  if (pi?.on) {
    const awarenessBridge = createAwarenessBridge();

    pi.on('resources_discover', async () => {
      const paths = getAssetPaths();
      const skillPath = existingDirectory(paths.skillsDir);
      return skillPath ? { skillPaths: [skillPath] } : {};
    });

    pi.on('before_agent_start', async (event) => {
      const prompt = readTextIfExists(getAssetPaths().systemPrompt);
      if (!shouldAppendSystemPrompt(event.systemPrompt, prompt)) {
        return;
      }

      return {
        systemPrompt: `${event.systemPrompt}\n\n${renderSystemPromptAddendum(prompt)}`,
      };
    });

    pi.on('tool_call', async (event, ctx) => awarenessBridge.handleToolCall(event, ctx));
    pi.on('tool_result', async (event, ctx) => awarenessBridge.handleToolResult(event, ctx));
  }

  if (!pi?.registerCommand) {
    return;
  }

  pi.registerCommand('octocode-status', {
    description: 'Show Octocode Pi extension assets and bundled skills.',
    handler: async (_args, ctx) => {
      notify(ctx, formatStatus(), 'info');
    },
  });

  pi.registerCommand('octocode-setup', {
    description: 'Install the Octocode APPEND_SYSTEM.md block into .pi or ~/.pi/agent.',
    handler: async (args, ctx) => {
      await installAppendSystem(args, ctx);
    },
  });

  pi.registerCommand('octocode-mcp-install', {
    description: 'Run the Octocode MCP installer after confirmation.',
    handler: async (args, ctx) => {
      const extraArgs = splitArgs(args);
      const cmdStr = ['npx', 'octocode', 'install', ...extraArgs].join(' ').trim();
      const ok = await confirm(ctx, 'Run Octocode MCP installer?', `Execute: ${cmdStr}`);
      if (!ok) {
        notify(ctx, 'Command cancelled.', 'info');
        return;
      }
      pi.sendUserMessage(cmdStr, { deliverAs: 'followUp' });
    },
  });

  pi.registerCommand('octocode-skills-update', {
    description: 'Update this Pi package, then reload Pi resources.',
    handler: async (_args, ctx) => {
      const source = getInstallSource();
      const cmdStr = `pi update ${source}`;
      const ok = await confirm(ctx, 'Update Octocode Pi package?', `Execute: ${cmdStr}`);
      if (!ok) {
        notify(ctx, 'Command cancelled.', 'info');
        return;
      }
      pi.sendUserMessage(cmdStr, { deliverAs: 'followUp' });
      if (ctx?.reload) {
        await ctx.reload();
      }
    },
  });
}
