#define LIMIT 3
struct Widget { int n; };
int helper(int n) { return n * 2; }
int run(void) {
  const char *s = "😀"; return helper((int)s[0]);
}
