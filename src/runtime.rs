use crate::cli::Options;
use crate::platform::{self, Power, Process};
use std::{
    ffi::OsString,
    io,
    path::Path,
    process::{Child, Command, ExitStatus},
    time::{Duration, Instant},
};

pub(crate) enum Target<'a> {
    Idle,
    Process(&'a Process),
    Child(&'a mut Child),
}

pub fn run(options: &Options, command: &[OsString]) -> i32 {
    #[cfg(windows)]
    if options.disk {
        eprintln!("caffeinate: Disk idle requests are unavailable.");
        return windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32;
    }

    execute(options, command).unwrap_or_else(|error| {
        eprintln!("caffeinate: {error}");
        1
    })
}

fn execute(options: &Options, command: &[OsString]) -> Result<i32, String> {
    let command = command.split_first();
    let process = match (command.is_none(), options.wait) {
        (true, Some(pid)) => match Process::open(pid.get())
            .map_err(|error| format!("watching process {pid}: {error}"))?
        {
            Some(process) => Some(process),
            None => return Ok(0),
        },
        _ => None,
    };

    #[cfg(windows)]
    let started = Instant::now();
    let power = Power::start(options)?;
    #[cfg(unix)]
    let started = Instant::now();

    let Some((program, args)) = command else {
        let target = process.as_ref().map_or(Target::Idle, Target::Process);
        return hold(options, power, started, target).map(|()| 0);
    };
    platform::protect_terminal()
        .map_err(|error| format!("setting up terminal controls: {error}"))?;
    let mut child = match Command::new(program).args(args).spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!(
                "caffeinate: could not start {}: {error}",
                Path::new(program).display()
            );
            return Ok(launch_status(&error));
        }
    };
    if (options.timeout().is_some() || options.user_active || cfg!(windows) && options.system)
        && let Err(error) = hold(options, power, started, Target::Child(&mut child))
    {
        eprintln!("caffeinate: {error}");
    }
    child
        .wait()
        .map(exit_status)
        .map_err(|error| format!("waiting for the command: {error}"))
}

fn hold(
    options: &Options,
    mut power: Power,
    started: Instant,
    mut target: Target<'_>,
) -> Result<(), String> {
    let activity = options.activity_timeout();
    let activity_only = matches!(target, Target::Idle)
        && !(options.display || options.idle || options.system || options.disk);
    let timeout = options.timeout().or(activity.filter(|_| activity_only));
    loop {
        let elapsed = started.elapsed();
        let remaining = timeout.map(|limit| limit.saturating_sub(elapsed));
        if remaining == Some(Duration::ZERO) {
            return Ok(());
        }
        let active = activity
            .and_then(|limit| limit.checked_sub(elapsed))
            .filter(|duration| !duration.is_zero());
        platform::update(&mut power, options, active.is_some())?;
        if platform::wait(
            &power,
            &mut target,
            remaining.into_iter().chain(active).min(),
        )? {
            return Ok(());
        }
    }
}

fn launch_status(error: &io::Error) -> i32 {
    let missing = error.kind() == io::ErrorKind::NotFound;
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
        error.raw_os_error().unwrap_or(if missing {
            ERROR_FILE_NOT_FOUND as i32
        } else {
            1
        })
    }
    #[cfg(unix)]
    {
        if missing { 127 } else { 126 }
    }
}

fn exit_status(status: ExitStatus) -> i32 {
    status.code().unwrap_or({
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            128 + status.signal().unwrap_or(0)
        }
        #[cfg(windows)]
        1
    })
}
