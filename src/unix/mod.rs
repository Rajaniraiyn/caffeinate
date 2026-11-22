#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use crate::cli::Options;
#[cfg(target_os = "linux")]
use linux::{Power, Process};
#[cfg(target_os = "macos")]
use macos::{Power, Process};
use signal_hook::{
    SigId,
    consts::{SIGINT, SIGQUIT},
    flag, low_level,
};
use std::ffi::OsString;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

const ACTIVITY_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run(options: &Options, command: &[OsString]) -> i32 {
    run_inner(options, command).unwrap_or_else(|error| {
        eprintln!("caffeinate: {error}");
        1
    })
}

fn run_inner(options: &Options, command: &[OsString]) -> Result<i32, String> {
    if let Some((program, args)) = command.split_first() {
        let power = Power::start(options)?;
        let started = Instant::now();
        let _signals = TerminalSignals::listen().map_err(|error| error.to_string())?;
        let mut child = match Command::new(program).args(args).spawn() {
            Ok(child) => child,
            Err(error) => {
                eprintln!(
                    "caffeinate: could not start {}: {error}",
                    Path::new(program).display()
                );
                return Ok(if error.kind() == io::ErrorKind::NotFound {
                    127
                } else {
                    126
                });
            }
        };

        if options.timeout().is_none() && !options.user_active {
            let status = child.wait();
            drop(power);
            return child_status(status);
        }

        let result = wait(options, started, power, true, |duration| match duration {
            Some(duration) => child.wait_timeout(duration).map(|status| status.is_some()),
            None => child.wait().map(|_| true),
        });
        if let Err(error) = result {
            eprintln!("caffeinate: {error}");
        }
        return child_status(child.wait());
    }

    let process = match options.wait {
        Some(pid) => match Process::open(pid.get())
            .map_err(|error| format!("watching process {pid}: {error}"))?
        {
            Some(process) => Some(process),
            None => return Ok(0),
        },
        None => None,
    };
    let power = Power::start(options)?;
    wait(
        options,
        Instant::now(),
        power,
        process.is_some(),
        |duration| {
            if let Some(process) = &process {
                return process.wait(duration);
            }
            match duration {
                Some(duration) => thread::sleep(duration),
                None => thread::park(),
            }
            Ok(false)
        },
    )?;
    Ok(0)
}

fn child_status(status: io::Result<std::process::ExitStatus>) -> Result<i32, String> {
    let status = status.map_err(|error| format!("waiting for the command: {error}"))?;
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

fn wait(
    options: &Options,
    started: Instant,
    mut power: Power,
    watching_process: bool,
    mut wait_for_exit: impl FnMut(Option<Duration>) -> io::Result<bool>,
) -> Result<(), String> {
    let timeout = options.timeout();
    let activity_only =
        !watching_process && !(options.display || options.idle || options.system || options.disk);
    let mut activity = options
        .user_active
        .then_some(timeout.unwrap_or(ACTIVITY_TIMEOUT));
    let timeout = timeout.or(activity.filter(|_| activity_only));

    loop {
        let elapsed = started.elapsed();
        let remaining = timeout.map(|limit| limit.saturating_sub(elapsed));
        if remaining == Some(Duration::ZERO) {
            return Ok(());
        }

        let mut user_remaining = activity.map(|limit| limit.saturating_sub(elapsed));
        if user_remaining == Some(Duration::ZERO) {
            power.end_activity();
            activity = None;
            user_remaining = None;
        }

        let duration = remaining
            .into_iter()
            .chain(user_remaining)
            .min()
            .map(|duration| duration.min(Duration::from_secs(86_400)));
        match wait_for_exit(duration) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("waiting for the process: {error}")),
        }
    }
}

struct TerminalSignals([Option<SigId>; 2]);

impl TerminalSignals {
    fn listen() -> io::Result<Self> {
        let mut scope = Self([None; 2]);
        let caught = Arc::default();
        // A terminal sends these to the whole foreground job, including the child.
        for (slot, signal) in scope.0.iter_mut().zip([SIGINT, SIGQUIT]) {
            *slot = Some(flag::register(signal, Arc::clone(&caught))?);
        }
        Ok(scope)
    }
}

impl Drop for TerminalSignals {
    fn drop(&mut self) {
        for id in self.0.into_iter().flatten() {
            low_level::unregister(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    #[test]
    fn process_watch_waits_until_exit() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "read line"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let process = Process::open(child.id()).unwrap().unwrap();
        let was_running = !process.wait(Some(Duration::ZERO)).unwrap();
        drop(child.stdin.take());
        let exited = process.wait(Some(Duration::from_secs(5)));
        let _ = child.wait();
        assert!(was_running);
        assert!(exited.unwrap());
    }
}
