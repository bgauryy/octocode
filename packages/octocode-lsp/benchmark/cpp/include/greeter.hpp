#pragma once

#include <string>

struct Greeter {
  virtual ~Greeter() = default;
  virtual std::string greet(const std::string& name) const = 0;
};

struct FriendlyGreeter final : Greeter {
  std::string greet(const std::string& name) const override;
};

std::string welcome(const Greeter& greeter);
