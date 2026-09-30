const LIMIT = 3;
class Widget {
  area() { return helper(LIMIT); }
}
function helper(n) { return n * 2; }
function run() {
  const s = "😀"; return helper(s.length);
}
module.exports = { Widget, helper, run };
