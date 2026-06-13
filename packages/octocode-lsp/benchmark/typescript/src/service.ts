import type { Greeter } from './interface';

export class FriendlyGreeter implements Greeter {
  greet(name: string): string {
    return `Hello, ${name}`;
  }
}

export function welcome(greeter: Greeter): string {
  return greeter.greet('Octocode');
}
