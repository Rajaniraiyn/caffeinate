//! Keep a computer awake with the operating system's power APIs.
//!
//! ```no_run
//! use caffeinate::Request;
//!
//! let awake = Request::default().acquire()?;
//! // Do work that needs the computer to stay awake.
//! drop(awake);
//! # Ok::<(), std::io::Error>(())
//! ```

#[cfg(not(any(
    windows,
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd"
)))]
compile_error!("caffeinate supports Windows, macOS, Linux, and FreeBSD");

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
#[path = "unix/mod.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows/mod.rs"]
mod platform;

use std::{
    io,
    num::NonZeroU32,
    process::Child,
    time::{Duration, Instant},
};

use platform::{Power, Process};

/// Power requests to make. The default inhibits idle sleep.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Request {
    /// Inhibit idle sleep, including when other assertions are enabled.
    pub idle: bool,
    /// Keep the display on.
    pub display: bool,
    /// Request system sleep inhibition. On Windows and macOS this applies on AC power.
    pub system: bool,
    /// Inhibit disk idle on macOS.
    pub disk: bool,
    /// Signal user activity for five seconds or for the configured timeout.
    pub user_active: bool,
    /// Limit the request when using a wait method.
    pub timeout: Option<Duration>,
}

impl Request {
    /// Acquire power requests and release them when the returned guard is dropped.
    ///
    /// Use a wait method to enforce timeout and display activity limits across
    /// platforms, and to track AC power changes on Windows. macOS can also
    /// release timed assertions on its own. Acquiring a guard starts no
    /// background thread.
    pub fn acquire(self) -> io::Result<Inhibitor> {
        #[cfg(not(target_os = "macos"))]
        if self.disk {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Disk idle requests are unavailable.",
            ));
        }
        #[cfg(windows)]
        let started = Instant::now();
        let power = Power::start(&self)?;
        #[cfg(unix)]
        let started = Instant::now();
        Ok(Inhibitor {
            request: self,
            power,
            started,
        })
    }

    /// Hold the request until the process exits or the timeout elapses.
    ///
    /// If the process has already exited, return without acquiring a request.
    pub fn wait_for_process(self, pid: NonZeroU32) -> io::Result<()> {
        if let Some(process) = Process::open(pid.get())? {
            self.acquire()?.hold(Target::Process(&process))?;
        }
        Ok(())
    }

    fn timeout(&self) -> Option<Duration> {
        self.timeout.filter(|duration| !duration.is_zero())
    }

    fn keep_idle(&self) -> bool {
        self.idle || !(self.display || self.system || self.user_active || self.disk)
    }

    fn activity_timeout(&self) -> Option<Duration> {
        self.user_active
            .then(|| self.timeout().unwrap_or(Duration::from_secs(5)))
    }
}

/// Releases the acquired power requests on drop.
#[must_use = "dropping the inhibitor releases its power requests"]
pub struct Inhibitor {
    request: Request,
    power: Power,
    started: Instant,
}

impl Inhibitor {
    /// Wait until the timeout or end of a user-activity-only request.
    ///
    /// An untimed sleep or display request waits indefinitely.
    pub fn wait(self) -> io::Result<()> {
        self.hold(Target::Idle)
    }

    /// Wait until the child exits or the request times out.
    ///
    /// The caller can retrieve the child's exit status with `child.wait()`.
    /// A timeout releases the requests and lets the child continue. Any pipe
    /// still attached as `child.stdin` stays open.
    pub fn wait_for_child(self, child: &mut Child) -> io::Result<()> {
        #[cfg(windows)]
        return self.hold(Target::Child(child));

        #[cfg(unix)]
        {
            if self.request.timeout().is_none()
                && !self.request.user_active
                && child.stdin.is_none()
            {
                return child.wait().map(|_| ());
            }
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            match Process::open(child.id()) {
                Ok(Some(process)) => self.hold(Target::Process(&process)),
                Ok(None) => Ok(()),
                #[cfg(target_os = "linux")]
                Err(error) if platform::pidfd_unavailable(&error) => {
                    self.hold(Target::Child(child))
                }
                Err(error) => Err(error),
            }
        }
    }

    fn hold(mut self, mut target: Target<'_>) -> io::Result<()> {
        let request = &self.request;
        let activity = request.activity_timeout();
        let activity_only = matches!(target, Target::Idle)
            && !(request.display || request.idle || request.system || request.disk);
        let timeout = request.timeout().or(activity.filter(|_| activity_only));
        loop {
            let elapsed = self.started.elapsed();
            let remaining = timeout.map(|limit| limit.saturating_sub(elapsed));
            if remaining == Some(Duration::ZERO) {
                return Ok(());
            }
            let active = activity
                .and_then(|limit| limit.checked_sub(elapsed))
                .filter(|duration| !duration.is_zero());
            platform::update(&mut self.power, request, active.is_some())?;
            if platform::wait(
                &self.power,
                &mut target,
                remaining.into_iter().chain(active).min(),
            )? {
                return Ok(());
            }
        }
    }
}

pub(crate) enum Target<'a> {
    Idle,
    Process(&'a Process),
    #[cfg(any(windows, target_os = "linux"))]
    Child(&'a mut Child),
}
