package service

type Greeter interface {
	Greet(name string) string
}

type FriendlyGreeter struct{}

func (FriendlyGreeter) Greet(name string) string {
	return "Hello, " + name
}

func Welcome(greeter Greeter) string {
	return greeter.Greet("Octocode")
}
