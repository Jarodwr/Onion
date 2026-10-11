/// Port of Onion's `FNV1A_Pippip_Yurii` (src/common/utils/hash.h), which names
/// romScreens: `romScreens/<hash>.png` where the hash is over the recents rompath.
pub fn fnv1a_pippip_yurii(s: &[u8]) -> u32 {
    const PRIME: u64 = 591_798_841;
    // The C version reads 8 bytes at a time and may run past the end.
    let mut buf = s.to_vec();
    buf.extend_from_slice(&[0; 8]);
    let read = |at: usize| u64::from_le_bytes(buf[at..at + 8].try_into().unwrap());

    let len = s.len();
    let mut hash64: u64 = 14_695_981_039_346_656_037;
    if len > 8 {
        let cycles = ((len - 1) >> 4) + 1;
        let nd_head = len - (cycles << 3);
        for at in (0..cycles).map(|c| c * 8) {
            hash64 = (hash64 ^ read(at)).wrapping_mul(PRIME);
            hash64 = (hash64 ^ read(at + nd_head)).wrapping_mul(PRIME);
        }
    } else {
        let shift = ((8 - len) << 3) as u32;
        let word = read(0).checked_shl(shift).map_or(0, |w| w >> shift);
        hash64 = (hash64 ^ word).wrapping_mul(PRIME);
    }
    let hash32 = (hash64 ^ (hash64 >> 32)) as u32;
    hash32 ^ (hash32 >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values from compiling Onion's hash.h with clang.
    #[test]
    fn matches_onion() {
        for (input, want) in REFERENCE {
            assert_eq!(fnv1a_pippip_yurii(input.as_bytes()), *want, "{input}");
        }
    }

    const REFERENCE: &[(&str, u32)] = &[
        ("a", 579392569),
        ("abcdefgh", 622653775),
        ("abcdefghi", 2300094295),
        ("/mnt/SDCARD/Roms/GBA/x.gba", 1430017418),
        (
            "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Pokemon - Emerald Version (USA, Europe).gba",
            2349991472,
        ),
        (
            "/mnt/SDCARD/Emu/GB/../../Roms/GB/Tetris (World) (Rev 1).gb",
            1116341086,
        ),
        ("0123456789abcdef", 2519323961),
        ("0123456789abcdefg", 3638872414),
    ];
}
