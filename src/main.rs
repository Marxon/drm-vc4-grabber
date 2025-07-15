#[macro_use]
extern crate nix;

use std::fs::{File, OpenOptions};
use std::net::TcpStream;
use std::os::fd::{AsRawFd, AsFd};

use clap::{App, Arg};
use drm::control::{Device as ControlDevice, framebuffer::Handle};
use drm::Device;
use drm_ffi::drm_set_client_cap;

use dump_image::dump_framebuffer_to_image;
use image::{ImageError, RgbImage};

use std::{thread, time::Duration};

use std::io::Result as StdResult;

pub mod ffi;
pub mod framebuffer;
pub mod hyperion;
pub mod hyperion_reply_generated;
pub mod hyperion_request_generated;
pub mod image_decoder;
pub mod dump_image;

pub use hyperion_request_generated::hyperionnet::{Clear, Color, Command, Image, Register};

use hyperion::{read_reply, register_direct, send_color_red, send_image};

pub struct Card(File);

impl AsRawFd for Card {
    fn as_raw_fd(&self) -> std::os::unix::io::RawFd {
        self.0.as_raw_fd()
    }
}

impl AsFd for Card {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Device for Card {}
impl ControlDevice for Card {}

impl Card {
    fn open(path: &str) -> Self {
        let mut options = OpenOptions::new();
        options.read(true).write(false);
        Card(options.open(path).unwrap())
    }
}

fn save_screenshot(img: &RgbImage) -> Result<(), ImageError> {
    img.save("screenshot.png")
}

fn send_dumped_image(socket: &mut TcpStream, img: &RgbImage, verbose: bool) -> StdResult<()> {
    register_direct(socket)?;
    read_reply(socket, verbose)?;
    send_image(socket, img, verbose)
}

fn dump_and_send_framebuffer(
    socket: &mut TcpStream,
    card: &Card,
    fb: Handle,
    verbose: bool,
) -> StdResult<()> {
    if let Ok(img) = dump_framebuffer_to_image(card, fb, verbose) {
        send_dumped_image(socket, &img, verbose)
    } else {
        println!("Error dumping framebuffer to image.");
        Ok(())
    }
}

fn find_framebuffer(card: &Card, verbose: bool) -> Option<Handle> {
    let resource_handles = card.resource_handles().ok()?;

    for crtc in resource_handles.crtcs() {
        let info = card.get_crtc(*crtc).ok()?;

        if verbose {
            println!("CRTC Info: {:?}", info);
        }

        if info.mode().is_some() {
            return info.framebuffer();
        }
    }

    let plane_handles = card.plane_handles().ok()?;

    for plane in plane_handles.planes() {
        let info = card.get_plane(*plane).ok()?;

        if verbose {
            println!("Plane Info: {:?}", info);
        }

        if info.crtc().is_some() {
            return info.framebuffer();
        }
    }

    None
}
fn main() {
    let matches = App::new("DRM VC4 Screen Grabber for Hyperion")
        .version("0.1.mrx")
        .author("edit by Marxon")
        .about("Captures a screenshot and sends it to the Hyperion server.")
        .arg(
            Arg::with_name("device")
                .short("d")
                .long("device")
                .default_value("/dev/dri/card0")
                .takes_value(true)
                .help("The device path of the DRM device to capture the image from."),
        )
        .arg(
            Arg::with_name("address")
                .short("a")
                .long("address")
                .default_value("127.0.0.1:19400")
                .takes_value(true)
                .help("The Hyperion TCP socket address to send the captured screenshots to."),
        )
        .arg(
            Arg::with_name("screenshot")
                .long("screenshot")
                .takes_value(false)
                .help("Capture a screenshot and save it to screenshot.png"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Print verbose debugging information."),
        )
        .get_matches();

    let verbose = matches.is_present("verbose");
    let screenshot = matches.is_present("screenshot");
    let device_path = matches.value_of("device").unwrap();
    let card = Card::open(device_path);
    let authenticated = card.authenticated().unwrap();

    if verbose {
        let driver = card.get_driver().unwrap();
        println!("Driver (auth={}): {:?}", authenticated, driver);
    }

    unsafe {
        let set_cap = drm_set_client_cap {
            capability: drm_ffi::DRM_CLIENT_CAP_UNIVERSAL_PLANES as u64,
            value: 1,
        };
        drm_ffi::ioctl::set_cap(card.as_raw_fd(), &set_cap).unwrap();
    }

    let address = matches.value_of("address").unwrap();
    if screenshot {
        if let Some(fb) = find_framebuffer(&card, verbose) {
            let img = dump_framebuffer_to_image(&card, fb, verbose).unwrap();
            save_screenshot(&img).unwrap();
        } else {
            println!("No framebuffer found!");
        }
    } else {
        let mut socket = TcpStream::connect(address).unwrap();
        register_direct(&mut socket).unwrap();
        read_reply(&mut socket, verbose).unwrap();

        send_color_red(&mut socket, verbose).unwrap();
        thread::sleep(Duration::from_secs(1));

        loop {
            if let Some(fb) = find_framebuffer(&card, verbose) {
                dump_and_send_framebuffer(&mut socket, &card, fb, verbose).unwrap();
                thread::sleep(Duration::from_millis(1000 / 20));
            } else {
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}
