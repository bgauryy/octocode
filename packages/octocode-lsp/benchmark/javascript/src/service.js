export class FriendlyGreeter {
  greet(name) {
    return `Hello, ${name}`;
  }
}

export function welcome(greeter) {
  return greeter.greet('Octocode');
}
