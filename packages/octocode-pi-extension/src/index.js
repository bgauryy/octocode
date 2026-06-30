import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
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

export function formatStatus(baseDir = extensionDir) {
  const paths = getAssetPaths(baseDir);
  const skills = listBundledSkills(baseDir);
  const promptStatus = fs.existsSync(paths.systemPrompt) ? 'found' : 'missing';

  return [
    'Octocode Pi extension',
    `system prompt: ${promptStatus}`,
    `skills: ${skills.length}${skills.length > 0 ? ` (${skills.join(', ')})` : ''}`,
    `package assets: ${baseDir}`,
  ].join('\n');
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
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.writeFileSync(targetPath, nextContent, 'utf8');
  notify(ctx, `Octocode APPEND_SYSTEM.md installed at ${targetPath}`, 'info');
}

async function runWithConfirmation(pi, ctx, title, message, command, args) {
  const ok = await confirm(ctx, title, message);
  if (!ok) {
    notify(ctx, 'Command cancelled.', 'info');
    return;
  }

  if (!pi?.exec) {
    notify(ctx, `Pi exec API is unavailable. Run manually: ${[command, ...args].join(' ')}`, 'error');
    return;
  }

  const result = await pi.exec(command, args, {});
  const status = result?.status ?? result?.exitCode ?? 0;
  if (status === 0) {
    notify(ctx, `Finished: ${[command, ...args].join(' ')}`, 'info');
  } else {
    notify(ctx, `Command failed (${status}): ${[command, ...args].join(' ')}`, 'error');
  }
}

function existingDirectory(filePath) {
  return fs.existsSync(filePath) ? filePath : null;
}

export default function octocodePiExtension(pi) {
  if (pi?.on) {
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
      await runWithConfirmation(
        pi,
        ctx,
        'Run Octocode MCP installer?',
        `Execute: npx octocode install ${extraArgs.join(' ')}`.trim(),
        'npx',
        ['octocode', 'install', ...extraArgs]
      );
    },
  });

  pi.registerCommand('octocode-skills-update', {
    description: 'Update this Pi package, then reload Pi resources.',
    handler: async (_args, ctx) => {
      await runWithConfirmation(
        pi,
        ctx,
        'Update Octocode Pi package?',
        'Execute: pi update npm:@octocodeai/pi-extension',
        'pi',
        ['update', 'npm:@octocodeai/pi-extension']
      );

      if (ctx?.reload) {
        await ctx.reload();
      }
    },
  });
}
