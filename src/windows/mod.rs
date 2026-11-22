mod power;

use self::power::{PowerRequests, PowerSource, reset_display_timer};
use crate::{Request, Target};
use std::{
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{ERROR_INVALID_PARAMETER, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{
        INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, Sleep, WaitForMultipleObjects,
        WaitForSingleObject,
    },
};

pub(crate) struct Process(OwnedHandle);

impl Process {
    pub(crate) fn open(pid: u32) -> io::Result<Option<Self>> {
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if !handle.is_null() {
            // SAFETY: OpenProcess returned an owned handle.
            return Ok(Some(Self(unsafe { OwnedHandle::from_raw_handle(handle) })));
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(code) if code == ERROR_INVALID_PARAMETER as i32 => Ok(None),
            _ => Err(error),
        }
    }
}

pub(crate) struct Power {
    source: Option<PowerSource>,
    requests: Option<PowerRequests>,
}

impl Power {
    pub(crate) fn start(options: &Request) -> io::Result<Self> {
        let mut power = Self {
            source: options.system.then(PowerSource::subscribe).transpose()?,
            requests: None,
        };
        update(&mut power, options, options.user_active)?;
        if options.user_active {
            reset_display_timer()?;
        }
        Ok(power)
    }
}

pub(crate) fn update(power: &mut Power, options: &Request, active: bool) -> io::Result<()> {
    let on_ac = power
        .source
        .as_ref()
        .map(PowerSource::on_ac)
        .transpose()?
        .unwrap_or(false);
    let display = options.display || active;
    if options.keep_idle() || display || on_ac {
        let requests = match &mut power.requests {
            Some(requests) => requests,
            slot => slot.insert(PowerRequests::new()?),
        };
        requests.set_display(display)?;
    } else {
        power.requests = None;
    }
    Ok(())
}

pub(crate) fn wait(
    power: &Power,
    target: &mut Target<'_>,
    duration: Option<Duration>,
) -> io::Result<bool> {
    let process = match target {
        Target::Idle => None,
        Target::Process(process) => Some(process.0.as_raw_handle()),
        Target::Child(child) => Some(child.as_raw_handle()),
    };
    let changed = power.source.as_ref().map(PowerSource::event);
    let changed = changed.as_ref().map(AsRawHandle::as_raw_handle);
    let milliseconds = duration.map_or(INFINITE, |value| {
        value
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u128::from(INFINITE - 1)) as u32
    });
    let result = unsafe {
        match (process, changed) {
            (Some(process), Some(event)) => {
                WaitForMultipleObjects(2, [process, event].as_ptr(), 0, milliseconds)
            }
            (Some(handle), None) | (None, Some(handle)) => {
                WaitForSingleObject(handle, milliseconds)
            }
            (None, None) => {
                Sleep(milliseconds);
                return Ok(false);
            }
        }
    };
    match result {
        WAIT_OBJECT_0 => Ok(process.is_some()),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        event if event == WAIT_OBJECT_0 + 1 && changed.is_some() => Ok(false),
        event => Err(io::Error::other(format!("Unexpected wait result {event}."))),
    }
}
