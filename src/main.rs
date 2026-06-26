use clap::Parser;

use png_pdf::cli;

fn main() {
    let args = cli::Args::parse();
    match cli::run(args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }
}
