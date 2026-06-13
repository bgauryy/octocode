import { FriendlyGreeter, welcome } from './service';

export function main(): string {
  const greeter = new FriendlyGreeter();
  return welcome(greeter);
}

main();
