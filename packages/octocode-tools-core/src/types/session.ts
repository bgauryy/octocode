export interface SessionData {
  sessionId: string;
  intent: 'init' | 'error' | 'tool_call' | 'rate_limit';
  data: ToolCallData | ErrorData | RateLimitData | Record<string, never>;
  timestamp: string;
  version: string;
}

export interface ToolCallData {
  tool_name: string;
  repos: string[];
  provider?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

export interface ErrorData {
  error: string;
  provider?: string;
}

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
