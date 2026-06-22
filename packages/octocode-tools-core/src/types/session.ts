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
