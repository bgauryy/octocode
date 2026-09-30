public class Widget {
  static final int LIMIT = 3;
  int area() { return helper(LIMIT); }
  static int helper(int n) { return n * 2; }
  static int run() {
    String s = "😀"; return helper(s.length());
  }
}
