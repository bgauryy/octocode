// One mapping from typed recipe options to the environment variables a recipe reads, used by
// the standalone tool/CLI path (cli.mts) and by the `snapshot`/`screenshot` plan operations.
import { chromeRecipeOptionNames } from '../chrome-contract.mjs';

/** Plan operations that run a recipe, with the recipe whose options they accept. */
export const CAPTURE_RECIPES: Record<string, string> = {
  snapshot: 'page-snapshot',
  screenshot: 'page-screenshot',
};

export type RecipeEnv = Record<string, string>;

const NUMBER_TEXT = /^-?\d+(\.\d+)?$/;
const ANNOTATE_RANGE = /^e\d+-e?\d+$/;
type Kind = (value: unknown) => boolean;
const number: Kind = v =>
  (typeof v === 'number' && Number.isFinite(v)) ||
  (typeof v === 'string' && NUMBER_TEXT.test(v));
const flag: Kind = v => typeof v === 'boolean' || v === '0' || v === '1';
const text: Kind = v => typeof v === 'string';
const oneOf =
  (...allowed: string[]): Kind =>
  v =>
    typeof v === 'string' && allowed.includes(v);

// The value rules of the typed schema for the recipes a plan can run; other recipes keep type-only checks.
const KINDS: Record<string, Record<string, Kind>> = {
  'page-snapshot': {
    SNAPSHOT_DEPTH: number,
    SNAPSHOT_WAIT_SELECTOR: text,
    SNAPSHOT_WAIT_TEXT: text,
    SNAPSHOT_WAIT_MS: number,
    SNAPSHOT_MAX: number,
    SNAPSHOT_PAGE: number,
    SNAPSHOT_TEXT: number,
    SNAPSHOT_ROOT: text,
    SNAPSHOT_VIEWPORT: flag,
    SNAPSHOT_OUTLINE: flag,
    SNAPSHOT_CONTEXT: flag,
    SNAPSHOT_URLS: flag,
    SNAPSHOT_CLICKABLE: flag,
    SNAPSHOT_STDOUT: oneOf('summary'),
  },
  'page-screenshot': {
    SHOT_FULL: flag,
    SHOT_SELECTOR: text,
    SHOT_FORMAT: oneOf('jpeg', 'png'),
    SHOT_QUALITY: number,
    SHOT_SCALE: number,
    SHOT_ANNOTATE: v =>
      flag(v) || (typeof v === 'string' && ANNOTATE_RANGE.test(v)),
  },
};

/** The environment a recipe run reads for these options; throws on an unknown name or a bad value. */
export function recipeEnv(recipe: string, options: unknown): RecipeEnv {
  if (!options || typeof options !== 'object' || Array.isArray(options))
    throw new Error('options needs an object');
  const names = chromeRecipeOptionNames[recipe] ?? [];
  const env: RecipeEnv = {};
  for (const [key, value] of Object.entries(options)) {
    const valid =
      names.includes(key) &&
      value !== null &&
      ['string', 'number', 'boolean', 'object'].includes(typeof value) &&
      (KINDS[recipe]?.[key]?.(value) ?? true);
    if (!valid)
      throw new Error(
        `Unsupported ${recipe} option ${key}; allowed: ${names.join(', ')}`
      );
    env[key] =
      typeof value === 'boolean'
        ? value
          ? '1'
          : '0'
        : typeof value === 'object'
          ? JSON.stringify(value)
          : String(value);
  }
  return env;
}
