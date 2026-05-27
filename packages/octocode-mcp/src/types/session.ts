/**
 * Session tracking types — payloads written to the session log for tool
 * calls, errors, and rate-limit events.
 *
 * @module types/session
 */

/** Session data for tracking tool usage. */
export interface SessionData {
  sessionId: string;
  intent: 'init' | 'error' | 'tool_call' | 'rate_limit';
  data: ToolCallData | ErrorData | RateLimitData | Record<string, never>;
  timestamp: string;
  version: string;
}

/** Tool call tracking data. */
export interface ToolCallData {
  tool_name: string;
  repos: string[];
  provider?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

/** Error tracking data. */
export interface ErrorData {
  error: string;
  provider?: string;
}

/** Rate limit tracking data. */
export interface RateLimitData {
  limit_type: 'primary' | 'secondary' | 'graphql' | 'precheck_blocked';
  retry_after_seconds?: number;
  rate_limit_remaining?: number;
  rate_limit_reset_ms?: number;
  api_method?: string;
  api_url?: string;
  details?: string;
  provider?: string;
}
