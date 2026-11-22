use crate::Request;
use dbus::blocking::{BlockingSender, Connection};
use dbus::channel::Sender;
use dbus::{Error, Message};
use std::time::Duration;
use std::{io, os::fd::OwnedFd};

const BUS_TIMEOUT: Duration = Duration::from_secs(2);
const APP: &str = "caffeinate";
const REASON: &str = "Prevent idle sleep";

pub(crate) struct Power {
    _system: Option<OwnedFd>,
    display: Option<Connection>,
    keep_display: bool,
}

impl Power {
    pub(crate) fn start(options: &Request) -> io::Result<Self> {
        let system = if options.system {
            Some(inhibit_system("sleep:idle")?)
        } else if options.keep_idle() {
            Some(inhibit_system("sleep")?)
        } else {
            None
        };
        let display = (options.display || options.user_active)
            .then(|| inhibit_display(options.user_active))
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

fn inhibit_system(what: &str) -> io::Result<OwnedFd> {
    let connection = Connection::new_system()
        .map_err(|error| bus_error(error, "connecting to the system bus"))?;
    request_system(&connection, what)
        .map_err(|error| bus_error(error, "requesting a sleep inhibitor"))
}

fn request_system(connection: &Connection, what: &str) -> Result<OwnedFd, Error> {
    let call = |name, path, interface| {
        connection.with_proxy(name, path, BUS_TIMEOUT).method_call(
            interface,
            "Inhibit",
            (what, APP, REASON, "block"),
        )
    };
    match call(
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    ) {
        Ok((fd,)) => return Ok(fd),
        Err(error) if missing_endpoint(&error) => (),
        Err(error) => return Err(error),
    }
    let (fd,) = call(
        "org.freedesktop.ConsoleKit",
        "/org/freedesktop/ConsoleKit/Manager",
        "org.freedesktop.ConsoleKit.Manager",
    )?;
    Ok(fd)
}

fn missing_endpoint(error: &Error) -> bool {
    matches!(
        error.name(),
        Some(
            "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner"
                | "org.freedesktop.DBus.Error.UnknownObject"
                | "org.freedesktop.DBus.Error.UnknownInterface"
                | "org.freedesktop.DBus.Error.UnknownMethod"
        )
    )
}

fn bus_error(error: Error, operation: &str) -> io::Error {
    use io::ErrorKind::{Other, PermissionDenied, TimedOut, Unsupported};
    let kind = match error.name() {
        Some(
            "org.freedesktop.DBus.Error.AccessDenied"
            | "org.freedesktop.DBus.Error.AuthFailed"
            | "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
        ) => PermissionDenied,
        Some(
            "org.freedesktop.DBus.Error.NoReply"
            | "org.freedesktop.DBus.Error.Timeout"
            | "org.freedesktop.DBus.Error.TimedOut",
        ) => TimedOut,
        _ if missing_endpoint(&error) => Unsupported,
        _ => Other,
    };
    io::Error::new(kind, format!("{operation}: {error}"))
}

const SCREENSAVER: &str = "org.freedesktop.ScreenSaver";
const GNOME: &str = "org.gnome.SessionManager";
const XFCE: &str = "org.xfce.ScreenSaver";
const MATE: &str = "org.mate.ScreenSaver";

fn inhibit_display(wake_display: bool) -> io::Result<Connection> {
    let connection = Connection::new_session()
        .map_err(|error| bus_error(error, "connecting to the desktop session"))?;
    for (name, path) in [
        (XFCE, "/org/xfce/ScreenSaver"),
        (MATE, "/org/mate/ScreenSaver"),
        (SCREENSAVER, "/org/freedesktop/ScreenSaver"),
        (SCREENSAVER, "/ScreenSaver"),
        (GNOME, "/org/gnome/SessionManager"),
    ] {
        let proxy = connection.with_proxy(name, path, BUS_TIMEOUT);
        let result: Result<(u32,), _> = match name {
            // Use private desktop interfaces only when their service is already running.
            XFCE | MATE => {
                let mut request = Message::method_call(
                    &proxy.destination,
                    &proxy.path,
                    &name.into(),
                    &"Inhibit".into(),
                )
                .append2(APP, REASON);
                request.set_auto_start(false);
                connection
                    .send_with_reply_and_block(request, BUS_TIMEOUT)
                    .and_then(|reply| reply.read_all())
            }
            GNOME => proxy.method_call(name, "Inhibit", (APP, 0u32, REASON, 8u32)),
            _ => proxy.method_call(name, "Inhibit", (APP, REASON)),
        };
        match result {
            Ok(_) => {
                if wake_display && name != GNOME {
                    let mut message = Message::method_call(
                        &proxy.destination,
                        &proxy.path,
                        &name.into(),
                        &"SimulateUserActivity".into(),
                    );
                    message.set_no_reply(true);
                    message.set_auto_start(false);
                    if connection.send(message).is_ok() {
                        let _ = connection.channel().read_write(Some(Duration::ZERO));
                    }
                }
                // The inhibitor belongs to this private bus connection. Closing
                // it releases the request without a second synchronous call.
                return Ok(connection);
            }
            Err(error) if missing_endpoint(&error) => (),
            Err(error) => return Err(bus_error(error, name)),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no desktop idle-inhibit service responded",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::channel::{MatchingReceiver, Sender};
    use dbus::message::MatchRule;
    use std::{
        io::Read,
        os::unix::net::UnixStream,
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::Instant,
    };

    #[test]
    fn access_denied_is_not_a_missing_endpoint() {
        let error = Error::new_custom("org.freedesktop.DBus.Error.AccessDenied", "denied");
        assert!(!missing_endpoint(&error));
        assert_eq!(
            bus_error(error, "sleep").kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    #[ignore = "requires a private D-Bus session"]
    fn screen_saver_releases_on_disconnect_and_preserves_child_stdin() {
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
                        match method.as_str() {
                            "Inhibit" => {
                                connection
                                    .send(message.method_return().append1(17u32))
                                    .unwrap();
                            }
                            "SimulateUserActivity" => {
                                assert!(message.get_no_reply());
                            }
                            _ => panic!("unexpected method {method}"),
                        }
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
            let lock = inhibit_display(true).unwrap();
            let client = lock.unique_name().to_string();
            drop(lock);
            for method in ["Inhibit", "SimulateUserActivity"] {
                assert_eq!(calls.recv_timeout(BUS_TIMEOUT).unwrap(), method);
            }
            let observer = Connection::new_session().unwrap();
            let bus =
                observer.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", BUS_TIMEOUT);
            let deadline = Instant::now() + BUS_TIMEOUT;
            while bus
                .method_call::<(bool,), _, _, _>(
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    (client.as_str(),),
                )
                .unwrap()
                == (true,)
            {
                assert!(
                    Instant::now() < deadline,
                    "the inhibitor's bus connection stayed open"
                );
                thread::sleep(Duration::from_millis(5));
            }
            let mut child = Command::new("/bin/sh")
                .args(["-c", "sleep 0.1; exit 37"])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            Request {
                display: true,
                ..Request::default()
            }
            .acquire()
            .unwrap()
            .wait_for_child(&mut child)
            .unwrap();
            assert!(child.stdin.is_some());
            assert_eq!(child.wait().unwrap().code(), Some(37));
            assert_eq!(calls.recv_timeout(BUS_TIMEOUT).unwrap(), "Inhibit");
            finished.send(()).unwrap();
            server.join().unwrap();
        }
    }

    #[test]
    #[ignore = "requires a private D-Bus session"]
    fn consolekit_returns_an_owned_system_inhibitor() {
        let (mut observer, lock) = UnixStream::pair().unwrap();
        observer.set_read_timeout(Some(BUS_TIMEOUT)).unwrap();
        let (ready, started) = mpsc::channel();
        let worker = thread::spawn(move || {
            let server = Connection::new_session().unwrap();
            server
                .request_name("org.freedesktop.ConsoleKit", false, false, true)
                .unwrap();
            let (sent, done) = mpsc::channel();
            let mut lock = Some(OwnedFd::from(lock));
            server.start_receive(
                MatchRule::new_method_call()
                    .with_interface("org.freedesktop.ConsoleKit.Manager")
                    .with_path("/org/freedesktop/ConsoleKit/Manager"),
                Box::new(move |request, server| {
                    assert_eq!(request.member().unwrap().to_string(), "Inhibit");
                    assert_eq!(
                        request.read4::<&str, &str, &str, &str>().unwrap(),
                        ("sleep:idle", APP, REASON, "block")
                    );
                    server
                        .send(request.method_return().append1(lock.take().unwrap()))
                        .unwrap();
                    sent.send(()).unwrap();
                    true
                }),
            );
            ready.send(()).unwrap();
            let deadline = Instant::now() + BUS_TIMEOUT;
            while done.try_recv().is_err() {
                assert!(
                    Instant::now() < deadline,
                    "no ConsoleKit inhibitor requested"
                );
                server.process(Duration::from_millis(20)).unwrap();
            }
            server.channel().flush();
        });
        started.recv_timeout(BUS_TIMEOUT).unwrap();
        let request = request_system(&Connection::new_session().unwrap(), "sleep:idle").unwrap();
        worker.join().unwrap();
        drop(request);
        assert_eq!(observer.read(&mut [0]).unwrap(), 0);
    }
}
