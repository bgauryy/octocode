import { z } from 'zod';
import path from 'path';
import os from 'os';


export const toNumber = (val: unknown): unknown => {
  if (typeof val === 'number') return val;
  if (typeof val === 'string' && /^\d+$/.test(val)) return parseInt(val, 10);
  return val;
};


export const toBoolean = (val: unknown): unknown => {
  if (typeof val === 'boolean') return val;
  if (val === 'true') return true;
  if (val === 'false') return false;
  return val;
};


export const toArray = (val: unknown): unknown => {
  if (Array.isArray(val)) return val;
  if (typeof val === 'string') {
    if (val.trim() === '') return [];
    return val.split(',').map((s) => s.trim()).filter(Boolean);
  }
  return val;
};


export const numericString = z.preprocess(toNumber, z.number().optional());


export const requiredNumber = z.preprocess(toNumber, z.number());


export const booleanString = z.preprocess(toBoolean, z.boolean().optional());


export const stringArray = z.preprocess(toArray, z.array(z.string()));


const URL_ENCODED_TRAVERSAL = [
  '%2e%2e',
  '%2e%2e%2f',
  '%2e%2e%5c',
  '%252e',
  '%2f',
  '%5c',
] as const;


export const safePath = z.string().refine(
  (p) => {
    if (p.includes('\0')) return false;

    const normalized = path.normalize(p);
    if (normalized.includes('..')) return false;

    if (os.platform() !== 'win32' && p.includes('\\')) return false;

    const lowerPath = p.toLowerCase();
    if (URL_ENCODED_TRAVERSAL.some((pattern) => lowerPath.includes(pattern))) {
      return false;
    }

    return true;
  },
  {
    message:
      'Path contains invalid characters or traversal patterns ' +
      '(null bytes, .., \\, URL-encoded sequences)',
  }
);


const RESEARCH_DEFAULTS = {
  mainResearchGoal: 'HTTP API request',
  researchGoal: 'Execute tool via HTTP',
  reasoning: 'HTTP API call',
} as const;

let httpQueryCounter = 0;


export function withResearchDefaults<T extends Record<string, unknown>>(
  data: T
): Omit<T, 'id' | 'mainResearchGoal' | 'researchGoal' | 'reasoning'> & {
  id: string;
  mainResearchGoal: string;
  researchGoal: string;
  reasoning: string;
} {
  return {
    ...data,
    id: (data.id as string | undefined) ?? `http-${++httpQueryCounter}`,
    mainResearchGoal:
      (data.mainResearchGoal as string | undefined) ??
      RESEARCH_DEFAULTS.mainResearchGoal,
    researchGoal:
      (data.researchGoal as string | undefined) ??
      RESEARCH_DEFAULTS.researchGoal,
    reasoning:
      (data.reasoning as string | undefined) ?? RESEARCH_DEFAULTS.reasoning,
  };
}
