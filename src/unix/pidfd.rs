use super::checked_pid;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::process::{PidfdFlags, pidfd_open};
use std::{io, os::fd::OwnedFd, process::Child, time::Duration};

pub(crate) struct Process(OwnedFd);

impl Process {
    pub(crate) fn open(pid: u32) -> io::Result<Option<Self>> {
        match pidfd_open(checked_pid(pid)?, PidfdFlags::empty()) {
            Ok(fd) => Ok(Some(Self(fd))),
            Err(Errno::SRCH) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let mut fds = [PollFd::new(&self.0, PollFlags::IN)];
        let timeout = timeout
            .map(Timespec::try_from)
            .transpose()
            .map_err(io::Error::other)?;
        Ok(poll(&mut fds, timeout.as_ref())? > 0)
    }
}

pub(crate) fn pidfd_unavailable(error: &io::Error) -> bool {
    let code = error.raw_os_error();
    code == Some(Errno::NOSYS.raw_os_error()) || code == Some(Errno::PERM.raw_os_error())
}

pub(crate) fn wait_for_legacy_child(
    child: &mut Child,
    timeout: Option<Duration>,
) -> io::Result<bool> {
    if child.try_wait()?.is_some() {
        return Ok(true);
    }
    let interval = Duration::from_millis(100);
    std::thread::sleep(timeout.map_or(interval, |duration| duration.min(interval)));
    child.try_wait().map(|status| status.is_some())
}
