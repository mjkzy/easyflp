use crate::convert::{self, Outcome, FL20_KNOWN_OPS};
use crate::flp::{self, op, Event, Flp, Payload};
use crate::wrapper;

pub const FL200_VERSION: &str = "20.0.5.681";
pub const FL200_BUILD: u32 = 681;

/* 20.0.5 writes a 12-byte registration blob. the value is copied from the truth save. */
const FL200_REGISTRATION: [u8; 12] = [
    0x3C, 0x00, 0x37, 0x00, 0x38, 0x00, 0x31, 0x00, 0x44, 0x00, 0x00, 0x00,
];

const POST_FL200_OPS: [u8; 6] = [0x26, 0x27, 0x28, 0x9D, 0x9E, 0xA4];

const FL200_D7_LEN: usize = 157;

/* the plugin folders of a 20.0.5 install, Plugins\Fruity\Effects and \Generators. the
   internal name of a plugin matches its folder name case-insensitively. a native effect outside
   the list did not ship with 20.0.5; 20.0.5 reports it missing and stops soon after. */
const FL200_EFFECTS: [&str; 68] = [
    "Control Surface",
    "EQUO",
    "Edison",
    "Effector",
    "Fruity 7 Band EQ",
    "Fruity Balance",
    "Fruity Bass Boost",
    "Fruity Big Clock",
    "Fruity Center",
    "Fruity Chorus",
    "Fruity Compressor",
    "Fruity Convolver",
    "Fruity Delay",
    "Fruity Delay 2",
    "Fruity Delay 3",
    "Fruity Delay Bank",
    "Fruity Fast Dist",
    "Fruity Fast LP",
    "Fruity Filter",
    "Fruity Flanger",
    "Fruity Flangus",
    "Fruity Formula Controller",
    "Fruity Free Filter",
    "Fruity HTML NoteBook",
    "Fruity LSD",
    "Fruity Limiter",
    "Fruity Love Philter",
    "Fruity Multiband Compressor",
    "Fruity Mute 2",
    "Fruity NoteBook",
    "Fruity NoteBook 2",
    "Fruity PanOMatic",
    "Fruity Parametric EQ",
    "Fruity Parametric EQ 2",
    "Fruity Peak Controller",
    "Fruity Phase Inverter",
    "Fruity Phaser",
    "Fruity Reeverb",
    "Fruity Reeverb 2",
    "Fruity Scratcher",
    "Fruity Send",
    "Fruity Soft Clipper",
    "Fruity Spectroman",
    "Fruity Squeeze",
    "Fruity Stereo Enhancer",
    "Fruity Stereo Shaper",
    "Fruity Vocoder",
    "Fruity WaveShaper",
    "Fruity Wrapper",
    "Fruity X-Y Controller",
    "Fruity X-Y-Z Controller",
    "Fruity dB Meter",
    "Gross Beat",
    "Hardcore",
    "Maximus",
    "Newtone",
    "Patcher",
    "Pitcher",
    "Razer Chroma",
    "Soundgoodizer",
    "Transient Processor",
    "VFX Color Mapper",
    "VFX Key Mapper",
    "VFX Keyboard Splitter",
    "VFX Level Scaler",
    "Vocodex",
    "Wave Candy",
    "ZGameEditor Visualizer",
];

const FL200_GENERATORS: [&str; 45] = [
    "3x Osc",
    "Autogun",
    "BassDrum",
    "BeepMap",
    "BooBass",
    "Dashboard",
    "DirectWave",
    "Drumaxx",
    "Drumpad",
    "FL Keys",
    "FL Slayer",
    "FL Studio Mobile",
    "FPC",
    "Fruit Kick",
    "Fruity DX10",
    "Fruity Dance",
    "Fruity DrumSynth Live",
    "Fruity Envelope Controller",
    "Fruity Granulizer",
    "Fruity Keyboard Controller",
    "Fruity Slicer",
    "Fruity Soundfont Player",
    "Fruity Video Player",
    "Fruity Wrapper",
    "GMS",
    "Harmless",
    "Harmor",
    "MIDI Out",
    "MiniSynth",
    "Morphine",
    "Ogun",
    "Patcher",
    "Plucked!",
    "PoiZone",
    "ReWired",
    "Sakura",
    "Sawer",
    "SimSynth",
    "Slicex",
    "Sytrus",
    "Toxic Biohazard",
    "Transistor Bass",
    "Wasp",
    "Wasp XT",
    "Wave Traveller",
];
const FL200_LANE_LEN: usize = 62;
/* 20.1 raised the playlist from 199 to 500 tracks. 20.0.5 stops with "invalid data" on a lane
   record above 199. lanes are stored as 500 - track, so track 199 is lane 301. */
const FL200_TRACKS: u32 = 199;
const FL200_MIN_LANE: u16 = 500 - FL200_TRACKS as u16;

/* 21 and 25 write the same default lane colour and the same default channel colour. 20.0.5
   writes different values. a user-set colour differs from the default and passes through. */
const LANE_COLOUR_OFFSET: usize = 4;
const LANE_COLOUR_NEWER: [u8; 3] = [0x34, 0x38, 0x3A];
const LANE_COLOUR_FL200: [u8; 3] = [0x48, 0x51, 0x56];
const CHANNEL_COLOUR: u8 = 0x80;
const CHANNEL_COLOUR_NEWER: u32 = 0x0047_4541;
const CHANNEL_COLOUR_FL200: u32 = 0x006A_655C;

const WRAPPER_STATE_FL200: u32 = 8;
const WRAPPER_HOST_CHUNK: u32 = 57;
const WRAPPER_OPTIONS_CHUNK: u32 = 2;
const WRAPPER_OPTIONS_FLAG_OFFSET: usize = 12;

const LIMITER_STATE_FL208: u32 = 7;
const LIMITER_STATE_FL200: u32 = 6;
const LIMITER_LEN_FL200: usize = 168;

/* native states whose 20.0 form is byte-verified against the truth save. every other native
   plugin state is carried through unchanged and reported. */
const NATIVE_UNCHANGED: [&str; 3] = ["Fruity Delay 2", "Fruity Delay 3", "Fruity Parametric EQ"];

/* 20.0.5 hangs at playback on a Parametric EQ 2 state of version 7 or 8 (354 bytes). no 20.0.5
   save of the plugin is available. the 10.0.9 form (version 2, 305 bytes, verified in the 10
   profile) is the oldest form 20.0.5 loads. */
const EQ2_STATES_NEWER: [u32; 2] = [7, 8];
const EQ2_STATE_FL10: u32 = 2;
const EQ2_LEN_FL10: usize = 305;

struct Counts {
    deleted: usize,
    wrappers: usize,
    wrapper_versions: Vec<u32>,
    host_chunks: usize,
    option_flags: usize,
    limiters: usize,
    eq2: usize,
    lanes: usize,
    lane_src_len: usize,
    lanes_dropped: usize,
    lane_colours: usize,
    clips_clamped: usize,
    channel_colours: usize,
    d7: usize,
    unverified: Vec<String>,
    effects_dropped: Vec<String>,
    generators_kept: Vec<String>,
}

fn native_exists(shipped: &[&str], name: &str) -> bool {
    name.is_empty() || name == "Fruity Wrapper" || shipped.iter().any(|s| s.eq_ignore_ascii_case(name))
}

fn wrapper_state_fl200(b: &[u8], c: &mut Counts) -> Option<Vec<u8>> {
    let (version, chunks) = wrapper::parse_state(b)?;
    let mut kept: Vec<wrapper::Chunk> = Vec::with_capacity(chunks.len());
    for mut chunk in chunks {
        if chunk.id == WRAPPER_HOST_CHUNK {
            c.host_chunks += 1;
            continue;
        }
        if chunk.id == WRAPPER_OPTIONS_CHUNK
            && chunk.data.len() > WRAPPER_OPTIONS_FLAG_OFFSET
            && chunk.data[WRAPPER_OPTIONS_FLAG_OFFSET] != 0
        {
            chunk.data[WRAPPER_OPTIONS_FLAG_OFFSET] = 0;
            c.option_flags += 1;
        }
        kept.push(chunk);
    }
    if version != WRAPPER_STATE_FL200 {
        c.wrappers += 1;
        if !c.wrapper_versions.contains(&version) {
            c.wrapper_versions.push(version);
        }
    }
    Some(wrapper::build_state(WRAPPER_STATE_FL200, &kept))
}

/* Fruity Limiter grew one byte and one state version between 20.0.5 and 20.8. the same rule
   serves the 10 profile (see convert10::native_state_fl10). */
fn native_state_fl200(name: &str, b: &[u8], c: &mut Counts) -> Option<Vec<u8>> {
    if NATIVE_UNCHANGED.contains(&name) {
        return Some(b.to_vec());
    }
    if b.len() < 4 {
        return None;
    }
    let marker = u32::from_le_bytes(b[0..4].try_into().unwrap());
    if name == "Fruity Parametric EQ 2" {
        if !EQ2_STATES_NEWER.contains(&marker) || b.len() < EQ2_LEN_FL10 {
            return None;
        }
        let mut nb = b[..EQ2_LEN_FL10].to_vec();
        nb[0..4].copy_from_slice(&EQ2_STATE_FL10.to_le_bytes());
        c.eq2 += 1;
        return Some(nb);
    }
    if name != "Fruity Limiter" || marker != LIMITER_STATE_FL208 || b.len() < LIMITER_LEN_FL200 {
        return None;
    }
    let mut nb = b[..LIMITER_LEN_FL200].to_vec();
    nb[0..4].copy_from_slice(&LIMITER_STATE_FL200.to_le_bytes());
    c.limiters += 1;
    Some(nb)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

pub fn fl208_to_fl200(
    src: &Flp,
    notes: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Flp, String> {
    let mut c = Counts {
        deleted: 0,
        wrappers: 0,
        wrapper_versions: Vec::new(),
        host_chunks: 0,
        option_flags: 0,
        limiters: 0,
        eq2: 0,
        lanes: 0,
        lane_src_len: 0,
        lanes_dropped: 0,
        lane_colours: 0,
        clips_clamped: 0,
        channel_colours: 0,
        d7: 0,
        unverified: Vec::new(),
        effects_dropped: Vec::new(),
        generators_kept: Vec::new(),
    };
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
            o if POST_FL200_OPS.contains(&o) => c.deleted += 1,
            op::VERSION => {
                let mut b = FL200_VERSION.as_bytes().to_vec();
                b.push(0);
                out.push(Event { op: op::VERSION, payload: Payload::Blob(b) });
            }
            op::BUILD => out.push(Event { op: op::BUILD, payload: Payload::U32(FL200_BUILD) }),
            0x1C => out.push(Event { op: 0x1C, payload: Payload::U8(3) }),
            0xC8 => out.push(Event {
                op: 0xC8,
                payload: Payload::Blob(FL200_REGISTRATION.to_vec()),
            }),
            CHANNEL_COLOUR if ev.value() == Some(CHANNEL_COLOUR_NEWER) => {
                c.channel_colours += 1;
                out.push(Event {
                    op: CHANNEL_COLOUR,
                    payload: Payload::U32(CHANNEL_COLOUR_FL200),
                });
            }
            op::PLUGIN_INTERNAL_NAME => {
                current_plugin = ev.blob().map(flp::utf16z).unwrap_or_default();
                if in_mixer && !native_exists(&FL200_EFFECTS, &current_plugin) {
                    c.effects_dropped.push(current_plugin.clone());
                    skip_slot = true;
                    continue;
                }
                if !in_mixer && !native_exists(&FL200_GENERATORS, &current_plugin) {
                    c.generators_kept.push(current_plugin.clone());
                }
                out.push(ev.clone());
            }
            op::WRAPPER => {
                let b = ev.blob().unwrap_or_default();
                let nb = if current_plugin == "Fruity Wrapper" {
                    wrapper_state_fl200(b, &mut c)
                } else {
                    native_state_fl200(&current_plugin, b, &mut c)
                };
                match nb {
                    Some(nb) => out.push(Event { op: op::WRAPPER, payload: Payload::Blob(nb) }),
                    None => {
                        if !c.unverified.contains(&current_plugin) {
                            c.unverified.push(current_plugin.clone());
                        }
                        out.push(ev.clone());
                    }
                }
            }
            op::CHANNEL_DECO => {
                let b = ev.blob().ok_or("0xD7 without blob payload")?;
                if b.len() > FL200_D7_LEN {
                    c.d7 += 1;
                    out.push(Event {
                        op: op::CHANNEL_DECO,
                        payload: Payload::Blob(b[..FL200_D7_LEN].to_vec()),
                    });
                } else {
                    out.push(ev.clone());
                }
            }
            op::PLAYLIST => {
                let b = ev.blob().ok_or("0xE9 without blob payload")?;
                let mut nb = b.to_vec();
                for rec in nb.chunks_exact_mut(32) {
                    let lane = u16::from_le_bytes([rec[12], rec[13]]);
                    if lane < FL200_MIN_LANE {
                        rec[12..14].copy_from_slice(&FL200_MIN_LANE.to_le_bytes());
                        c.clips_clamped += 1;
                    }
                }
                out.push(Event { op: op::PLAYLIST, payload: Payload::Blob(nb) });
            }
            op::LANE => {
                let b = ev.blob().ok_or("0xEE without blob payload")?;
                let index = b
                    .get(..4)
                    .map(|h| u32::from_le_bytes([h[0], h[1], h[2], h[3]]))
                    .unwrap_or(0);
                if index > FL200_TRACKS {
                    c.lanes_dropped += 1;
                    continue;
                }
                let mut nb = if b.len() > FL200_LANE_LEN {
                    if c.lanes == 0 {
                        c.lane_src_len = b.len();
                    }
                    c.lanes += 1;
                    b[..FL200_LANE_LEN].to_vec()
                } else {
                    b.to_vec()
                };
                let colour = LANE_COLOUR_OFFSET..LANE_COLOUR_OFFSET + 3;
                if nb.len() >= colour.end && nb[colour.clone()] == LANE_COLOUR_NEWER {
                    nb[colour].copy_from_slice(&LANE_COLOUR_FL200);
                    c.lane_colours += 1;
                }
                out.push(Event { op: op::LANE, payload: Payload::Blob(nb) });
            }
            _ => out.push(ev.clone()),
        }
    }

    if c.deleted > 0 {
        notes.push(format!("deleted {} events 20.0.5 never writes", c.deleted));
    }
    if c.lanes > 0 {
        notes.push(format!(
            "truncated {} lane records {} -> {FL200_LANE_LEN} bytes",
            c.lanes, c.lane_src_len
        ));
    }
    if c.lanes_dropped > 0 {
        notes.push(format!(
            "dropped {} lane records above track {FL200_TRACKS}",
            c.lanes_dropped
        ));
    }
    if c.clips_clamped > 0 {
        warnings.push(format!(
            "{} playlist clip{} sat above track {FL200_TRACKS}; moved to track {FL200_TRACKS}, which 20.0.5 has as its last track",
            c.clips_clamped,
            plural(c.clips_clamped)
        ));
    }
    if c.d7 > 0 {
        notes.push(format!(
            "truncated {} channel blobs 0xD7 to {FL200_D7_LEN} bytes",
            c.d7
        ));
    }
    if c.wrappers > 0 || c.host_chunks > 0 || c.option_flags > 0 {
        c.wrapper_versions.sort_unstable();
        let head = if c.wrappers > 0 {
            let from = c
                .wrapper_versions
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "wrapper state version {from} -> {WRAPPER_STATE_FL200} on {} plugin{}",
                c.wrappers,
                plural(c.wrappers)
            )
        } else {
            format!("wrapper state already version {WRAPPER_STATE_FL200}")
        };
        notes.push(format!(
            "{head} (chunk {WRAPPER_HOST_CHUNK} removed on {}, chunk {WRAPPER_OPTIONS_CHUNK} flag cleared on {})",
            c.host_chunks, c.option_flags
        ));
    }
    if c.limiters > 0 {
        notes.push(format!(
            "Fruity Limiter state {LIMITER_STATE_FL208} -> {LIMITER_STATE_FL200} on {} plugin{}",
            c.limiters,
            plural(c.limiters)
        ));
    }
    if c.eq2 > 0 {
        notes.push(format!(
            "Fruity Parametric EQ 2 state {} -> {EQ2_STATE_FL10} ({EQ2_LEN_FL10} bytes, the 10.0.9 form) on {} plugin{}",
            "7/8", c.eq2, plural(c.eq2)
        ));
        warnings.push(format!(
            "{} Fruity Parametric EQ 2 state{} written in the 10.0.9 form; a 20.0.5 save of the plugin is needed to verify the settings survive",
            c.eq2, plural(c.eq2)
        ));
    }
    if c.lane_colours > 0 {
        notes.push(format!("set {} default lane colours to the 20.0.5 value", c.lane_colours));
    }
    if c.channel_colours > 0 {
        notes.push(format!(
            "set {} default channel colours to the 20.0.5 value",
            c.channel_colours
        ));
    }
    if !c.effects_dropped.is_empty() {
        warnings.push(format!(
            "{} mixer effect{} dropped, the plugin did not exist in 20.0.5: {}",
            c.effects_dropped.len(),
            plural(c.effects_dropped.len()),
            c.effects_dropped.join(", ")
        ));
    }
    if !c.generators_kept.is_empty() {
        warnings.push(format!(
            "{} generator{} kept although the plugin did not exist in 20.0.5, 20.0.5 will report it missing: {}",
            c.generators_kept.len(),
            plural(c.generators_kept.len()),
            c.generators_kept.join(", ")
        ));
    }
    if !c.unverified.is_empty() {
        warnings.push(format!(
            "plugin state carried unchanged, unverified in FL 20.0: {}",
            c.unverified.join(", ")
        ));
    }

    let leftovers: Vec<u8> = {
        let mut seen: Vec<u8> = out
            .iter()
            .map(|e| e.op)
            .filter(|o| !FL20_KNOWN_OPS.contains(o))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    if !leftovers.is_empty() {
        warnings.push(format!(
            "events kept that v20.0.5 is not known to write: {}",
            leftovers
                .iter()
                .map(|o| format!("0x{o:02X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(Flp {
        format: src.format,
        n_channels: src.n_channels,
        ppq: src.ppq,
        header_raw: src.header_raw.clone(),
        events: out,
        trailing: src.trailing.clone(),
    })
}

pub fn to_fl200(src: &Flp) -> Result<Outcome, String> {
    let version = src.version().ok_or("file has no version event (0xC7)")?;
    let major = src
        .version_major()
        .ok_or_else(|| format!("cannot parse version \"{version}\""))?;
    let minor = src
        .version_minor()
        .ok_or_else(|| format!("cannot parse version \"{version}\""))?;
    if major < 20 || (major == 20 && minor == 0) {
        return Err(format!("already v{version}, nothing to convert"));
    }
    let (base, mut notes, mut warnings) = if major > 20 {
        let o = convert::to_fl208(src)?;
        (o.flp, o.notes, o.warnings)
    } else {
        (
            Flp {
                format: src.format,
                n_channels: src.n_channels,
                ppq: src.ppq,
                header_raw: src.header_raw.clone(),
                events: src.events.clone(),
                trailing: src.trailing.clone(),
            },
            Vec::new(),
            Vec::new(),
        )
    };
    let base_version = base.version().unwrap_or_default();
    let out = fl208_to_fl200(&base, &mut notes, &mut warnings)?;
    notes.insert(0, format!("version {base_version} -> {FL200_VERSION}"));
    Ok(Outcome { flp: out, notes, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts() -> Counts {
        Counts {
            deleted: 0,
            wrappers: 0,
            wrapper_versions: Vec::new(),
            host_chunks: 0,
            option_flags: 0,
            limiters: 0,
            eq2: 0,
            lanes: 0,
            lane_src_len: 0,
            lanes_dropped: 0,
            lane_colours: 0,
            clips_clamped: 0,
            channel_colours: 0,
            d7: 0,
            unverified: Vec::new(),
            effects_dropped: Vec::new(),
            generators_kept: Vec::new(),
        }
    }

    fn chunk(id: u32, data: Vec<u8>) -> wrapper::Chunk {
        wrapper::Chunk { id, data }
    }

    fn flp_of(events: Vec<Event>) -> Flp {
        let mut header_raw = vec![0u8; 6];
        header_raw[4..6].copy_from_slice(&96u16.to_le_bytes());
        Flp {
            format: 0,
            n_channels: 1,
            ppq: 96,
            header_raw,
            events,
            trailing: Vec::new(),
        }
    }

    fn version_event(text: &str) -> Event {
        let mut b = text.as_bytes().to_vec();
        b.push(0);
        Event { op: op::VERSION, payload: Payload::Blob(b) }
    }

    #[test]
    fn wrapper_drops_host_chunk_and_clears_option_flag() {
        let mut options = vec![0u8; 25];
        options[WRAPPER_OPTIONS_FLAG_OFFSET] = 0xA0;
        options[17] = 1;
        let state = wrapper::build_state(
            10,
            &[
                chunk(WRAPPER_OPTIONS_CHUNK, options),
                chunk(53, vec![9; 7]),
                chunk(WRAPPER_HOST_CHUNK, vec![0x60, 0x09, 0x00, 0x00]),
            ],
        );
        let mut c = counts();
        let out = wrapper_state_fl200(&state, &mut c).unwrap();
        let (version, chunks) = wrapper::parse_state(&out).unwrap();
        assert_eq!(version, 8);
        assert!(chunks.iter().all(|c| c.id != WRAPPER_HOST_CHUNK));
        let opts = chunks.iter().find(|c| c.id == WRAPPER_OPTIONS_CHUNK).unwrap();
        assert_eq!(opts.data[WRAPPER_OPTIONS_FLAG_OFFSET], 0);
        assert_eq!(opts.data[17], 1);
        let plugin = chunks.iter().find(|c| c.id == 53).unwrap();
        assert_eq!(plugin.data, vec![9; 7]);
        assert_eq!((c.host_chunks, c.option_flags, c.wrappers), (1, 1, 1));
    }

    #[test]
    fn limiter_state_drops_to_six() {
        let mut b = vec![0u8; 169];
        b[0..4].copy_from_slice(&7u32.to_le_bytes());
        b[8] = 0x5A;
        let mut c = counts();
        let out = native_state_fl200("Fruity Limiter", &b, &mut c).unwrap();
        assert_eq!(out.len(), 168);
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 6);
        assert_eq!(out[8], 0x5A);
        assert_eq!(c.limiters, 1);
    }

    #[test]
    fn unknown_native_state_passes_through_and_warns() {
        let src = flp_of(vec![
            version_event("20.8.4.2576"),
            Event {
                op: op::PLUGIN_INTERNAL_NAME,
                payload: Payload::Blob("Maximus".encode_utf16().flat_map(u16::to_le_bytes).chain([0, 0]).collect()),
            },
            Event { op: op::WRAPPER, payload: Payload::Blob(vec![3, 0, 15, 0, 7]) },
        ]);
        let (mut notes, mut warnings) = (Vec::new(), Vec::new());
        let out = fl208_to_fl200(&src, &mut notes, &mut warnings).unwrap();
        let state = out.events.iter().find(|e| e.op == op::WRAPPER).unwrap();
        assert_eq!(state.blob(), Some(&[3, 0, 15, 0, 7][..]));
        assert!(warnings.iter().any(|w| w.contains("unverified in FL 20.0") && w.contains("Maximus")));
    }

    #[test]
    fn lane_records_truncate_and_recolour() {
        let mut rec = vec![0u8; 66];
        rec[0..4].copy_from_slice(&7u32.to_le_bytes());
        rec[LANE_COLOUR_OFFSET..LANE_COLOUR_OFFSET + 3].copy_from_slice(&LANE_COLOUR_NEWER);
        rec[61] = 1;
        let src = flp_of(vec![
            version_event("20.8.4.2576"),
            Event { op: op::LANE, payload: Payload::Blob(rec) },
        ]);
        let (mut notes, mut warnings) = (Vec::new(), Vec::new());
        let out = fl208_to_fl200(&src, &mut notes, &mut warnings).unwrap();
        let b = out.events.iter().find(|e| e.op == op::LANE).unwrap().blob().unwrap();
        assert_eq!(b.len(), FL200_LANE_LEN);
        assert_eq!(b[LANE_COLOUR_OFFSET..LANE_COLOUR_OFFSET + 3], LANE_COLOUR_FL200);
        assert_eq!(b[61], 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn channel_blob_truncates_to_157() {
        let src = flp_of(vec![
            version_event("20.8.4.2576"),
            Event { op: op::CHANNEL_DECO, payload: Payload::Blob((0..158u32).map(|v| v as u8).collect()) },
        ]);
        let (mut notes, mut warnings) = (Vec::new(), Vec::new());
        let out = fl208_to_fl200(&src, &mut notes, &mut warnings).unwrap();
        let b = out.events.iter().find(|e| e.op == op::CHANNEL_DECO).unwrap().blob().unwrap();
        assert_eq!(b.len(), FL200_D7_LEN);
        assert_eq!(b[156], 156);
    }

    #[test]
    fn a_20_0_source_is_refused() {
        let src = flp_of(vec![version_event("20.0.5.681")]);
        let Err(err) = to_fl200(&src) else {
            panic!("a 20.0 source must be refused");
        };
        assert!(err.contains("nothing to convert"));
    }

    #[test]
    fn a_20_8_source_takes_the_post_pass_only() {
        let src = flp_of(vec![
            version_event("20.8.4.2576"),
            Event { op: op::BUILD, payload: Payload::U32(2576) },
            Event { op: 0x26, payload: Payload::U8(1) },
            Event { op: 0x1C, payload: Payload::U8(1) },
        ]);
        let outcome = to_fl200(&src).unwrap();
        assert_eq!(outcome.flp.version().as_deref(), Some(FL200_VERSION));
        assert_eq!(outcome.notes[0], format!("version 20.8.4.2576 -> {FL200_VERSION}"));
        assert!(outcome.flp.events.iter().all(|e| e.op != 0x26));
        assert_eq!(
            outcome.flp.events.iter().find(|e| e.op == 0x1C).unwrap().value(),
            Some(3)
        );
        assert_eq!(
            outcome.flp.events.iter().find(|e| e.op == op::BUILD).unwrap().value(),
            Some(FL200_BUILD)
        );
    }
}
