export const LIMIT = 3;
export interface Shape { area(): number }
export class Widget implements Shape {
  area(): number { return helper(LIMIT); }
}
export function helper(n: number): number { return n * 2; }
export function run(): number {
  const s = "😀"; return helper(s.length);
}
