pub trait Greeter {
    fn greet(&self, name: &str) -> String;
}

pub struct FriendlyGreeter;

impl Greeter for FriendlyGreeter {
    fn greet(&self, name: &str) -> String {
        format!("Hello, {name}")
    }
}

pub fn welcome(greeter: &dyn Greeter) -> String {
    greeter.greet("Octocode")
}
