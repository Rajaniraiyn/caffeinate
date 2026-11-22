#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "macos", path = "macos.rs")]
mod backend;

use crate::{cli::Options, runtime::Target};
pub(crate) use backend::{Power, Process};
use rustix::process::Pid;
use signal_hook::{
    consts::{SIGINT, SIGQUIT},
    flag,
};
use std::{io, sync::Arc, thread, time::Duration};
use wait_timeout::ChildExt;

fn checked_pid(pid: u32) -> io::Result<Pid> {
    i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PID is out of range"))
}

pub(crate) fn update(power: &mut Power, _: &Options, active: bool) -> Result<(), String> {
    if !active {
        power.end_activity();
    }
    Ok(())
}

pub(crate) fn wait(
    _: &Power,
    target: &mut Target<'_>,
    duration: Option<Duration>,
) -> Result<bool, String> {
    let duration = duration.map(|value| value.min(Duration::from_secs(86_400)));
    let result = match target {
        Target::Child(child) => match duration {
            Some(duration) => child.wait_timeout(duration).map(|status| status.is_some()),
            None => child.wait().map(|_| true),
        },
        Target::Process(process) => process.wait(duration),
        Target::Idle => {
            match duration {
                Some(duration) => thread::sleep(duration),
                None => thread::park(),
            }
            return Ok(false);
        }
    };
    match result {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
        result => result.map_err(|error| format!("waiting for the process: {error}")),
    }
}

pub(crate) fn protect_terminal() -> io::Result<()> {
    let caught = Arc::default();
    // The child receives these signals independently as part of the foreground job.
    for signal in [SIGINT, SIGQUIT] {
        flag::register(signal, Arc::clone(&caught))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

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
