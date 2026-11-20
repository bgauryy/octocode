// The shape of a flow: a named, parametrized plan. Built-ins and saved workspace flows share it.
export type ParamType = 'string' | 'number' | 'boolean';
export type ParamValue = string | number | boolean;
export type ParamDefinition = {
  type: ParamType;
  required?: boolean;
  default?: ParamValue;
  /** Never echoed: not by list, show, run replies, run artifacts or errors. */
  secret?: boolean;
  description: string;
};
export type FlowDefinition = {
  name: string;
  description: string;
  params: Record<string, ParamDefinition>;
  /** The plan `run` takes; `{{param}}` placeholders may appear inside string values only. */
  plan: Record<string, unknown>;
  /** Steps appended after plan.steps, repeated `params[param] + offset` times. */
  repeat?: { param: string; offset?: number; steps: unknown[] };
  /** Human-readable postconditions the run should leave true. */
  expects: string[];
};
