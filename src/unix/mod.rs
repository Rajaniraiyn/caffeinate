#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(any(target_os = "linux", target_os = "freebsd"), path = "dbus.rs")]
mod power;
#[cfg_attr(target_os = "linux", path = "pidfd.rs")]
#[cfg_attr(any(target_os = "macos", target_os = "freebsd"), path = "kqueue.rs")]
mod process;

use crate::{Request, Target};
pub(crate) use power::Power;
pub(crate) use process::Process;
#[cfg(target_os = "linux")]
pub(crate) use process::pidfd_unavailable;
use rustix::event::{Timespec, poll};
use rustix::process::Pid;
use std::{io, time::Duration};

fn checked_pid(pid: u32) -> io::Result<Pid> {
    i32::try_from(pid)
        .ok()
        .and_then(Pid::from_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PID is out of range"))
}

pub(crate) fn update(power: &mut Power, _: &Request, active: bool) -> io::Result<()> {
    if !active {
        power.end_activity();
    }
    Ok(())
}

pub(crate) fn wait(
    _: &Power,
    target: &mut Target<'_>,
    duration: Option<Duration>,
) -> io::Result<bool> {
    let duration = duration.map(|value| value.min(Duration::from_secs(86_400)));
    let result = match target {
        Target::Process(process) => process.wait(duration),
        Target::Idle => {
            let timeout = duration
                .map(Timespec::try_from)
                .transpose()
                .map_err(io::Error::other)?;
            poll(&mut [], timeout.as_ref())
                .map(|_| false)
                .map_err(Into::into)
        }
        #[cfg(target_os = "linux")]
        Target::Child(child) => process::wait_for_legacy_child(child, duration),
    };
    match result {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
        result => result,
    }
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
