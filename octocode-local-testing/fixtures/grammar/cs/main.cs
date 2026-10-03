public class Widget {
  const int LIMIT = 3;
  int Area() { return Helper(LIMIT); }
  static int Helper(int n) { return n * 2; }
  static int Run() {
    var s = "😀"; return Helper(s.Length);
  }
}
