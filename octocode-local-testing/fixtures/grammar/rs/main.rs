pub const LIMIT: i32 = 3;
pub struct Widget { n: i32 }
impl Widget {
    pub fn area(&self) -> i32 { helper(LIMIT + self.n) }
}
pub fn helper(n: i32) -> i32 { n * 2 }
pub fn run() -> i32 {
    let s = "😀"; helper(s.len() as i32)
}
