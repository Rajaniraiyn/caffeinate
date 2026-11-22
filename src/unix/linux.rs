use super::checked_pid;
use crate::Request;
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

pub(crate) struct Power {
    _system: Option<OwnedFd>,
    display: Option<DisplayLock>,
    keep_display: bool,
}

impl Power {
    pub(crate) fn start(options: &Request) -> Result<Self, String> {
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

    pub(crate) fn end_activity(&mut self) {
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
const XFCE: &str = "org.xfce.ScreenSaver";
const MATE: &str = "org.mate.ScreenSaver";

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
        let bus =
            connection.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", BUS_TIMEOUT);
        let mut last_error = None;
        for (name, path) in [
            (XFCE, "/org/xfce/ScreenSaver"),
            (MATE, "/org/mate/ScreenSaver"),
            (SCREENSAVER, "/org/freedesktop/ScreenSaver"),
            (SCREENSAVER, "/ScreenSaver"),
            (GNOME, "/org/gnome/SessionManager"),
        ] {
            // A session can have other desktops installed. Use their private
            // interfaces only when a service is already running.
            if matches!(name, XFCE | MATE)
                && !matches!(
                    bus.method_call("org.freedesktop.DBus", "NameHasOwner", (name,)),
                    Ok((true,))
                )
            {
                continue;
            }
            let proxy = connection.with_proxy(name, path, BUS_TIMEOUT);
            let result: Result<(u32,), _> = if name == GNOME {
                // GNOME's idle-inhibit flag is 8. This client has no window.
                proxy.method_call(name, "Inhibit", (APP, 0u32, REASON, 8u32))
            } else {
                proxy.method_call(name, "Inhibit", (APP, REASON))
            };
            match result {
                Ok((cookie,)) => {
                    if wake_display && name != GNOME {
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

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::channel::{MatchingReceiver, Sender};
    use dbus::message::MatchRule;
    use std::{sync::mpsc, thread, time::Instant};

    #[test]
    #[ignore = "requires a private D-Bus session"]
    fn linux_screen_saver_acquires_refreshes_and_releases() {
        for (name, path) in [
            ("org.xfce.ScreenSaver", "/org/xfce/ScreenSaver"),
            ("org.mate.ScreenSaver", "/org/mate/ScreenSaver"),
        ] {
            let (ready, started) = mpsc::channel();
            let (record, calls) = mpsc::channel();
            let (finished, stop) = mpsc::channel();
            let server = thread::spawn(move || {
                let connection = Connection::new_session().unwrap();
                connection.request_name(name, false, false, true).unwrap();
                let rule = MatchRule::new_method_call()
                    .with_interface(name)
                    .with_path(path);
                connection.start_receive(
                    rule,
                    Box::new(move |message, connection| {
                        let method = message.member().unwrap().to_string();
                        let mut reply = message.method_return();
                        match method.as_str() {
                            "Inhibit" => reply = reply.append1(17u32),
                            "UnInhibit" => assert_eq!(message.read1::<u32>().unwrap(), 17),
                            _ => (),
                        }
                        connection.send(reply).unwrap();
                        record.send(method).unwrap();
                        true
                    }),
                );
                ready.send(()).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while stop.try_recv().is_err() && Instant::now() < deadline {
                    connection.process(Duration::from_millis(20)).unwrap();
                }
            });
            started.recv_timeout(BUS_TIMEOUT).unwrap();
            let lock = DisplayLock::new(true).unwrap();
            drop(lock);
            for method in ["Inhibit", "SimulateUserActivity", "UnInhibit"] {
                assert_eq!(calls.recv_timeout(BUS_TIMEOUT).unwrap(), method);
            }
            finished.send(()).unwrap();
            server.join().unwrap();
        }
    }
}

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
        let timeout = timeout.map(|duration| Timespec {
            tv_sec: duration.as_secs() as i64,
            tv_nsec: duration.subsec_nanos().into(),
        });
        Ok(poll(&mut fds, timeout.as_ref())? > 0)
    }
}
