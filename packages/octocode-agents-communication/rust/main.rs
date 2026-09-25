fn main() {
    if let Err(error) = octocode_agents_communication::cli::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
