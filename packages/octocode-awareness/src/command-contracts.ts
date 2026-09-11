/** Legacy command request retained only for the explicit operator/recovery lane. */
export interface AwarenessCommandCall {
  command: string;
  /** Exact snake_case fields returned by the legacy command descriptor. */
  params?: Record<string, unknown>;
}
