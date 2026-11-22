#[cfg(not(windows))]
compile_error!("caffeinate requires Windows");

#[cfg(windows)]
mod cli;
#[cfg(windows)]
mod power;
#[cfg(windows)]
mod runtime;

#[cfg(windows)]
fn main() {
    use std::io::{self, Write};
    use usage_rs::embedded::Outcome;

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let status = match cli::parse(&args) {
        Outcome::Parsed((options, command)) => runtime::run(&options, command),
        Outcome::Exit(exit) => {
            let text = exit.text.as_bytes();
            let _ = if exit.stderr {
                io::stderr().lock().write_all(text)
            } else {
                io::stdout().lock().write_all(text)
            };
            exit.code
        }
    };
    std::process::exit(status);
}
