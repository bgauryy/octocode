import { FriendlyGreeter, welcome } from './service.js';

export function main() {
  const greeter = new FriendlyGreeter();
  return welcome(greeter);
}

main();
