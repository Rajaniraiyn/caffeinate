use super::checked_pid;
use crate::cli::Options;
use dbus::blocking::Connection;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::process::{PidfdFlags, pidfd_open};
use std::io;
use std::os::fd::OwnedFd;
use std::time::Duration;

const BUS_TIMEOUT: Duration = Duration::from_secs(2);
const APP: &str = "caffeinate";
const REASON: &str = "Prevent idle sleep";

pub(super) struct Power {
    _system: Option<OwnedFd>,
    display: Option<DisplayLock>,
    keep_display: bool,
}

impl Power {
    pub(super) fn start(options: &Options) -> Result<Self, String> {
        if options.disk {
            return Err("Disk idle requests are unavailable.".into());
        }

        let system = if options.system {
            Some(inhibit_system("sleep:idle")?)
        } else if options.keep_idle() {
            Some(inhibit_system("sleep")?)
        } else {
            None
        };
        let display = (options.display || options.user_active)
            .then(|| DisplayLock::new(options.user_active))
            .transpose()?;
        Ok(Self {
            _system: system,
            display,
            keep_display: options.display,
        })
    }

    pub(super) fn end_activity(&mut self) {
        if !self.keep_display {
            self.display = None;
        }
    }
}

fn inhibit_system(what: &str) -> Result<OwnedFd, String> {
    let connection = Connection::new_system()
        .map_err(|error| format!("connecting to the system bus: {error}"))?;
    let proxy = connection.with_proxy(
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        BUS_TIMEOUT,
    );
    let (fd,): (OwnedFd,) = proxy
        .method_call(
            "org.freedesktop.login1.Manager",
            "Inhibit",
            (what, APP, REASON, "block"),
        )
        .map_err(|error| format!("requesting a login1 sleep inhibitor: {error}"))?;
    Ok(fd)
}

const SCREENSAVER: &str = "org.freedesktop.ScreenSaver";
const GNOME: &str = "org.gnome.SessionManager";

struct DisplayLock {
    connection: Connection,
    name: &'static str,
    path: &'static str,
    cookie: u32,
}

impl DisplayLock {
    fn new(wake_display: bool) -> Result<Self, String> {
        let connection = Connection::new_session()
            .map_err(|error| format!("connecting to the desktop session: {error}"))?;
        let mut last_error = None;
        for (name, path) in [
            (SCREENSAVER, "/org/freedesktop/ScreenSaver"),
            (SCREENSAVER, "/ScreenSaver"),
            (GNOME, "/org/gnome/SessionManager"),
        ] {
            let proxy = connection.with_proxy(name, path, BUS_TIMEOUT);
            let result: Result<(u32,), _> = if name == GNOME {
                // GNOME's idle-inhibit flag is 8. This client has no window.
                proxy.method_call(name, "Inhibit", (APP, 0u32, REASON, 8u32))
            } else {
                proxy.method_call(name, "Inhibit", (APP, REASON))
            };
            match result {
                Ok((cookie,)) => {
                    if wake_display && name == SCREENSAVER {
                        let _: Result<(), _> = proxy.method_call(name, "SimulateUserActivity", ());
                    }
                    return Ok(Self {
                        connection,
                        name,
                        path,
                        cookie,
                    });
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(format!(
            "no desktop idle-inhibit service responded: {}",
            last_error.expect("services were tried")
        ))
    }
}

impl Drop for DisplayLock {
    fn drop(&mut self) {
        let proxy = self
            .connection
            .with_proxy(self.name, self.path, BUS_TIMEOUT);
        let method = if self.name == GNOME {
            "Uninhibit"
        } else {
            "UnInhibit"
        };
        let _: Result<(), _> = proxy.method_call(self.name, method, (self.cookie,));
    }
}

pub(super) struct Process(OwnedFd);

impl Process {
    pub(super) fn open(pid: u32) -> io::Result<Option<Self>> {
        match pidfd_open(checked_pid(pid)?, PidfdFlags::empty()) {
            Ok(fd) => Ok(Some(Self(fd))),
            Err(Errno::SRCH) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn wait(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let mut fds = [PollFd::new(&self.0, PollFlags::IN)];
        let timeout = timeout.map(|duration| Timespec {
            tv_sec: duration.as_secs() as i64,
            tv_nsec: duration.subsec_nanos().into(),
        });
        Ok(poll(&mut fds, timeout.as_ref())? > 0)
    }
}
