use crate::convert::{self, Layout, Outcome};
use crate::convert200::{gross_beat_state_8, plural};
use crate::flp::{self, op, Event, Flp, Payload};
use crate::wrapper;

/* plugins that the user's version table dates after the target. matched case-insensitively
   against the internal name. a dropped effect leaves an empty slot; a generator stays and the
   target reports it missing. */
const MISSING_IN_FL24: [&str; 6] = [
    "Emphasis",
    "Emphasizer",
    "FL Studio Mobile",
    "FL Studio Mobile Rack",
    "Fruity Slicer 2",
    "Transmitter",
];

const MISSING_IN_FL21: [&str; 10] = [
    "Emphasis",
    "Emphasizer",
    "FL Studio Mobile",
    "FL Studio Mobile Rack",
    "Fruity Slicer 2",
    "Transmitter",
    "Kepler Exo",
    "Low Lifter",
    "Spreader",
    "Transporter",
];

/* native states whose leading u32 and length are the same in every corpus version that
   holds them, with at least two versions per plugin. they pass through without a warning. */
const NATIVE_STABLE: [&str; 25] = [
    "EQUO",
    "Edison",
    "FL Keys",
    "Fruity Blood Overdrive",
    "Fruity Chorus",
    "Fruity Convolver",
    "Fruity Delay 2",
    "Fruity Delay 3",
    "Fruity Delay Bank",
    "Fruity Fast Dist",
    "Fruity Flanger",
    "Fruity Flangus",
    "Fruity Limiter",
    "Fruity Love Philter",
    "Fruity Multiband Compressor",
    "Fruity Mute 2",
    "Fruity Phaser",
    "Fruity Reeverb",
    "Fruity Reeverb 2",
    "Fruity Soft Clipper",
    "Fruity Stereo Shaper",
    "Low Lifter",
    "Maximus",
    "Soundgoodizer",
    "Fruity Parametric EQ",
];

const WRAPPER_OPTIONS_CHUNK: u32 = 2;
const WRAPPER_OPTIONS_FLAG_OFFSET: usize = 12;

/* Fruity Compressor writes state 2 at 32 bytes in 21 and 24, and state 3 at 36 bytes in 25
   and 26. the 36-byte body is the 32-byte body plus one u32, zero in all 74 corpus samples. */
const COMPRESSOR_STATE_NEWER: u32 = 3;
const COMPRESSOR_STATE_OLDER: u32 = 2;
const COMPRESSOR_LEN_OLDER: usize = 32;
const COMPRESSOR_LEN_NEWER: usize = 36;

/* Parametric EQ 2 writes state 7 in 21 and state 8 in 24 to 26, both 354 bytes. 21 writes
   only 0 or 1 at offset 292; 24 to 26 also write 3. FL 21.2.3 loads and saves a 3 unchanged,
   but crashes in the plugin when that EQ processes audio. Setting it to 1 stops the crash. */
const EQ2_STATE_FL21: u32 = 7;
const EQ2_STATE_NEWER: u32 = 8;
const EQ2_LEN: usize = 354;
const EQ2_MODE_OFFSET: usize = 292;
const EQ2_MODES_FL21: [u32; 2] = [0, 1];
const EQ2_MODE_FL21_DEFAULT: u32 = 1;

struct Profile {
    layout: &'static Layout,
    target: &'static str,
    missing: &'static [&'static str],
    wrapper_flag_mask: u8,
    eq2_to_7: bool,
}

const PROFILE_FL24: Profile = Profile {
    layout: &convert::FL24,
    target: "24.2",
    missing: &MISSING_IN_FL24,
    wrapper_flag_mask: 0x01,
    eq2_to_7: false,
};

const PROFILE_FL21: Profile = Profile {
    layout: &convert::FL21,
    target: "21.2",
    missing: &MISSING_IN_FL21,
    wrapper_flag_mask: 0x11,
    eq2_to_7: true,
};

#[derive(Default)]
struct Counts {
    option_flags: usize,
    compressors: usize,
    compressor_tail_nonzero: usize,
    gross_beats: usize,
    eq2: usize,
    eq2_mode: usize,
    flex: usize,
    unverified: Vec<String>,
    effects_dropped: Vec<String>,
    generators_kept: Vec<String>,
}

/* chunk 2 byte 12 is a flag byte. 21 writes {0x20, 0x24, 0xA4}, 24 adds 0xB4, and 25/26 add
   0xA0, 0xA5 and 0xB5. the bits that the target never writes are cleared; the rest stays. */
fn wrapper_state(b: &[u8], mask: u8, c: &mut Counts) -> Option<Vec<u8>> {
    let (version, mut chunks) = wrapper::parse_state(b)?;
    let mut changed = false;
    for chunk in &mut chunks {
        if chunk.id == WRAPPER_OPTIONS_CHUNK && chunk.data.len() > WRAPPER_OPTIONS_FLAG_OFFSET {
            let flag = &mut chunk.data[WRAPPER_OPTIONS_FLAG_OFFSET];
            if *flag & mask != 0 {
                *flag &= !mask;
                changed = true;
            }
        }
    }
    if !changed {
        return None;
    }
    c.option_flags += 1;
    Some(wrapper::build_state(version, &chunks))
}

enum State {
    Rewritten(Vec<u8>),
    Unchanged,
    Unverified,
}

fn native_state(p: &Profile, name: &str, b: &[u8], c: &mut Counts) -> State {
    let marker = b.get(0..4).map(|h| u32::from_le_bytes(h.try_into().unwrap()));
    match name {
        "Fruity Compressor" => match marker {
            Some(COMPRESSOR_STATE_NEWER) if b.len() == COMPRESSOR_LEN_NEWER => {
                if b[COMPRESSOR_LEN_OLDER..].iter().any(|&x| x != 0) {
                    c.compressor_tail_nonzero += 1;
                }
                let mut nb = b[..COMPRESSOR_LEN_OLDER].to_vec();
                nb[0..4].copy_from_slice(&COMPRESSOR_STATE_OLDER.to_le_bytes());
                c.compressors += 1;
                State::Rewritten(nb)
            }
            Some(COMPRESSOR_STATE_OLDER) if b.len() == COMPRESSOR_LEN_OLDER => State::Unchanged,
            _ => State::Unverified,
        },
        "Gross Beat" => match gross_beat_state_8(b) {
            Some(nb) => {
                c.gross_beats += 1;
                State::Rewritten(nb)
            }
            None if b.get(2..4) == Some(&[8u8, 0][..]) => State::Unchanged,
            None => State::Unverified,
        },
        "Fruity Parametric EQ 2" => match marker {
            Some(EQ2_STATE_NEWER) if b.len() == EQ2_LEN && p.eq2_to_7 => {
                let mut nb = b.to_vec();
                nb[0..4].copy_from_slice(&EQ2_STATE_FL21.to_le_bytes());
                let mode = &mut nb[EQ2_MODE_OFFSET..EQ2_MODE_OFFSET + 4];
                if !EQ2_MODES_FL21.contains(&u32::from_le_bytes((&*mode).try_into().unwrap())) {
                    mode.copy_from_slice(&EQ2_MODE_FL21_DEFAULT.to_le_bytes());
                    c.eq2_mode += 1;
                }
                c.eq2 += 1;
                State::Rewritten(nb)
            }
            Some(EQ2_STATE_NEWER) if b.len() == EQ2_LEN => State::Unchanged,
            Some(EQ2_STATE_FL21) if b.len() == EQ2_LEN => State::Unchanged,
            _ => State::Unverified,
        },
        "FLEX" => {
            c.flex += 1;
            State::Unverified
        }
        _ if NATIVE_STABLE.contains(&name) => State::Unchanged,
        _ => State::Unverified,
    }
}

fn post_pass(p: &Profile, src: &Flp, notes: &mut Vec<String>, warnings: &mut Vec<String>) -> Flp {
    let mut c = Counts::default();
    let mut out: Vec<Event> = Vec::with_capacity(src.events.len());
    let mut current_plugin = String::new();
    let mut in_mixer = false;
    let mut skip_slot = false;

    for ev in &src.events {
        if ev.op == op::INSERT_FLAGS {
            in_mixer = true;
        }
        if skip_slot {
            if ev.op == op::WRAPPER {
                skip_slot = false;
            }
            continue;
        }
        match ev.op {
            op::PLUGIN_INTERNAL_NAME => {
                current_plugin = ev.blob().map(flp::utf16z).unwrap_or_default();
                let missing = p.missing.iter().any(|m| m.eq_ignore_ascii_case(&current_plugin));
                if missing && in_mixer {
                    c.effects_dropped.push(current_plugin.clone());
                    skip_slot = true;
                    continue;
                }
                if missing {
                    c.generators_kept.push(current_plugin.clone());
                }
                out.push(ev.clone());
            }
            op::WRAPPER => {
                let b = ev.blob().unwrap_or_default();
                if current_plugin == "Fruity Wrapper" {
                    match wrapper_state(b, p.wrapper_flag_mask, &mut c) {
                        Some(nb) => out.push(Event { op: op::WRAPPER, payload: Payload::Blob(nb) }),
                        None => out.push(ev.clone()),
                    }
                    continue;
                }
                if current_plugin.is_empty() {
                    out.push(ev.clone());
                    continue;
                }
                match native_state(p, &current_plugin, b, &mut c) {
                    State::Rewritten(nb) => {
                        out.push(Event { op: op::WRAPPER, payload: Payload::Blob(nb) })
                    }
                    State::Unchanged => out.push(ev.clone()),
                    State::Unverified => {
                        if !c.unverified.contains(&current_plugin) {
                            c.unverified.push(current_plugin.clone());
                        }
                        out.push(ev.clone());
                    }
                }
            }
            _ => out.push(ev.clone()),
        }
    }

    let target = p.target;
    if c.option_flags > 0 {
        notes.push(format!(
            "wrapper chunk {WRAPPER_OPTIONS_CHUNK} flag bits 0x{:02X} cleared on {} plugin{}",
            p.wrapper_flag_mask,
            c.option_flags,
            plural(c.option_flags)
        ));
    }
    if c.compressors > 0 {
        notes.push(format!(
            "Fruity Compressor state {COMPRESSOR_STATE_NEWER} -> {COMPRESSOR_STATE_OLDER} ({COMPRESSOR_LEN_OLDER} bytes) on {} plugin{}",
            c.compressors,
            plural(c.compressors)
        ));
    }
    if c.compressor_tail_nonzero > 0 {
        warnings.push(format!(
            "{} Fruity Compressor state{} carried a nonzero trailing value that {target} has no field for",
            c.compressor_tail_nonzero,
            plural(c.compressor_tail_nonzero)
        ));
    }
    if c.gross_beats > 0 {
        notes.push(format!(
            "Gross Beat state 9 -> 8 (trailing byte dropped) on {} plugin{}",
            c.gross_beats,
            plural(c.gross_beats)
        ));
    }
    if c.eq2 > 0 {
        notes.push(format!(
            "Fruity Parametric EQ 2 state {EQ2_STATE_NEWER} -> {EQ2_STATE_FL21} on {} plugin{}",
            c.eq2,
            plural(c.eq2)
        ));
    }
    if c.eq2_mode > 0 {
        warnings.push(format!(
            "{} Fruity Parametric EQ 2 plugin{} used a setting that {target} does not have (state offset {EQ2_MODE_OFFSET}); set to the {target} value {EQ2_MODE_FL21_DEFAULT}",
            c.eq2_mode,
            plural(c.eq2_mode)
        ));
    }
    if c.flex > 0 {
        warnings.push(format!(
            "{} FLEX state{} from the FL 26 rework carried unchanged; {target} can reset {} to the default preset",
            c.flex,
            plural(c.flex),
            if c.flex == 1 { "it" } else { "them" }
        ));
    }
    if !c.effects_dropped.is_empty() {
        warnings.push(format!(
            "{} mixer effect{} dropped, the plugin did not exist in {target}: {}",
            c.effects_dropped.len(),
            plural(c.effects_dropped.len()),
            c.effects_dropped.join(", ")
        ));
    }
    if !c.generators_kept.is_empty() {
        warnings.push(format!(
            "{} generator{} kept although the plugin did not exist in {target}, {target} will report it missing: {}",
            c.generators_kept.len(),
            plural(c.generators_kept.len()),
            c.generators_kept.join(", ")
        ));
    }
    let unverified: Vec<&String> = c.unverified.iter().filter(|n| n.as_str() != "FLEX").collect();
    if !unverified.is_empty() {
        warnings.push(format!(
            "plugin state carried unchanged, unverified in FL {target}: {}",
            unverified.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }

    Flp {
        format: src.format,
        n_channels: src.n_channels,
        ppq: src.ppq,
        header_raw: src.header_raw.clone(),
        events: out,
        trailing: src.trailing.clone(),
    }
}

fn convert(src: &Flp, p: &Profile) -> Result<Outcome, String> {
    let base = convert::retarget(src, p.layout)?;
    let mut notes = base.notes;
    let mut warnings = base.warnings;
    let flp = post_pass(p, &base.flp, &mut notes, &mut warnings);
    Ok(Outcome { flp, notes, warnings })
}

pub fn to_fl24(src: &Flp) -> Result<Outcome, String> {
    convert(src, &PROFILE_FL24)
}

pub fn to_fl21(src: &Flp) -> Result<Outcome, String> {
    convert(src, &PROFILE_FL21)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapper_blob(flag: u8) -> Vec<u8> {
        let mut options = vec![0u8; 25];
        options[WRAPPER_OPTIONS_FLAG_OFFSET] = flag;
        wrapper::build_state(
            12,
            &[
                wrapper::Chunk { id: WRAPPER_OPTIONS_CHUNK, data: options },
                wrapper::Chunk { id: 57, data: vec![0x60, 0x09, 0, 0] },
            ],
        )
    }

    #[test]
    fn wrapper_flags_keep_target_bits() {
        let mut c = Counts::default();
        let nb = wrapper_state(&wrapper_blob(0xB5), 0x01, &mut c).unwrap();
        let (version, chunks) = wrapper::parse_state(&nb).unwrap();
        assert_eq!(version, 12);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].data[WRAPPER_OPTIONS_FLAG_OFFSET], 0xB4);
        let nb = wrapper_state(&wrapper_blob(0xB5), 0x11, &mut c).unwrap();
        let (_, chunks) = wrapper::parse_state(&nb).unwrap();
        assert_eq!(chunks[0].data[WRAPPER_OPTIONS_FLAG_OFFSET], 0xA4);
        assert!(wrapper_state(&wrapper_blob(0xA4), 0x11, &mut c).is_none());
        assert_eq!(c.option_flags, 2);
    }

    #[test]
    fn compressor_state_3_drops_trailing_u32() {
        let mut b = vec![0u8; COMPRESSOR_LEN_NEWER];
        b[0..4].copy_from_slice(&3u32.to_le_bytes());
        b[4] = 0x55;
        let mut c = Counts::default();
        let State::Rewritten(nb) = native_state(&PROFILE_FL24, "Fruity Compressor", &b, &mut c)
        else {
            panic!("compressor state not rewritten");
        };
        assert_eq!(nb.len(), COMPRESSOR_LEN_OLDER);
        assert_eq!(&nb[0..4], &2u32.to_le_bytes());
        assert_eq!(nb[4], 0x55);
        assert_eq!(c.compressor_tail_nonzero, 0);
    }

    #[test]
    fn eq2_version_rewrite_only_for_fl21() {
        let mut b = vec![0u8; EQ2_LEN];
        b[0..4].copy_from_slice(&8u32.to_le_bytes());
        b[292] = 3;
        let mut c = Counts::default();
        assert!(matches!(
            native_state(&PROFILE_FL24, "Fruity Parametric EQ 2", &b, &mut c),
            State::Unchanged
        ));
        let State::Rewritten(nb) = native_state(&PROFILE_FL21, "Fruity Parametric EQ 2", &b, &mut c)
        else {
            panic!("eq2 state not rewritten");
        };
        assert_eq!(nb.len(), EQ2_LEN);
        assert_eq!(&nb[0..4], &7u32.to_le_bytes());
        assert_eq!(&nb[292..296], &1u32.to_le_bytes());
        assert_eq!(&nb[4..292], &b[4..292]);
        assert_eq!(&nb[296..], &b[296..]);
        assert_eq!(c.eq2, 1);
        assert_eq!(c.eq2_mode, 1);

        b[292] = 0;
        let State::Rewritten(nb) = native_state(&PROFILE_FL21, "Fruity Parametric EQ 2", &b, &mut c)
        else {
            panic!("eq2 state not rewritten");
        };
        assert_eq!(&nb[4..], &b[4..]);
        assert_eq!(c.eq2_mode, 1);
    }
}
