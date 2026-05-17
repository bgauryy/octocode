import { isLoggingEnabled, getActiveProvider } from './serverConfig.js';
import { version } from '../package.json';
import {
  getOrCreateSession,
  incrementToolCalls,
  incrementPromptCalls,
  incrementErrors,
  incrementRateLimits,
  incrementGitHubCacheRateLimits,
  updateSessionStats,
  type PersistedSession,
} from 'octocode-shared';
import type {
  SessionData,
  ToolCallData,
  PromptCallData,
  ErrorData,
  RateLimitData,
} from './types.js';
import { isLocalTool } from './tools/toolNames.js';

/**
 * SessionManager handles both:
 * 1. Local session persistence (via octocode-shared)
 * 2. Remote telemetry logging (existing behavior)
 *
 * The session ID is persisted in ~/.octocode/session.json and reused
 * across server restarts. Statistics are tracked in ~/.octocode/stats.json.
 */
class SessionManager {
  private session: PersistedSession;
  private readonly logEndpoint = 'https://octocode-mcp-host.onrender.com/log';

  constructor() {
    this.session = getOrCreateSession();
  }

  getSessionId(): string {
    return this.session.sessionId;
  }

  getSession(): PersistedSession {
    return this.session;
  }

  async logInit(): Promise<void> {
    await this.sendLog('init', {});
  }

  async logToolCall(
    toolName: string,
    repos: string[],
    _mainResearchGoal?: string,
    _researchGoal?: string,
    _reasoning?: string
  ): Promise<void> {
    // Update persistent stats
    const result = incrementToolCalls(1);
    if (result.session) {
      this.session = result.session;
    }

    const data: ToolCallData = {
      tool_name: toolName,
      repos: !isLocalTool(toolName) ? repos.map(() => '[redacted]') : [],
      provider: !isLocalTool(toolName) ? getActiveProvider() : undefined,
    };
    await this.sendLog('tool_call', data);
  }

  async logPromptCall(promptName: string): Promise<void> {
    // Update persistent stats
    const result = incrementPromptCalls(1);
    if (result.session) {
      this.session = result.session;
    }

    const data: PromptCallData = { prompt_name: promptName };
    await this.sendLog('prompt_call', data);
  }

  async logError(toolName: string, errorCode: string): Promise<void> {
    // Update persistent stats
    const result = incrementErrors(1);
    if (result.session) {
      this.session = result.session;
    }

    await this.sendLog('error', { error: `${toolName}:${errorCode}` });
  }

  async logRateLimit(data: RateLimitData): Promise<void> {
    // Update persistent stats
    const result = data.provider
      ? updateSessionStats({
          rateLimits: 1,
          rateLimitsByProvider: {
            [data.provider]: 1,
          },
        } as Parameters<typeof updateSessionStats>[0])
      : incrementRateLimits(1);
    if (result.session) {
      this.session = result.session;
    }

    if (data.provider === 'github') {
      const githubCacheResult = incrementGitHubCacheRateLimits(1);
      if (githubCacheResult.session) {
        this.session = githubCacheResult.session;
      }
    }

    await this.sendLog('rate_limit', data);
  }

  logPackageRegistryFailure(registry: string): void {
    const result = updateSessionStats({
      packageRegistryFailures: {
        [registry]: 1,
      },
    } as Parameters<typeof updateSessionStats>[0]);
    if (result.session) {
      this.session = result.session;
    }
  }

  private async sendLog(
    intent: 'init' | 'tool_call' | 'prompt_call' | 'error' | 'rate_limit',
    data:
      | ToolCallData
      | PromptCallData
      | ErrorData
      | RateLimitData
      | Record<string, never>
  ): Promise<void> {
    if (intent !== 'init' && !isLoggingEnabled()) {
      return;
    }

    try {
      const payload: SessionData = {
        sessionId: this.session.sessionId,
        intent,
        data,
        timestamp: new Date().toISOString(),
        version,
      };

      await fetch(this.logEndpoint, {
        method: 'POST',
        body: JSON.stringify(payload),
        headers: { 'Content-Type': 'application/json' },
        signal: AbortSignal.timeout(5000),
      });
    } catch {
      // Telemetry POST failures are non-actionable; avoid stderr noise for stdio MCP consumers.
    }
  }
}

let sessionManager: SessionManager | null = null;

export function initializeSession(): SessionManager {
  if (!sessionManager) {
    sessionManager = new SessionManager();
  }
  return sessionManager;
}

export function getSessionManager(): SessionManager | null {
  return sessionManager;
}

export async function logSessionInit(): Promise<void> {
  const session = getSessionManager();
  if (session) {
    await session.logInit();
  }
}

export async function logToolCall(
  toolName: string,
  repos: string[],
  mainResearchGoal?: string,
  researchGoal?: string,
  reasoning?: string
): Promise<void> {
  const session = getSessionManager();
  if (session) {
    await session.logToolCall(
      toolName,
      repos,
      mainResearchGoal,
      researchGoal,
      reasoning
    );
  }
}

export async function logPromptCall(promptName: string): Promise<void> {
  const session = getSessionManager();
  if (session) {
    await session.logPromptCall(promptName);
  }
}

export async function logSessionError(
  toolName: string,
  errorCode: string
): Promise<void> {
  const session = getSessionManager();
  if (session) {
    await session.logError(toolName, errorCode);
  }
}

export async function logRateLimit(data: RateLimitData): Promise<void> {
  const session = getSessionManager();
  if (session) {
    await session.logRateLimit(data);
  }
}

export function logPackageRegistryFailure(registry: string): void {
  const session = getSessionManager();
  if (session) {
    session.logPackageRegistryFailure(registry);
  }
}

export function resetSessionManager(): void {
  sessionManager = null;
}
