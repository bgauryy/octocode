package main

import "octocode_lsp_benchmark_go/service"

func main() {
	greeter := service.FriendlyGreeter{}
	_ = service.Welcome(greeter)
}
