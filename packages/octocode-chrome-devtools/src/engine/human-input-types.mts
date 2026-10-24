// Type-only shapes for human-input.mts; erased at build, so the sandbox needs no read grant for it.
/** An inclusive [min, max] range a random value is drawn from. */
export type Range = [number, number];
export type Point = { x: number; y: number };
/** One trusted CDP input command and the pause after it. */
export type InputEvent = {
  method: string;
  params: Record<string, unknown>;
  delayMs: number;
};
/** CDP key fields for one key press. */
export type KeyDefinition = {
  key: string;
  code: string;
  windowsVirtualKeyCode: number;
  text: string | undefined;
};
export type MoveOptions = {
  minSteps?: number;
  maxSteps?: number;
  stepsDivisor?: number;
  wobbleMax?: number;
  overshootChance?: number;
  overshootPx?: Range;
  burstSize?: Range;
  burstPauseMs?: Range;
  stepDelayMs?: Range;
};
export type ClickOptions = {
  isInput?: boolean;
  aimDelayMs?: Range;
  holdMs?: Range;
  button?: string;
};
export type TypingOptions = {
  instant?: boolean;
  wpmBase?: number;
  wpmVariance?: number;
  mistakeChance?: number;
  burstSize?: Range;
  burstPauseMs?: Range;
};
export type ScrollOptions = { steps?: number; stepDelayMs?: Range };
