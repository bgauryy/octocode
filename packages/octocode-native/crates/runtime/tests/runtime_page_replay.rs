mod github;
mod saved_pages;
mod snapshots;
mod windows;

fn lines(word: &str) -> String {
    (0..200)
        .map(|index| format!("needle {word} {index}\n"))
        .collect()
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
