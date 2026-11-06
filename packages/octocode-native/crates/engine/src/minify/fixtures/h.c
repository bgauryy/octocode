/* license */
#include <stdio.h>
#include "a//b.h"
#define URL "http://x" // macro comment
#define MAX(a,b) \
    ((a) > (b) ? (a) : (b))
// line comment \
   continued
#if 0
don't compile this // text
#endif
int main(void) { /* c */
    char c = '\''; // q
    int big = 1'000'000;
    printf("a // b\n"); // d
    return 0;
}
