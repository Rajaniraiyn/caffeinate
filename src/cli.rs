#[cfg(windows)]
use std::borrow::Cow;
#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::num::NonZeroU32;
use std::time::Duration;
use usage_rs::embedded::{Exit, Outcome};
use usage_rs::{Args, Cli, Error, Event, Parser};

/// Prevent idle sleep while a command runs or until a timeout.
#[derive(Cli, Debug)]
#[usage(bin = "caffeinate", unknown_flags = "error", disable_help_subcommand)]
#[cfg_attr(windows, usage(after_help = "Slash switches are case-insensitive."))]
struct Invocation {
    #[usage(flatten)]
    options: Options,

    /// Command to run while the request is active.
    #[usage(double_dash = "automatic", value_name = "COMMAND")]
    _command: Option<OsString>,

    /// Arguments passed through to the command.
    #[usage(double_dash = "preserve", value_name = "ARGUMENT")]
    _arguments: Vec<OsString>,
}

#[derive(Args, Debug)]
pub struct Options {
    /// Keep the display on and prevent idle sleep.
    #[usage(short, long)]
    pub display: bool,

    /// Prevent idle sleep. This is the default.
    #[usage(short, long)]
    pub idle: bool,

    /// Request system sleep inhibition.
    #[usage(short, long)]
    pub system: bool,

    /// Keep the display active for five seconds or for the specified timeout.
    #[usage(short, long)]
    pub user_active: bool,

    /// Release the request after this many seconds. Zero means no timeout.
    #[usage(short = 't', long, value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Stop when this process exits. A command takes precedence.
    #[usage(short, long, value_name = "PID")]
    pub wait: Option<NonZeroU32>,

    /// Prevent disk idle sleep where supported.
    #[usage(short = 'm', long)]
    pub disk: bool,
}

impl Options {
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
            .filter(|&seconds| seconds != 0)
            .map(Duration::from_secs)
    }

    pub fn keep_idle(&self) -> bool {
        self.idle || !(self.display || self.system || self.user_active || self.disk)
    }

    pub fn activity_timeout(&self) -> Option<Duration> {
        self.user_active
            .then(|| self.timeout().unwrap_or(Duration::from_secs(5)))
    }
}

pub fn parse(args: &[OsString]) -> Outcome<(Options, &[OsString])> {
    #[cfg(windows)]
    let normalized: Vec<_> = args.iter().map(|arg| slash_switch(arg)).collect();
    #[cfg(windows)]
    let words: Vec<_> = normalized.iter().map(Cow::as_ref).collect();
    #[cfg(not(windows))]
    let words: Vec<_> = args.iter().map(OsString::as_os_str).collect();
    let mut parser = Parser::new(Invocation::command(), &words);
    let mut count = 0;
    while let Some(Ok(event)) = parser.next_event() {
        count += usize::from(matches!(event, Event::Arg { .. }));
    }
    // Parse only the options. The declared trailing arguments locate the original
    // command without converting or copying its arguments.
    let command_start = args.len() - count;
    let options = &words[..command_start];
    match Invocation::parse_from(options) {
        Ok(invocation) => Outcome::Parsed((invocation.options, &args[command_start..])),
        Err(Error::Help { cmd, long }) => Outcome::Exit(Exit {
            text: Invocation::render_help(cmd, long).unwrap_or_default(),
            stderr: false,
            code: 0,
        }),
        Err(error) => Outcome::Exit(Exit {
            text: Invocation::render_failure(options, &error),
            stderr: true,
            code: 2,
        }),
    }
}

#[cfg(windows)]
fn slash_switch(arg: &OsStr) -> Cow<'_, OsStr> {
    if !arg.as_encoded_bytes().starts_with(b"/") {
        return Cow::Borrowed(arg);
    }
    match arg.to_str().and_then(|text| text.strip_prefix('/')) {
        Some("?" | "h" | "H") => Cow::Borrowed(OsStr::new("--help")),
        Some(name) if !name.is_empty() => {
            let mut switch = String::with_capacity(name.len() + 2);
            switch.push_str(if name.len() == 1 { "-" } else { "--" });
            switch.push_str(name);
            switch.make_ascii_lowercase();
            Cow::Owned(switch.into())
        }
        _ => Cow::Borrowed(arg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_short_bundles_and_long_flags() {
        let args = ["-disum", "--wait", "32", "-t2", "--timeout=16"].map(OsString::from);
        let (options, command) = parse(&args).parsed().unwrap();
        assert!(
            options.display
                && options.idle
                && options.system
                && options.user_active
                && options.disk
        );
        assert_eq!(options.timeout(), Some(Duration::from_secs(16)));
        assert_eq!(options.wait.map(NonZeroU32::get), Some(32));
        assert!(command.is_empty());
        assert!(parse(&[]).parsed().unwrap().0.keep_idle());
    }

    #[test]
    fn keeps_the_original_command_arguments() {
        let mut args = ["-d", "--", "/tool", "/TIMEOUT", "--", "", "café", "-h"]
            .map(OsString::from)
            .to_vec();
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            args.push(OsString::from_wide(&[0xd800]));
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            args.push(OsString::from_vec(vec![0xff]));
        }
        let (options, command) = parse(&args).parsed().unwrap();
        assert!(options.display);
        assert_eq!(command, &args[2..]);

        let args = ["tool.exe", "--", "/?", "/d"].map(OsString::from);
        assert_eq!(parse(&args).parsed().unwrap().1, &args);
    }

    #[cfg(unix)]
    #[test]
    fn accepts_absolute_command_paths() {
        let args = ["/bin/sh", "-c", "exit 7"].map(OsString::from);
        assert_eq!(parse(&args).parsed().unwrap().1, &args);
    }

    #[cfg(windows)]
    #[test]
    fn accepts_slash_switches() {
        let args = ["/DISPLAY", "/T", "2", "/WAIT", "32"].map(OsString::from);
        let (options, _) = parse(&args).parsed().unwrap();
        assert!(options.display);
        assert_eq!(options.timeout(), Some(Duration::from_secs(2)));
        assert_eq!(options.wait.map(NonZeroU32::get), Some(32));
    }

    #[test]
    fn uses_generated_help_and_diagnostics() {
        let help = ["--help", "-h"];
        #[cfg(windows)]
        let help = ["/?", "/HELP", help[0], help[1]];
        for help in help {
            let args = [help.into()];
            let outcome = parse(&args);
            let exit = outcome.exit().unwrap();
            assert_eq!(exit.code, 0, "{help}");
            assert!(!exit.stderr && exit.text.contains("--timeout"));
        }
        for (switch, value) in [
            ("--wait", "0"),
            ("--wait", "4294967296"),
            ("-w", "-1"),
            ("--timeout", "bad"),
            ("-t", "-1"),
            ("-t", "7seconds"),
            ("-t", "0x10"),
        ] {
            let args = [switch.into(), value.into()];
            let outcome = parse(&args);
            let exit = outcome.exit().unwrap();
            assert_eq!(exit.code, 2, "{switch} {value}");
            assert!(exit.stderr);
        }
    }
}
