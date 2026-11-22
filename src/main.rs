#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
compile_error!("caffeinate supports Windows, macOS, and Linux");

mod cli;
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "unix/mod.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows/mod.rs"]
mod platform;
mod runtime;

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
