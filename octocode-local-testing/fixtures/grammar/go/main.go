package demo

const LIMIT = 3

type Widget struct{ n int }

func (w Widget) Area() int { return helper(LIMIT) }

func helper(n int) int { return n * 2 }

func run() int {
	s := "😀"; return helper(len(s))
}
