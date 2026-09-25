import {
  DISABLED_BUILTIN_TOOL_NAMES,
  OCTOCODE_SUPPORT_TOOL_NAMES,
} from '../constants.js';
import type { NotifyFn, PiInstance, SkillInfo } from '../types.js';
import { registerAskUserTool } from './ask-user-tool.js';
import type { JobManager } from './bash-bg-tool.js';
import { registerBashTool } from './bash-tool.js';
import { registerCallTool } from './call-tool.js';
import { registerChromeDebugTool } from './chrome-debug-tool.js';
import { registerMediaTool } from './create-media-tool.js';
import { registerFileTool } from './file-tool.js';
import { registerUnifiedAgentTool } from './agents/tool.js';
import { isSubagentProcess } from './agents/registry.js';
import { registerLocalServerTool } from './local-server-tool.js';
import { registerMcpTool } from './mcp-tool.js';
import { registerUniqueTool } from './octocode-tools.js';
import { registerPlanTool } from './planning/plan-registration.js';
import { registerReadMediaTool } from './read-media-tool.js';
import { registerRunFfmpegTool } from './run-ffmpeg-tool.js';
import { registerSkillTool } from './skill-tool.js';
import { registerWebTool } from './web-tool.js';

/** Effective direct palette after environment and worker-process policy. */
export function activeSupportToolNames(): readonly string[] {
  return OCTOCODE_SUPPORT_TOOL_NAMES.filter((name) => {
    if (name === 'chromeDebug' && process.env['OCTOCODE_CHROME_DEBUG'] === '0') return false;
    if (isSubagentProcess() && (name === 'agent' || name === 'callTool')) return false;
    return true;
  });
}

/**
 * Remove Pi builtins replaced by MCP research or the unified `file` tool.
 * This is idempotent because Pi may reset its active palette during startup.
 */
export function disableBuiltinTools(pi: PiInstance): boolean {
  if (!pi.getActiveTools || !pi.setActiveTools) return false;
  try {
    const activeTools = pi.getActiveTools();
    if (!Array.isArray(activeTools)) return false;
    const disabled = new Set<string>(DISABLED_BUILTIN_TOOL_NAMES);
    const nextTools = activeTools.filter((toolName) => !disabled.has(toolName));
    if (nextTools.length === activeTools.length) return false;
    pi.setActiveTools(nextTools);
    return true;
  } catch (error) {
    const message = String((error as Error)?.message ?? error);
    if (!message.includes('Extension runtime not initialized')) {
      console.warn('[octocode-pi-extension] disableBuiltinTools non-critical error:', message);
    }
    return false;
  }
}

export interface SupportToolRegistrationOptions {
  pi: PiInstance;
  registeredToolNames: Set<string>;
  notify: NotifyFn;
  getPiSkills: () => SkillInfo[] | undefined;
}

/** The one composition point for Octocode's static Pi tool palette. */
export function registerSupportTools({
  pi,
  registeredToolNames,
  notify,
  getPiSkills,
}: SupportToolRegistrationOptions): JobManager {
  registerFileTool(pi, registeredToolNames, registerUniqueTool);
  const backgroundJobs = registerBashTool(pi, registeredToolNames, registerUniqueTool);
  registerReadMediaTool(pi, registeredToolNames, registerUniqueTool);
  registerMediaTool(pi, registeredToolNames, registerUniqueTool);
  registerRunFfmpegTool(pi, registeredToolNames, registerUniqueTool);
  registerWebTool(pi, registeredToolNames, registerUniqueTool);

  if (process.env['OCTOCODE_CHROME_DEBUG'] !== '0') {
    registerChromeDebugTool(pi, registeredToolNames, registerUniqueTool, notify);
  }

  registerUnifiedAgentTool(pi, registeredToolNames, registerUniqueTool);
  registerCallTool(pi, registeredToolNames, registerUniqueTool);
  registerSkillTool(pi, registeredToolNames, registerUniqueTool, getPiSkills);
  registerPlanTool(pi, registeredToolNames, registerUniqueTool);
  registerLocalServerTool(pi, registeredToolNames, registerUniqueTool);
  registerAskUserTool(pi, registeredToolNames, registerUniqueTool);
  registerMcpTool(pi, registeredToolNames, registerUniqueTool);
  return backgroundJobs;
}
