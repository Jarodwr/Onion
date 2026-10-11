use std::path::PathBuf;

pub struct Args {
    pub sdcard: PathBuf,
    pub tmp: PathBuf,
    pub panel: (u32, u32),
    /// Window size relative to the panel, used by the desktop frontend.
    pub zoom: f32,
    pub rom: Option<String>,
}

impl Args {
    pub fn parse() -> Result<Self, String> {
        let mut args = Args {
            sdcard: PathBuf::from("/mnt/SDCARD"),
            tmp: PathBuf::from("/tmp"),
            panel: (640, 480),
            zoom: 1.0,
            rom: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(flag) = it.next() {
            let mut value = || it.next().ok_or(format!("{flag} needs a value"));
            match flag.as_str() {
                "--sdcard" => args.sdcard = value()?.into(),
                "--tmp" => args.tmp = value()?.into(),
                "--rom" => args.rom = Some(value()?),
                "--panel" => {
                    let v = value()?;
                    let (w, h) = v.split_once('x').ok_or("--panel wants WxH")?;
                    args.panel = (
                        w.parse().map_err(|_| "bad --panel width")?,
                        h.parse().map_err(|_| "bad --panel height")?,
                    );
                }
                "--zoom" => {
                    args.zoom = value()?
                        .parse()
                        .ok()
                        .filter(|z: &f32| *z > 0.0)
                        .ok_or("--zoom wants a positive number, e.g. 0.5, 1 or 2")?;
                }
                _ => return Err(format!("unknown argument {flag}")),
            }
        }
        Ok(args)
    }
}
