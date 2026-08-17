fn main() {
    let command = ls_repository_engineering::cli::parse_command(std::env::args().skip(1));
    match command {
        Ok(_) => {
            eprintln!("repository projection is not composed");
            std::process::exit(2);
        }
        Err(_) => {
            eprint!("{}", ls_repository_engineering::cli::HELP);
            std::process::exit(64);
        }
    }
}
