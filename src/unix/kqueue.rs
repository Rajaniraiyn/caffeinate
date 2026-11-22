use super::checked_pid;
use rustix::event::kqueue::{Event, EventFilter, EventFlags, ProcessEvents, kevent, kqueue};
use rustix::io::Errno;
use std::{io, mem::MaybeUninit, os::fd::OwnedFd, ptr::null_mut, time::Duration};

pub(crate) struct Process(OwnedFd);

impl Process {
    pub(crate) fn open(pid: u32) -> io::Result<Option<Self>> {
        let change = Event::new(
            EventFilter::Proc {
                pid: checked_pid(pid)?,
                flags: ProcessEvents::EXIT,
            },
            EventFlags::ADD | EventFlags::ONESHOT,
            null_mut(),
        );
        let queue = kqueue()?;
        let mut events: [Event; 0] = [];
        // SAFETY: The event refers to a PID and carries no data pointer.
        match unsafe {
            kevent(
                &queue,
                &[change],
                events.as_mut_slice(),
                Some(Duration::ZERO),
            )
        } {
            Ok(_) => Ok(Some(Self(queue))),
            Err(Errno::SRCH) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let mut events = [MaybeUninit::uninit()];
        // SAFETY: No registrations or data pointers are passed to this call.
        let (ready, _) = unsafe { kevent(&self.0, &[], events.as_mut_slice(), timeout) }?;
        Ok(!ready.is_empty())
    }
}
