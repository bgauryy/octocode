const int LIMIT = 3;
int helper(int n) { return n * 2; }
class Widget {
 public:
  int area() { return helper(LIMIT); }
};
int run() {
  const char *s = "😀"; return helper(s[0]);
}
