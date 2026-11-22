use super::checked_pid;
use crate::cli::Options;
use objc2_core_foundation::CFString;
use objc2_io_kit::{IOPMAssertionCreateWithDescription, IOPMAssertionID, IOPMAssertionRelease};
use rustix::event::kqueue::{Event, EventFilter, EventFlags, ProcessEvents, kevent, kqueue};
use rustix::io::Errno;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::ptr::null_mut;
use std::time::Duration;

struct Assertion(IOPMAssertionID);

impl Assertion {
    fn new(kind: &'static str, timeout: Option<Duration>) -> Result<Self, String> {
        let assertion_type = CFString::from_static_str(kind);
        let name = CFString::from_static_str("caffeinate");
        let action = CFString::from_static_str("TimeoutActionRelease");
        let mut id = 0;
        // SAFETY: The strings live through the call and id is writable.
        let result = unsafe {
            IOPMAssertionCreateWithDescription(
                Some(&assertion_type),
                Some(&name),
                None,
                None,
                None,
                timeout.map_or(0.0, |duration| duration.as_secs_f64()),
                Some(&action),
                &mut id,
            )
        };
        if result != 0 {
            return Err(format!(
                "could not create {kind} assertion (IOKit {:#010x})",
                result as u32
            ));
        }
        Ok(Self(id))
    }
}

impl Drop for Assertion {
    fn drop(&mut self) {
        IOPMAssertionRelease(self.0);
    }
}

pub(crate) struct Power {
    _assertions: [Option<Assertion>; 4],
    activity: Option<Assertion>,
}

impl Power {
    pub(crate) fn start(options: &Options) -> Result<Self, String> {
        let timeout = options.timeout();
        let assertions = [
            (options.keep_idle(), "PreventUserIdleSystemSleep"),
            (options.display, "PreventUserIdleDisplaySleep"),
            (options.system, "PreventSystemSleep"),
            (options.disk, "PreventDiskIdle"),
        ];
        let mut held = [const { None }; 4];
        for (slot, (requested, kind)) in held.iter_mut().zip(assertions) {
            if requested {
                *slot = Some(Assertion::new(kind, timeout)?);
            }
        }
        let activity = options
            .activity_timeout()
            .map(|timeout| Assertion::new("UserIsActive", Some(timeout)))
            .transpose()?;
        Ok(Self {
            _assertions: held,
            activity,
        })
    }

    pub(crate) fn end_activity(&mut self) {
        self.activity = None;
    }
}

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
