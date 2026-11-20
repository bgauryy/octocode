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
  /** number: inclusive bounds. */
  min?: number;
  max?: number;
  /** string: the only accepted values. */
  enum?: string[];
  /** string: a regular expression the whole value must match. */
  pattern?: string;
};
export type FlowArea =
  'emulation' | 'inspection' | 'input' | 'permissions' | 'audits';
export type FlowDefinition = {
  name: string;
  description: string;
  /** Group shown by `flow list`; saved workspace flows have none. */
  area?: FlowArea;
  /** Changes browser or origin permissions: needs a required boolean param `acknowledge`, which must be true. */
  sensitive?: boolean;
  params: Record<string, ParamDefinition>;
  /** The plan `run` takes; `{{param}}` placeholders may appear inside string values only; a value that is exactly one placeholder takes the param's typed value. */
  plan: Record<string, unknown>;
  /** Steps appended after plan.steps, repeated `params[param] + offset` times. */
  repeat?: { param: string; offset?: number; steps: unknown[] };
  /** Human-readable postconditions the run should leave true. */
  expects: string[];
};
