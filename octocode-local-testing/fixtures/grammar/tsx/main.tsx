export const LIMIT = 3;
export function helper(n: number) { return n * 2; }
export function Widget() {
  const s = "😀"; return <div>{helper(s.length)}</div>;
}
export function run() { return helper(LIMIT); }
