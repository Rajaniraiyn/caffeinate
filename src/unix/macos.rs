use super::ACTIVITY_TIMEOUT;
use crate::cli::Options;
use nix::{
    errno::Errno,
    libc::timespec,
    sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue},
};
use objc2_core_foundation::CFString;
use objc2_io_kit::{IOPMAssertionCreateWithDescription, IOPMAssertionID, IOPMAssertionRelease};
use std::io;
use std::time::Duration;

struct Assertion(IOPMAssertionID);

impl Assertion {
    fn new(kind: &str, timeout: Option<Duration>) -> Result<Self, String> {
        let assertion_type = CFString::from_str(kind);
        let name = CFString::from_str("caffeinate");
        let action = CFString::from_str("TimeoutActionRelease");
        let mut id = 0;
        // SAFETY: All borrowed strings live through the call and id is writable.
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

pub(super) struct Power {
    _assertions: [Option<Assertion>; 4],
    activity: Option<Assertion>,
}

impl Power {
    pub(super) fn start(options: &Options) -> Result<Self, String> {
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
            .user_active
            .then(|| Assertion::new("UserIsActive", Some(timeout.unwrap_or(ACTIVITY_TIMEOUT))))
            .transpose()?;
        Ok(Self {
            _assertions: held,
            activity,
        })
    }

    pub(super) fn end_activity(&mut self) {
        self.activity = None;
    }
}

pub(super) struct Process(Kqueue);

impl Process {
    pub(super) fn open(pid: u32) -> io::Result<Option<Self>> {
        let pid = i32::try_from(pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PID is out of range"))?;
        let queue = Kqueue::new()?;
        let change = KEvent::new(
            pid as usize,
            EventFilter::EVFILT_PROC,
            EvFlags::EV_ADD | EvFlags::EV_ONESHOT,
            FilterFlag::NOTE_EXIT,
            0,
            0,
        );
        match queue.kevent(&[change], &mut [], None) {
            Ok(_) => Ok(Some(Self(queue))),
            Err(Errno::ESRCH) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let timeout = timeout.map(|duration| timespec {
            tv_sec: duration.as_secs() as i64,
            tv_nsec: duration.subsec_nanos().into(),
        });
        let mut events = [KEvent::new(
            0,
            EventFilter::EVFILT_PROC,
            EvFlags::empty(),
            FilterFlag::empty(),
            0,
            0,
        )];
        Ok(self.0.kevent(&[], &mut events, timeout)? > 0)
    }
}
