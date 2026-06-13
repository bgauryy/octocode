#include "greeter.hpp"

std::string FriendlyGreeter::greet(const std::string& name) const {
  return "Hello, " + name;
}

std::string welcome(const Greeter& greeter) {
  return greeter.greet("Octocode");
}
