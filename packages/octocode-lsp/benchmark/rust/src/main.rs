use octocode_lsp_benchmark_rust::{welcome, FriendlyGreeter};

fn main() {
    let greeter = FriendlyGreeter;
    let _message = welcome(&greeter);
}
