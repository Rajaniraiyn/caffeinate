use crate::cli::Options;
use crate::power::{PowerRequests, PowerSource, reset_display_timer};
use std::ffi::OsString;
use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::process::Command;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows_sys::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, Sleep, WaitForMultipleObjects, WaitForSingleObject,
};
use windows_sys::core::BOOL;

pub fn run(options: &Options, command: &[OsString]) -> i32 {
    if options.disk {
        eprintln!("caffeinate: Disk idle requests are unavailable.");
        return ERROR_NOT_SUPPORTED as i32;
    }

    match execute(options, command) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("caffeinate: {error}");
            1
        }
    }
}

fn execute(options: &Options, command: &[OsString]) -> Result<i32, String> {
    if let Some((program, args)) = command.split_first() {
        let session = Session::start(options)?;
        if unsafe { SetConsoleCtrlHandler(Some(utility_control), 1) } == 0 {
            return Err(format!(
                "Setting the console control handler failed. {}",
                io::Error::last_os_error()
            ));
        }
        let mut child = match Command::new(program).args(args).spawn() {
            Ok(child) => child,
            Err(error) => {
                eprintln!(
                    "caffeinate: Could not start '{}'. {error}",
                    program.to_string_lossy()
                );
                return Ok(error.raw_os_error().unwrap_or_else(|| {
                    if error.kind() == io::ErrorKind::NotFound {
                        ERROR_FILE_NOT_FOUND as i32
                    } else {
                        1
                    }
                }));
            }
        };
        if options.timeout().is_some() || options.system || options.user_active {
            if let Err(error) = session.wait(options, Some(child.as_handle())) {
                eprintln!("caffeinate: {error}");
            }
        } else {
            let status = child.wait();
            drop(session);
            return command_status(status);
        }
        return command_status(child.wait());
    }

    let process = match options.wait {
        Some(pid) => {
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid.get()) };
            if handle.is_null() {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                    return Ok(0);
                }
                return Err(format!("Opening process {pid} failed. {error}"));
            }
            // SAFETY: OpenProcess returned an owned handle.
            Some(unsafe { OwnedHandle::from_raw_handle(handle) })
        }
        None => None,
    };
    Session::start(options)?.wait(options, process.as_ref().map(AsHandle::as_handle))?;
    Ok(0)
}

fn command_status(status: io::Result<std::process::ExitStatus>) -> Result<i32, String> {
    status
        .map(|status| status.code().unwrap_or(1))
        .map_err(|error| format!("Waiting for the command failed. {error}"))
}

// The child shares the console and handles its own Ctrl+C and Ctrl+Break events.
unsafe extern "system" fn utility_control(event: u32) -> BOOL {
    i32::from(matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT))
}

struct Session {
    started: Instant,
    source: Option<PowerSource>,
    requests: Option<PowerRequests>,
}

impl Session {
    fn start(options: &Options) -> Result<Self, String> {
        let mut session = Self {
            started: Instant::now(),
            source: options.system.then(PowerSource::subscribe).transpose()?,
            requests: None,
        };
        session.refresh(options, options.user_active)?;
        if options.user_active {
            reset_display_timer()?;
        }
        Ok(session)
    }

    fn refresh(&mut self, options: &Options, user_active: bool) -> Result<(), String> {
        let on_ac = self
            .source
            .as_ref()
            .map(PowerSource::on_ac)
            .transpose()?
            .unwrap_or(false);
        let display = options.display || user_active;
        let system = options.keep_idle() || display || on_ac;
        if system {
            let requests = match &mut self.requests {
                Some(requests) => requests,
                slot => slot.insert(PowerRequests::new()?),
            };
            requests.set_display(display)?;
        } else {
            self.requests = None;
        }
        Ok(())
    }

    fn wait(
        mut self,
        options: &Options,
        process: Option<BorrowedHandle<'_>>,
    ) -> Result<(), String> {
        let timeout = options.timeout();
        let user_timeout = options
            .user_active
            .then_some(timeout.unwrap_or(Duration::from_secs(5)));
        let activity_only =
            process.is_none() && !(options.display || options.keep_idle() || options.system);
        let timeout = timeout.or(user_timeout.filter(|_| activity_only));
        loop {
            let elapsed = self.started.elapsed();
            let remaining = timeout.map(|timeout| timeout.saturating_sub(elapsed));
            if remaining == Some(Duration::ZERO) {
                // Dropping this session releases requests before the caller waits for the command.
                return Ok(());
            }
            let user_remaining = user_timeout
                .map(|timeout| timeout.saturating_sub(elapsed))
                .filter(|remaining| !remaining.is_zero());
            self.refresh(options, user_remaining.is_some())?;
            let deadline = remaining.into_iter().chain(user_remaining).min();
            let changed = self.source.as_ref().map(PowerSource::event);
            if wait_for_process(process, changed, wait_millis(deadline))? {
                return Ok(());
            }
        }
    }
}

fn wait_for_process(
    process: Option<BorrowedHandle<'_>>,
    changed: Option<BorrowedHandle<'_>>,
    milliseconds: u32,
) -> Result<bool, String> {
    let process = process.as_ref().map(AsRawHandle::as_raw_handle);
    let changed = changed.as_ref().map(AsRawHandle::as_raw_handle);
    let result = unsafe {
        match (process, changed) {
            (Some(process), Some(changed)) => {
                WaitForMultipleObjects(2, [process, changed].as_ptr(), 0, milliseconds)
            }
            (Some(handle), None) | (None, Some(handle)) => {
                WaitForSingleObject(handle, milliseconds)
            }
            (None, None) => {
                Sleep(milliseconds);
                return Ok(false);
            }
        }
    };
    match result {
        WAIT_OBJECT_0 => Ok(process.is_some()),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(format!("Waiting failed. {}", io::Error::last_os_error())),
        event if event == WAIT_OBJECT_0 + 1 && changed.is_some() => Ok(false),
        event => Err(format!("Unexpected wait result {event}.")),
    }
}

fn wait_millis(duration: Option<Duration>) -> u32 {
    duration.map_or(INFINITE, |value| {
        value
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u128::from(INFINITE - 1)) as u32
    })
}
