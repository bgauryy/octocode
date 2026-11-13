import { greet, Greeter } from './greeter';
import { shout } from './util';

// healthMarker app-1
export function main(): string {
  const first = greet('ada');
  const second = new Greeter('> ').say('grace');
  return shout(`${first.text} ${second}`);
}
