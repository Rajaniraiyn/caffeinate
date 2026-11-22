use crate::cli::Options;
use caffeinate::Request;
use std::{
    ffi::OsString,
    io,
    path::Path,
    process::{Command, ExitStatus},
};

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

fn execute(options: &Options, command: &[OsString]) -> io::Result<i32> {
    let mut request = Request::default();
    request.display = options.display;
    request.idle = options.idle;
    request.system = options.system;
    request.disk = options.disk;
    request.user_active = options.user_active;
    request.timeout = options.timeout();

    let Some((program, args)) = command.split_first() else {
        match options.wait {
            Some(pid) => request
                .wait_for_process(pid)
                .map_err(|error| io::Error::other(format!("watching process {pid}: {error}")))?,
            None => request.acquire()?.wait()?,
        }
        return Ok(0);
    };

    let inhibitor = request.acquire()?;
    protect_terminal()
        .map_err(|error| io::Error::other(format!("setting up terminal controls: {error}")))?;
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
    if let Err(error) = inhibitor.wait_for_child(&mut child) {
        eprintln!("caffeinate: {error}");
    }
    child
        .wait()
        .map(exit_status)
        .map_err(|error| io::Error::other(format!("waiting for the command: {error}")))
}

#[cfg(unix)]
fn protect_terminal() -> io::Result<()> {
    use signal_hook::{
        consts::{SIGINT, SIGQUIT},
        flag,
    };
    use std::sync::Arc;

    let caught = Arc::default();
    // The child receives these signals independently as part of the foreground job.
    for signal in [SIGINT, SIGQUIT] {
        flag::register(signal, Arc::clone(&caught))?;
    }
    Ok(())
}

#[cfg(windows)]
fn protect_terminal() -> io::Result<()> {
    use windows_sys::Win32::System::Console::{
        CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler,
    };

    // The child receives the same console event as the wrapper.
    unsafe extern "system" fn utility_control(event: u32) -> i32 {
        i32::from(matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT))
    }
    if unsafe { SetConsoleCtrlHandler(Some(utility_control), 1) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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
