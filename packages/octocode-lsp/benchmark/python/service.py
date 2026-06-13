from typing import Protocol


class Greeter(Protocol):
    def greet(self, name: str) -> str:
        ...


class FriendlyGreeter:
    def greet(self, name: str) -> str:
        return f"Hello, {name}"


def welcome(greeter: Greeter) -> str:
    return greeter.greet("Octocode")
