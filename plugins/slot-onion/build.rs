use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/audio/miyoo_audio.c");
    let target = env::var("TARGET").unwrap_or_default();
    if target != "armv7-unknown-linux-gnueabihf" {
        return;
    }

    let compiler = env::var("CC_armv7_unknown_linux_gnueabihf")
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "arm-linux-gnueabihf-gcc".into());
    let sysroot = Command::new(&compiler)
        .arg("--print-sysroot")
        .output()
        .expect("run ARM compiler to locate Miyoo sysroot");
    assert!(sysroot.status.success(), "ARM compiler could not report sysroot");
    let sysroot = String::from_utf8(sysroot.stdout)
        .expect("ARM sysroot path must be UTF-8")
        .trim()
        .to_owned();
    let sysroot = PathBuf::from(sysroot);

    cc::Build::new()
        .target(&target)
        .compiler(&compiler)
        .include(sysroot.join("usr/include"))
        .file("src/audio/miyoo_audio.c")
        .compile("slot_onion_miyoo_audio");

    println!("cargo:rustc-link-search=native={}", sysroot.join("usr/lib").display());
    // This mirrors Onion's Miyoo RetroArch link set for MI_AO_SetPubAttr.
    println!("cargo:rustc-link-lib=mi_ao");
    // Keep dependencies after mi_ao: rustc links with --as-needed, so listing
    // mi_sys first can discard it before mi_ao's references are seen.
    println!("cargo:rustc-link-lib=mi_sys");
    println!("cargo:rustc-link-lib=mi_gfx");
    println!("cargo:rustc-link-lib=mi_common");
}
