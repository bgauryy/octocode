#include "greeter.hpp"

int main() {
  FriendlyGreeter greeter;
  auto message = welcome(greeter);
  return message.empty() ? 1 : 0;
}
