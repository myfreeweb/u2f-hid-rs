/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use devd_rs;

use std::ffi::OsString;
use std::{io, fs};
use std::sync::mpsc::{channel, Receiver, TryIter};

use runloop::RunLoop;
use util::to_io_err;

pub enum Event {
    Add(OsString),
    Remove(OsString),
}

impl Event {
    fn from_devd(event: devd_rs::Event) -> Option<Self> {
        match event {
            devd_rs::Event::Attach { ref dev, parent: _, location: _ }
                if dev.starts_with("uhid") => Some(Event::Add(("/dev/".to_owned() + dev).into())),
            devd_rs::Event::Detach { ref dev, parent: _, location: _ }
                if dev.starts_with("uhid") => Some(Event::Remove(("/dev/".to_owned() + dev).into())),
            _ => None,
        }
    }
}

pub struct Monitor {
    // Receive events from the thread.
    rx: Receiver<Event>,
    // Handle to the thread loop.
    thread: RunLoop,
}

impl Monitor {
    pub fn new() -> io::Result<Self> {
        let (tx, rx) = channel();

        let thread = RunLoop::new(move |alive| -> io::Result<()> {
            let mut ctx = devd_rs::Context::new().map_err(|e| { let e2 : io::Error = e.into(); e2 })?;

            // Iterate all existing devices.
            for dev in fs::read_dir("/dev")? {
                println!("DEV {:?}", dev);
                if let Ok(dev) = dev {
                    let filename_ = dev.file_name();
                    let filename = filename_.to_str().unwrap_or("");
                    if filename.starts_with("uhid") {
                        tx.send(Event::Add(("/dev/".to_owned() + filename).into())).map_err(to_io_err)?;
                    }
                }
            }

            // Loop until we're stopped by the controlling thread, or fail.
            while alive() {
                let devd_event = ctx.wait_for_event();
                if let Ok(Some(event)) = devd_event.and_then(|e| Ok(Event::from_devd(e))) {
                    tx.send(event).map_err(to_io_err)?;
                }
            }

            Ok(())
        })?;

        Ok(Self { rx, thread })
    }

    pub fn events<'a>(&'a self) -> TryIter<'a, Event> {
        self.rx.try_iter()
    }

    pub fn alive(&self) -> bool {
        self.thread.alive()
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.thread.cancel();
    }
}
