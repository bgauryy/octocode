import { c, bold, dim } from '../../utils/colors.js';
import {
  MCP_CLIENTS,
  type ClientInstallStatus,
} from '../../utils/mcp-config.js';
import type { AppState, SkillsState } from '../state.js';
import type { OctocodeAuthStatus } from '../../types/index.js';
import type { MenuChoice } from './types.js';

export function getClientNames(clients: ClientInstallStatus[]): string {
  return clients.map(c => MCP_CLIENTS[c.client]?.name || c.client).join(', ');
}

export function printInstalledIDEs(
  installedClients: ClientInstallStatus[]
): void {
  if (installedClients.length === 0) {
    console.log(`  ${dim('No IDEs configured yet')}`);
    return;
  }

  console.log(`  ${dim('Installed on:')}`);
  for (const client of installedClients) {
    const clientName = MCP_CLIENTS[client.client]?.name || client.client;
    console.log(
      `    ${dim('•')} ${dim(clientName)} ${dim('->')} ${c('cyan', client.configPath)}`
    );
  }
}

export function buildSkillsMenuItem(skills: SkillsState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (!skills.sourceExists || !skills.hasSkills) {
    return {
      name: `${dim('- Manage System Skills')}`,
      value: 'skills',
      description: dim('Not available — no skill sources found'),
    };
  }

  if (skills.allInstalled) {
    return {
      name: `- Manage System Skills ${c('green', '✅')}`,
      value: 'skills',
      description: `${skills.totalInstalledCount} installed • Research, PR Review & more`,
    };
  }

  if (skills.totalInstalledCount > 0) {
    return {
      name: '- Manage System Skills',
      value: 'skills',
      description: `${skills.totalInstalledCount}/${skills.skills.length} installed • Get more skills!`,
    };
  }

  return {
    name: `- ${bold('Manage System Skills')} ${c('cyan', '[NEW]')}`,
    value: 'skills',
    description: `Install skills for AI-powered coding workflows`,
  };
}

export function buildOctocodeSkillsMenuItem(skills: SkillsState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  const octocodeSkillsInstalled = skills.skills.filter(
    s => s.name.startsWith('octocode-') && s.installed
  ).length;

  if (octocodeSkillsInstalled > 0) {
    return {
      name: `- Octocode Skills ${c('green', '✅')}`,
      value: 'octocode-skills',
      description: `${octocodeSkillsInstalled} installed • Awareness & Research`,
    };
  }

  return {
    name: '- Octocode Skills',
    value: 'octocode-skills',
    description: 'Install Octocode Awareness and Research skills',
  };
}

export function getAuthSourceDisplay(auth: OctocodeAuthStatus): string {
  switch (auth.tokenSource) {
    case 'gh-cli':
      return 'gh CLI';
    case 'env': {
      if (auth.envTokenSource) {
        const varName = auth.envTokenSource.replace('env:', '');
        return `env (${varName})`;
      }
      return 'env var';
    }
    case 'octocode':
      return 'Octocode';
    default:
      return 'unknown';
  }
}

export function buildAuthMenuItem(auth: OctocodeAuthStatus): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (auth.authenticated) {
    const source = getAuthSourceDisplay(auth);
    const user = auth.username ? `@${auth.username}` : '';
    const userPart = user ? `${user} ` : '';
    return {
      name: `- Manage Auth ${c('green', '✅')}`,
      value: 'auth',
      description: `${userPart}via ${source}`,
    };
  }

  return {
    name: `- ${bold('Manage Auth')} ${c('red', '[Required]')}`,
    value: 'auth',
    description: `Sign in to access GitHub`,
  };
}

export function buildStatusLine(state: AppState): string {
  const parts: string[] = [];

  if (state.octocode.isInstalled) {
    const clientLabel =
      state.octocode.installedCount === 1 ? 'client' : 'clients';
    parts.push(
      `${c('green', '●')} ${state.octocode.installedCount} ${clientLabel}`
    );
  } else {
    parts.push(`${c('yellow', '○')} Not installed`);
  }

  if (state.skills.totalInstalledCount > 0) {
    parts.push(`${c('green', '●')} ${state.skills.totalInstalledCount} skills`);
  } else if (state.skills.sourceExists && state.skills.hasSkills) {
    parts.push(`${c('yellow', '○')} ${state.skills.skills.length} skills`);
  }

  return parts.join(dim('  │  '));
}

export function buildOctocodeMenuItem(state: AppState): {
  name: string;
  value: MenuChoice;
  description: string;
} {
  if (state.octocode.isInstalled) {
    const clientLabel = state.octocode.installedCount === 1 ? 'IDE' : 'IDEs';

    if (state.githubAuth.authenticated) {
      return {
        name: `- Octocode MCP ${c('green', '✅')}`,
        value: 'octocode',
        description: `Configure Octocode MCP - ${state.octocode.installedCount} ${clientLabel} configured`,
      };
    }

    return {
      name: `- Octocode MCP ${c('red', '[X]')}`,
      value: 'octocode',
      description: `Configure Octocode MCP - ${state.octocode.installedCount} ${clientLabel} configured`,
    };
  }

  return {
    name: `- ${bold('Octocode Configuration')}`,
    value: 'octocode',
    description: 'Configure Octocode MCP - 0 IDEs configured',
  };
}

export function printContextualHints(state: AppState): void {
  if (!state.githubAuth.authenticated) {
    console.log();
    console.log(
      `  ${c('yellow', 'Warning:')} ${bold('Auth required!')} Run ${c('cyan', 'Manage Auth')} to access GitHub repos`
    );
  } else if (
    state.octocode.isInstalled &&
    state.skills.totalInstalledCount === 0
  ) {
    console.log();
    console.log(
      `  ${c('cyan', 'Tip:')} ${dim('Boost your AI coding:')} Install ${c('magenta', 'Skills')} for research, PR review & more!`
    );
  }
}
