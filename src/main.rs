mod cli;

fn main() {
    if let Err(error) = cli::run() {
        eprintln!("{error:#}");
        let rendered = error.to_string();
        std::process::exit(if rendered.starts_with("code: ") { 2 } else { 1 });
    }
}
