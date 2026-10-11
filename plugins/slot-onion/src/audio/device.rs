//! Onion's raw OSS path uses /dev/dsp and configures the Miyoo AO device.
//! Keep device initialization and blocking writes off the framebuffer loop.

use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::RawFd;
use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::app::SoundCue;

const CHANNELS: i32 = 2;
const AFMT_S16_LE: i32 = 0x10;
const SNDCTL_DSP_SPEED: libc::c_ulong = 0xc004_5002;
const SNDCTL_DSP_SETFMT: libc::c_ulong = 0xc004_5005;
const SNDCTL_DSP_CHANNELS: libc::c_ulong = 0xc004_5006;

#[cfg(target_arch = "arm")]
unsafe extern "C" {
    fn slot_onion_miyoo_audio_init(sample_rate: libc::c_int) -> libc::c_int;
}

#[cfg(target_os = "linux")]
fn open_raw(path: &CStr) -> io::Result<RawFd> {
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat,
            libc::AT_FDCWD,
            path.as_ptr(),
            libc::O_WRONLY | libc::O_CLOEXEC,
        ) as RawFd
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(fd)
    }
}

#[cfg(not(target_os = "linux"))]
fn open_raw(_path: &CStr) -> io::Result<RawFd> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "raw Miyoo audio is only available on Linux",
    ))
}

#[cfg(target_arch = "arm")]
fn configure_miyoo(sample_rate: i32) -> io::Result<()> {
    eprintln!("slot-onion: configuring Miyoo audio output");
    let result = unsafe { slot_onion_miyoo_audio_init(sample_rate) };
    if result != 0 {
        Err(io::Error::other(format!(
            "MI_AO_SetPubAttr failed with code {result}"
        )))
    } else {
        Ok(())
    }
}

#[cfg(not(target_arch = "arm"))]
fn configure_miyoo(_sample_rate: i32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "raw Miyoo audio is only available on ARM devices",
    ))
}

pub struct DeviceAudio {
    sender: Sender<SoundCue>,
}

impl DeviceAudio {
    pub fn open(rate: u32) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("slot-onion-audio".into())
            .spawn(move || {
                let first_cue = match receiver.recv() {
                    Ok(cue) => cue,
                    Err(_) => return,
                };
                eprintln!("slot-onion: initializing OSS audio on first sound");
                let mut playback = match Playback::open(rate) {
                    Ok(playback) => playback,
                    Err(error) => {
                        eprintln!("slot-onion: audio unavailable: {error}");
                        return;
                    }
                };
                if let Err(error) = playback.play(first_cue) {
                    eprintln!("slot-onion: audio playback: {error}");
                }
                while let Ok(cue) = receiver.recv() {
                    if let Err(error) = playback.play(cue) {
                        eprintln!("slot-onion: audio playback: {error}");
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        // Do not wait for OSS setup here. Keep any device-side initialization
        // off the frontend thread.
        Ok(Self { sender })
    }

    pub fn play(&self, cue: SoundCue) {
        let _ = self.sender.send(cue);
    }
}

struct Playback {
    fd: RawFd,
}

impl Playback {
    fn open(rate: u32) -> io::Result<Self> {
        let path = CString::new("/dev/dsp").expect("static path");
        // Onion's OSS driver calls glibc __open first so libpadsp cannot
        // intercept it. A raw syscall has the same effect and avoids the
        // audioserver open path, which hangs on this device.
        eprintln!("slot-onion: opening raw OSS device /dev/dsp");
        let fd = open_raw(&path)?;
        eprintln!("slot-onion: raw OSS open returned fd {fd}");
        let mut channels = CHANNELS;
        let mut format = AFMT_S16_LE;
        let mut sample_rate = i32::try_from(rate)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "sample rate is too large"))?;
        for (name, request, value) in [
            ("channels", SNDCTL_DSP_CHANNELS, &mut channels),
            ("format", SNDCTL_DSP_SETFMT, &mut format),
            ("sample rate", SNDCTL_DSP_SPEED, &mut sample_rate),
        ] {
            eprintln!("slot-onion: OSS configuring {name}");
            if unsafe { libc::ioctl(fd, request, value as *mut i32) } < 0 {
                let error = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(error);
            }
            eprintln!("slot-onion: OSS configured {name}");
        }
        if channels != CHANNELS || format != AFMT_S16_LE || sample_rate <= 0 {
            unsafe { libc::close(fd) };
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported OSS audio format: {channels}ch, {format:#x}, {sample_rate} Hz"
                ),
            ));
        }
        if let Err(error) = configure_miyoo(sample_rate) {
            unsafe { libc::close(fd) };
            return Err(error);
        }
        eprintln!("slot-onion: raw Miyoo OSS audio ready at {sample_rate} Hz");
        Ok(Self { fd })
    }

    fn play(&mut self, cue: SoundCue) -> io::Result<()> {
        let pcm = match cue {
            SoundCue::Insert => include_bytes!("../../assets/insert.pcm").as_slice(),
            SoundCue::Eject => include_bytes!("../../assets/eject.pcm").as_slice(),
        };
        let mut stereo = Vec::with_capacity(pcm.len() * 2);
        for pair in pcm.chunks_exact(2) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]).to_le_bytes();
            stereo.extend_from_slice(&sample);
            stereo.extend_from_slice(&sample);
        }

        let mut written = 0;
        while written < stereo.len() {
            let count = unsafe {
                libc::write(
                    self.fd,
                    stereo[written..].as_ptr().cast(),
                    stereo.len() - written,
                )
            };
            if count > 0 {
                written += count as usize;
            } else if count < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            } else {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}
