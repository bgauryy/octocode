// Shared text for flows that change browser state or read protocol domains.
/** Appended to `expects` of flows whose effect lives only on the run's own CDP connection. */
export const FRAGMENT =
  'PLAN FRAGMENT: the effect lasts only for the CDP connection of the run that applies it. `flow show` returns `steps` to splice into a larger `run` plan; `flow run` applies them for that one run only';
/** The step `{$step: n, pointer}` reference of a CDP result. */
export const ref = (step: number, pointer: string) => ({
  $step: step,
  pointer,
});
