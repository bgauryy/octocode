# Go (.go)

Source sample: `go/print.go`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 33315 | - | - |
| content-view | 21954 | 34.1% | 1.078 ms |
| applyMinification | 22018 | 33.9% | 0.981 ms |
| sync minify | 22018 | 33.9% | 1.004 ms |
| async minify | 22018 | 33.9% | 0.981 ms |
| symbols | 4385 | 86.8% | 3.995 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```go
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

package fmt

import (
	"internal/fmtsort"
	"io"
	"os"
	"reflect"
	"strconv"
	"sync"
	"unicode/utf8"
)

// Strings for use with buffer.WriteString.
// This is less overhead than using buffer.Write with byte arrays.
const (
	commaSpaceString  = ", "
	nilAngleString    = "<nil>"
	nilParenString    = "(nil)"
	nilString         = "nil"
	mapString         = "map["
	percentBangString = "%!"
	missingString     = "(MISSING)"
	badIndexString    = "(BADINDEX)"
	panicString       = "(PANIC="
	extraString       = "%!(EXTRA "
	badWidthString    = "%!(BADWIDTH)"
	badPrecString     = "%!(BADPREC)"
	noVerbString      = "%!(NOVERB)"
	invReflectString  = "<invalid reflect.Value>"
)

// State represents the printer state passed to custom formatters.
// It provides access to the [io.Writer] interface plus information about
// the flags and options for the operand's format specifier.
type State interface {
	// Write is the function to call to emit formatted output to be printed.
	Write(b []byte) (n int, err error)
	// Width returns the value of the width option and whet

... [truncated 31511 chars] ...

	prevString := false
	for argNum, arg := range a {
		isString := arg != nil && reflect.TypeOf(arg).Kind() == reflect.String
		// Add a space between two non-string arguments.
		if argNum > 0 && !isString && !prevString {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
		prevString = isString
	}
}

// doPrintln is like doPrint but always adds a space between arguments
// and a newline after the last argument.
func (p *pp) doPrintln(a []any) {
	for argNum, arg := range a {
		if argNum > 0 {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
	}
	p.buf.writeByte('\n')
}

```

## Content-View Excerpt

```go
package fmt

import (
	"internal/fmtsort"
	"io"
	"os"
	"reflect"
	"strconv"
	"sync"
	"unicode/utf8"
)

const (
	commaSpaceString  = ", "
	nilAngleString    = "<nil>"
	nilParenString    = "(nil)"
	nilString         = "nil"
	mapString         = "map["
	percentBangString = "%!"
	missingString     = "(MISSING)"
	badIndexString    = "(BADINDEX)"
	panicString       = "(PANIC="
	extraString       = "%!(EXTRA "
	badWidthString    = "%!(BADWIDTH)"
	badPrecString     = "%!(BADPREC)"
	noVerbString      = "%!(NOVERB)"
	invReflectString  = "<invalid reflect.Value>"
)

type State interface {

	Write(b []byte) (n int, err error)

	Width() (wid int, ok bool)

	Precision() (prec int, ok bool)

	Flag(c int) bool
}

type Formatter interface {
	Format(f State, verb rune)
}

type Stringer interface {
	String() string
}

type GoStringer interface {
	GoString() string
}

func FormatString(state State, verb rune) string {
	var tmp [16]byte
	b := append(tmp[:0], '%')
	for _, c := range " +-#0" {
		if state.Flag(int(c)) {
			b = append(b, byte(c))
		}
	}
	if w, ok := state.Width(); ok {
		b = strconv.AppendInt(b, int64(w), 10)
	}
	if p, ok := state.Precision(); ok {
		b = append(b, '.')
		b = strconv.AppendInt(b, int64(p), 10)
	}


... [truncated 20154 chars] ...

f.writeString(reflect.TypeOf(arg).String())
				p.buf.writeByte('=')
				p.printArg(arg, 'v')
			}
		}
		p.buf.writeByte(')')
	}
}

func (p *pp) doPrint(a []any) {
	prevString := false
	for argNum, arg := range a {
		isString := arg != nil && reflect.TypeOf(arg).Kind() == reflect.String

		if argNum > 0 && !isString && !prevString {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
		prevString = isString
	}
}

func (p *pp) doPrintln(a []any) {
	for argNum, arg := range a {
		if argNum > 0 {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
	}
	p.buf.writeByte('\n')
}
```

## Apply Minification Excerpt

```go


package fmt

import (
	"internal/fmtsort"
	"io"
	"os"
	"reflect"
	"strconv"
	"sync"
	"unicode/utf8"
)


const (
	commaSpaceString  = ", "
	nilAngleString    = "<nil>"
	nilParenString    = "(nil)"
	nilString         = "nil"
	mapString         = "map["
	percentBangString = "%!"
	missingString     = "(MISSING)"
	badIndexString    = "(BADINDEX)"
	panicString       = "(PANIC="
	extraString       = "%!(EXTRA "
	badWidthString    = "%!(BADWIDTH)"
	badPrecString     = "%!(BADPREC)"
	noVerbString      = "%!(NOVERB)"
	invReflectString  = "<invalid reflect.Value>"
)


type State interface {

	Write(b []byte) (n int, err error)

	Width() (wid int, ok bool)

	Precision() (prec int, ok bool)


	Flag(c int) bool
}


type Formatter interface {
	Format(f State, verb rune)
}


type Stringer interface {
	String() string
}


type GoStringer interface {
	GoString() string
}


func FormatString(state State, verb rune) string {
	var tmp [16]byte
	b := append(tmp[:0], '%')
	for _, c := range " +-#0" {
		if state.Flag(int(c)) {
			b = append(b, byte(c))
		}
	}
	if w, ok := state.Width(); ok {
		b = strconv.AppendInt(b, int64(w), 10)
	}
	if p, ok := state.Precision(); ok {
		b = append(b, '.')
		b = strconv.AppendInt(b, int64(p)

... [truncated 20218 chars] ...

.writeString(reflect.TypeOf(arg).String())
				p.buf.writeByte('=')
				p.printArg(arg, 'v')
			}
		}
		p.buf.writeByte(')')
	}
}

func (p *pp) doPrint(a []any) {
	prevString := false
	for argNum, arg := range a {
		isString := arg != nil && reflect.TypeOf(arg).Kind() == reflect.String

		if argNum > 0 && !isString && !prevString {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
		prevString = isString
	}
}


func (p *pp) doPrintln(a []any) {
	for argNum, arg := range a {
		if argNum > 0 {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
	}
	p.buf.writeByte('\n')
}
```

## Sync Minify Excerpt

```go


package fmt

import (
	"internal/fmtsort"
	"io"
	"os"
	"reflect"
	"strconv"
	"sync"
	"unicode/utf8"
)


const (
	commaSpaceString  = ", "
	nilAngleString    = "<nil>"
	nilParenString    = "(nil)"
	nilString         = "nil"
	mapString         = "map["
	percentBangString = "%!"
	missingString     = "(MISSING)"
	badIndexString    = "(BADINDEX)"
	panicString       = "(PANIC="
	extraString       = "%!(EXTRA "
	badWidthString    = "%!(BADWIDTH)"
	badPrecString     = "%!(BADPREC)"
	noVerbString      = "%!(NOVERB)"
	invReflectString  = "<invalid reflect.Value>"
)


type State interface {

	Write(b []byte) (n int, err error)

	Width() (wid int, ok bool)

	Precision() (prec int, ok bool)


	Flag(c int) bool
}


type Formatter interface {
	Format(f State, verb rune)
}


type Stringer interface {
	String() string
}


type GoStringer interface {
	GoString() string
}


func FormatString(state State, verb rune) string {
	var tmp [16]byte
	b := append(tmp[:0], '%')
	for _, c := range " +-#0" {
		if state.Flag(int(c)) {
			b = append(b, byte(c))
		}
	}
	if w, ok := state.Width(); ok {
		b = strconv.AppendInt(b, int64(w), 10)
	}
	if p, ok := state.Precision(); ok {
		b = append(b, '.')
		b = strconv.AppendInt(b, int64(p)

... [truncated 20218 chars] ...

.writeString(reflect.TypeOf(arg).String())
				p.buf.writeByte('=')
				p.printArg(arg, 'v')
			}
		}
		p.buf.writeByte(')')
	}
}

func (p *pp) doPrint(a []any) {
	prevString := false
	for argNum, arg := range a {
		isString := arg != nil && reflect.TypeOf(arg).Kind() == reflect.String

		if argNum > 0 && !isString && !prevString {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
		prevString = isString
	}
}


func (p *pp) doPrintln(a []any) {
	for argNum, arg := range a {
		if argNum > 0 {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
	}
	p.buf.writeByte('\n')
}
```

## Async Minify Excerpt

```go


package fmt

import (
	"internal/fmtsort"
	"io"
	"os"
	"reflect"
	"strconv"
	"sync"
	"unicode/utf8"
)


const (
	commaSpaceString  = ", "
	nilAngleString    = "<nil>"
	nilParenString    = "(nil)"
	nilString         = "nil"
	mapString         = "map["
	percentBangString = "%!"
	missingString     = "(MISSING)"
	badIndexString    = "(BADINDEX)"
	panicString       = "(PANIC="
	extraString       = "%!(EXTRA "
	badWidthString    = "%!(BADWIDTH)"
	badPrecString     = "%!(BADPREC)"
	noVerbString      = "%!(NOVERB)"
	invReflectString  = "<invalid reflect.Value>"
)


type State interface {

	Write(b []byte) (n int, err error)

	Width() (wid int, ok bool)

	Precision() (prec int, ok bool)


	Flag(c int) bool
}


type Formatter interface {
	Format(f State, verb rune)
}


type Stringer interface {
	String() string
}


type GoStringer interface {
	GoString() string
}


func FormatString(state State, verb rune) string {
	var tmp [16]byte
	b := append(tmp[:0], '%')
	for _, c := range " +-#0" {
		if state.Flag(int(c)) {
			b = append(b, byte(c))
		}
	}
	if w, ok := state.Width(); ok {
		b = strconv.AppendInt(b, int64(w), 10)
	}
	if p, ok := state.Precision(); ok {
		b = append(b, '.')
		b = strconv.AppendInt(b, int64(p)

... [truncated 20218 chars] ...

.writeString(reflect.TypeOf(arg).String())
				p.buf.writeByte('=')
				p.printArg(arg, 'v')
			}
		}
		p.buf.writeByte(')')
	}
}

func (p *pp) doPrint(a []any) {
	prevString := false
	for argNum, arg := range a {
		isString := arg != nil && reflect.TypeOf(arg).Kind() == reflect.String

		if argNum > 0 && !isString && !prevString {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
		prevString = isString
	}
}


func (p *pp) doPrintln(a []any) {
	for argNum, arg := range a {
		if argNum > 0 {
			p.buf.writeByte(' ')
		}
		p.printArg(arg, 'v')
	}
	p.buf.writeByte('\n')
}
```

## Symbols

```txt
   5| package fmt
   7| import (
   8| 	"internal/fmtsort"
   9| 	"io"
  10| 	"os"
  11| 	"reflect"
  12| 	"strconv"
  13| 	"sync"
  14| 	"unicode/utf8"
  15| )
  19| const (
  20| 	commaSpaceString  = ", "
  21| 	nilAngleString    = "<nil>"
  22| 	nilParenString    = "(nil)"
  23| 	nilString         = "nil"
  24| 	mapString         = "map["
  25| 	percentBangString = "%!"
  26| 	missingString     = "(MISSING)"
  27| 	badIndexString    = "(BADINDEX)"
  28| 	panicString       = "(PANIC="
  29| 	extraString       = "%!(EXTRA "
  30| 	badWidthString    = "%!(BADWIDTH)"
  31| 	badPrecString     = "%!(BADPREC)"
  32| 	noVerbString      = "%!(NOVERB)"
  33| 	invReflectString  = "<invalid reflect.Value>"
  34| )
  39| type State interface {
  41| 	Write(b []byte) (n int, err error)
  43| 	Width() (wid int, ok bool)
  45| 	Precision() (prec int, ok bool)
  48| 	Flag(c int) bool
  49| }
  54| type Formatter interface {
  55| 	Format(f State, verb rune)
  56| }
  63| type Stringer interface {
  64| 	String() string
  65| }
  71| type GoStringer interface {
  72| 	GoString() string
  73| }
  81| func FormatString(state State, verb rune) string {
 101| type buffer []byte
 103| func (b *buffer) write(p []byte) {
 107| func (b *buffer) writeString(s string) {
 111| func (b *buffer) writeByte(c byte) {
 115| func (b *buffer) writeRune(r rune) {
 120| type pp struct {
 121| 	buf buffer
 124| 	fmt fmt
 127| 	reordered bool
 129| 	goodArgNum bool
 131| 	panicking bool
 133| 	erroring bool
 135| 	wrapErrs bool
 137| 	wrappedErrs []int
 138| }
 140| var ppFree = sync.Pool{
 141| 	New: func() any { return new(pp) },
 142| }
 145| func newPrinter() *pp {
 155| func (p *pp) free() {
 176| func (p *pp) Width() (wid int, ok bool) { return p.fmt.wid, p.fmt.widPres

... [truncated 1785 chars] ...

unc (p *pp) fmtPointer(arg any, value reflect.Value, verb rune) {
 580| func (p *pp) catchPanic(arg any, verb rune, method string) {
 614| func (p *pp) handleMethods(arg any, value reflect.Value, verb rune) (handled bool) {
 674| func (p *pp) printArg(arg any, verb rune) {
 756| func (p *pp) printValue(value reflect.Value, verb rune, depth int) {
 922| func intFromArg(a []any, argNum int) (num int, isInt bool, newArgNum int) {
 960| func parseArgNumber(format string) (index int, wid int, ok bool) {
 982| func (p *pp) argNumber(argNum int, format string, i int, numArgs int) (newArgNum, newi int, found bool) {
 995| func (p *pp) badArgNum(verb rune) {
1001| func (p *pp) missingArg(verb rune) {
1007| func (p *pp) doPrintf(format string, a []any) {
1185| func (p *pp) doPrint(a []any) {
1200| func (p *pp) doPrintln(a []any) {
```
