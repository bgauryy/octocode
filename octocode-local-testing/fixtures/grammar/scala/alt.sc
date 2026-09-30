object Widget {
  val LIMIT = 3
  def helper(n: Int): Int = n * 2
  def run(): Int = {
    val s = "😀"; helper(s.length)
  }
}
