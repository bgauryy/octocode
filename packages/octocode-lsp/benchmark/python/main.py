from service import FriendlyGreeter, welcome


def main() -> str:
    greeter = FriendlyGreeter()
    return welcome(greeter)


main()
