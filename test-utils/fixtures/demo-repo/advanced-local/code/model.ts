export interface Shape {
  area(): number;
}

export class Square implements Shape {
  constructor(public size: number) {}
  area(): number {
    return this.size * this.size;
  }
}

export class LargeSquare extends Square {}

export function add(a: number, b: number): number {
  return a + b;
}

export function calculate(): number {
  const twice = add(1, 2);
  return add(twice, 3);
}

export const left = { run() { return 1; } };
export const right = { run() { return 2; } };
export const invoke = () => left.run() + right.run();
