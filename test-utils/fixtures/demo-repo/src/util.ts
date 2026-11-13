import { greet } from './greeter';

// healthMarker util-1
export function shout(text: string): string {
  return text.toUpperCase();
}

export function greetLoudly(name: string): string {
  return shout(greet(name).text);
}
