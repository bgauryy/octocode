//go:build linux
// +build linux

// Package p docs.
package p

/*
#include <stdio.h>
*/
import "C"

//go:generate stringer -type=Kind
//go:embed data.txt
var data string

const raw = `C:\path\` // windows
const raw2 = `a // not comment
  b ${x`
var r = 'x' // rune

func F() { // trailing
	fmt.Println("a  b") // c
}
