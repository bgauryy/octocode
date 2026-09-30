import { helper } from './b';

export function alphaNeedle(x: number): number {
  const y = helper(x);
  return y + 1;
}

export class Widget {
  run(): void {
    console.log('run');
    alphaNeedle(2);
  }
}
