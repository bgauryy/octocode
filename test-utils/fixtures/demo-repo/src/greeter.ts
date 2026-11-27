// Tool-health fixture: a tiny TypeScript module with declarations, callers and a marker.
export interface Greeting {
  text: string;
}

export function greet(name: string): Greeting {
  // healthMarker greeter-1
  return { text: `hello ${name}` };
}

export class Greeter {
  constructor(private readonly prefix: string) {}

  say(name: string): string {
    // healthMarker greeter-2
    return this.prefix + greet(name).text;
  }
}
