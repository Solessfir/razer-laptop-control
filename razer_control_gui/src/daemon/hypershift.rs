// Forwards the Fn (HyperShift) key to a virtual keyboard as F24, so a compositor or remapper can use it as a layer key.
// The firmware reports Fn only as vendor report 4 on the keyboard interface, which the kernel HID driver ignores.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context};
use log::*;

const KEYBOARD_INTERFACE: i32 = 1;
const FN_REPORT_ID: u8 = 0x04;
const FN_DOWN: u8 = 0x0a;
const FN_UP: u8 = 0x00;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const KEY_F24: u16 = 194;

// ioctl numbers from linux/uinput.h
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_SETUP: u64 = 0x405c5503;
const UI_SET_EVBIT: u64 = 0x40045564;
const UI_SET_KEYBIT: u64 = 0x40045565;

#[repr(C)]
struct UinputSetup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

struct VirtualKeyboard {
    file: File,
}

impl VirtualKeyboard {
    fn new(product_id: u16) -> anyhow::Result<Self> {
        let file = OpenOptions::new().write(true).open("/dev/uinput")
            .context("failed to open /dev/uinput")?;

        let mut setup = UinputSetup {
            bustype: 0x06, // BUS_VIRTUAL
            vendor: 0x1532,
            product: product_id,
            version: 1,
            name: [0; 80],
            ff_effects_max: 0,
        };
        let name = b"Razer HyperShift";
        setup.name[..name.len()].copy_from_slice(name);

        let fd = file.as_raw_fd();
        // SAFETY: valid uinput fd and argument types matching each request.
        let ok = unsafe {
            libc::ioctl(fd, UI_SET_EVBIT as _, EV_KEY as libc::c_int) == 0
                && libc::ioctl(fd, UI_SET_KEYBIT as _, KEY_F24 as libc::c_int) == 0
                && libc::ioctl(fd, UI_DEV_SETUP as _, &setup) == 0
                && libc::ioctl(fd, UI_DEV_CREATE as _) == 0
        };
        if !ok {
            bail!("failed to create uinput device: {}", std::io::Error::last_os_error());
        }

        Ok(VirtualKeyboard { file })
    }

    fn emit(&mut self, kind: u16, code: u16, value: i32) -> std::io::Result<()> {
        // SAFETY: input_event is plain data; the kernel fills in the timestamp.
        let mut event: libc::input_event = unsafe { std::mem::zeroed() };
        event.type_ = kind;
        event.code = code;
        event.value = value;
        // SAFETY: reads the bytes of a fully initialized repr(C) struct.
        let bytes = unsafe {
            std::slice::from_raw_parts(&event as *const _ as *const u8, std::mem::size_of::<libc::input_event>())
        };
        self.file.write_all(bytes)
    }

    fn set_fn(&mut self, pressed: bool) -> std::io::Result<()> {
        self.emit(EV_KEY, KEY_F24, pressed as i32)?;
        self.emit(EV_SYN, 0, 0)
    }
}

pub fn start(product_id: u16) {
    thread::spawn(move || {
        let mut keyboard = match VirtualKeyboard::new(product_id) {
            Ok(keyboard) => keyboard,
            Err(e) => {
                error!("HyperShift disabled: {e:#}");
                return;
            }
        };
        info!("HyperShift: forwarding Fn as F24");

        loop {
            if let Err(e) = forward_fn(product_id, &mut keyboard) {
                warn!("HyperShift: {e:#}");
            }
            // The interface disappears across suspend or USB resets, so don't leave the layer stuck.
            let _ = keyboard.set_fn(false);
            thread::sleep(Duration::from_secs(1));
        }
    });
}

fn forward_fn(product_id: u16, keyboard: &mut VirtualKeyboard) -> anyhow::Result<()> {
    let path = razer_laptop::interface_path(product_id, KEYBOARD_INTERFACE)?
        .context("keyboard interface not found")?;
    let path = OsStr::from_bytes(path.as_bytes());
    let mut hidraw = File::open(path).with_context(|| format!("failed to open {path:?}"))?;

    let mut report = [0u8; 64];
    loop {
        let size = hidraw.read(&mut report).context("failed to read keyboard report")?;
        if size < 2 || report[0] != FN_REPORT_ID {
            continue;
        }
        match report[1] {
            FN_DOWN => keyboard.set_fn(true)?,
            FN_UP => keyboard.set_fn(false)?,
            value => info!("HyperShift: unknown report 4 value {value:#04x}"),
        }
    }
}
