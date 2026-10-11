//! Desktop playback for slot's cartridge insertion and ejection sounds.

#[cfg(feature = "host")]
use slot::audio::{AudioSink, Sfx};

use crate::app::SoundCue;

#[cfg(feature = "device")]
mod device;

const SAMPLE_RATE: u32 = 48_000;

pub struct Audio {
    #[cfg(feature = "host")]
    sink: slot::audio::HostAudio,
    #[cfg(feature = "device")]
    sink: device::DeviceAudio,
}

impl Audio {
    pub fn open() -> Result<Self, String> {
        #[cfg(feature = "host")]
        {
            let mut sink = HostAudio::new();
            sink.open(SAMPLE_RATE).map_err(|e| e.to_string())?;
            return Ok(Audio { sink });
        }
        #[cfg(feature = "device")]
        {
            let sink = device::DeviceAudio::open(SAMPLE_RATE)?;
            Ok(Audio { sink })
        }
    }

    pub fn play(&self, cue: SoundCue) {
        #[cfg(feature = "host")]
        {
            let ring = self.sink.ring();
            let sfx = match cue {
                SoundCue::Insert => Sfx::Insert,
                SoundCue::Eject => Sfx::Eject,
            };
            ring.mix(&sfx.render(ring.sample_rate()));
        }
        #[cfg(feature = "device")]
        self.sink.play(cue);
    }
}

#[cfg(feature = "host")]
use slot::audio::HostAudio;
