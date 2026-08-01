use crate::flp::{op, Event, Flp, Payload};
use crate::info;

pub struct Outcome {
    pub flp: Flp,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

pub const FL20_VERSION: &str = "20.8.4.2576";
pub const FL20_BUILD: u32 = 2576;

/* 20.8's registration blob is 50 bytes. newer versions write a 10-byte stand-in,
   and whether 20.8 accepts that is untested — so we ship the verified 50-byte one. */
const FL20_REGISTRATION: [u8; 50] = [
    0x70, 0x00, 0x62, 0x00, 0x63, 0x00, 0x6B, 0x00, 0x65, 0x00, 0x67, 0x00, 0x5F, 0x00, 0x5B,
    0x00, 0x5E, 0x00, 0x56, 0x00, 0x52, 0x00, 0x5D, 0x00, 0x57, 0x00, 0x58, 0x00, 0x5B, 0x00,
    0x61, 0x00, 0x3D, 0x00, 0x41, 0x00, 0x3B, 0x00, 0x40, 0x00, 0x3A, 0x00, 0x39, 0x00, 0x35,
    0x00, 0x39, 0x00, 0x00, 0x00,
];

/* opcodes 20.8 never writes. derived by byte-diffing the same project saved by 20.8.4 and 25.1.4,
   plus 0xAC (25's fixed-3-byte event). 0x68 is absent on purpose: it converts to 0x16 before
   deletion would apply. 0xD8 was here until two genuine 20.8.4.2576 saves proved it is a
   v20-era event (see FORMAT.md). */
const POST_FL20_OPS: [u8; 18] = [
    0x29, 0x2A, 0x2B, 0x2C, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x67, 0xA5, 0xA6, 0xA7, 0xA9, 0xAA,
    0xAC, 0xF2, 0xF3,
];

/* every opcode observed in a 20.8 save, plus 20-era events our truth file happens not to use
   (0x5F insert icon, 0x87 sampler root note, 0x95/0xCC insert colour+name, 0xC1 pattern name,
   0xEF lane name, 0xD0 legacy notes). leftovers outside this set are reported, never deleted. */
const FL20_KNOWN_OPS: [u8; 104] = [
    0x00, 0x09, 0x0A, 0x0B, 0x11, 0x12, 0x14, 0x15, 0x16, 0x17, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
    0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x40, 0x41, 0x43, 0x45, 0x46, 0x47, 0x4A,
    0x4B, 0x4C, 0x50, 0x53, 0x55, 0x56, 0x59, 0x5F, 0x61, 0x62, 0x63, 0x64, 0x80, 0x83, 0x84,
    0x85, 0x87, 0x8A, 0x8B, 0x8F, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x9A, 0x9B, 0x9C,
    0x9D, 0x9E, 0x9F, 0xA4,
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF, 0xD0,
    0xD1, 0xD4, 0xD5, 0xD7, 0xD8, 0xD9, 0xDA, 0xDB, 0xDD, 0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4,
    0xE5, 0xE7, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF1,
];

/* 20.8 writes a fixed 4697-record 0xE1 table: a header record, then per strip the ten slot pairs
   and a fixed pid run whose tail shortens on high strips. 25 rebases targets to 0x7000 (addressing
   the "current" strip as strip 501) and drops the send/190 tail, so the table is rebuilt from
   scratch: source values where present, stock defaults elsewhere, unrecognised records appended
   after their strip's canonical run. */
fn rebuild_mixer_params(b: &[u8], rebased: &mut usize) -> Vec<u8> {
    use std::collections::BTreeMap;

    let mut existing: BTreeMap<(u16, u16, u8), i32> = BTreeMap::new();
    let mut extras: Vec<(u16, Vec<u8>)> = Vec::new();
    let mut verbatim: Vec<Vec<u8>> = Vec::new();

    for rec in b.chunks_exact(12) {
        let pid = rec[4];
        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
        let val = i32::from_le_bytes(rec[8..12].try_into().unwrap());
        if tgt == 0x4000 && pid == 0 {
            continue;
        }
        let (base, strip) = if tgt >= 0x7000 {
            *rebased += 1;
            (0x7000u16, ((tgt - 0x7000) >> 6).min(126))
        } else if tgt >= 0x2000 {
            (0x2000u16, ((tgt - 0x2000) >> 6).min(126))
        } else {
            verbatim.push(rec.to_vec());
            continue;
        };
        let off = (tgt - base) & 0x3F;
        if existing.insert((strip, off, pid), val).is_some() || !canonical_pid(off, pid) {
            let nt = 0x2000 + strip * 0x40 + off;
            let mut r = rec.to_vec();
            r[6..8].copy_from_slice(&nt.to_le_bytes());
            extras.push((strip, r));
        }
    }

    let mut out = Vec::with_capacity(4697 * 12);
    let push = |pid: u8, group: u8, tgt: u16, val: i32, out: &mut Vec<u8>| {
        out.extend_from_slice(&[0, 0, 0, 0, pid, group]);
        out.extend_from_slice(&tgt.to_le_bytes());
        out.extend_from_slice(&val.to_le_bytes());
    };
    push(0, 0x00, 0x4000, 12800, &mut out);
    out.extend(verbatim.into_iter().flatten());

    for strip in 0u16..127 {
        let base = 0x2000 + strip * 0x40;
        let mut get = |off: u16, pid: u8, default: i32| {
            existing.remove(&(strip, off, pid)).unwrap_or(default)
        };
        for slot in 0u16..10 {
            let en = get(slot, 0, 1);
            let mix = get(slot, 1, 12800);
            push(0, 0x1F, base + slot, en, &mut out);
            push(1, 0x1F, base + slot, mix, &mut out);
        }
        for (pid, default) in strip_pid_run(strip) {
            let v = get(0, pid, default);
            push(pid, 0x1F, base, v, &mut out);
        }
        for (_, r) in extras.iter().filter(|(s, _)| *s == strip) {
            out.extend_from_slice(r);
        }
    }
    out
}

fn strip_pid_run(strip: u16) -> Vec<(u8, i32)> {
    let mut run = vec![
        (192u8, 12800i32),
        (193, 0),
        (194, 0),
        (208, 0),
        (209, 0),
        (210, 0),
        (216, 5777),
        (217, 33145),
        (218, 55825),
        (224, 17500),
        (225, 17500),
        (226, 17500),
    ];
    if strip <= 99 {
        run.extend([(164, 0), (165, 0), (166, 0), (167, 0), (168, 0)]);
    } else if strip <= 104 {
        run.push((168, 0));
    }
    run.push((190, 0));
    run
}

fn canonical_pid(off: u16, pid: u8) -> bool {
    if pid <= 1 {
        return off < 10;
    }
    off == 0
        && matches!(pid, 192 | 193 | 194 | 208..=210 | 216..=218 | 224..=226 | 164..=168 | 190)
}

/* the sampler stretch time (0xD7 offset 96) uses the same 1/768-bar unit in both versions
   (bars*768 + steps*48 + ticks). only the storage type changed: 20.8 stores u32, 25 stores
   f32. the conversion is a reinterpretation, not a rescale — the project ppq does not enter
   it. legit u32 values stay far below 0x1000_0000 (349k bars), while an f32 for any value
   >= 1 has bits >= 0x3F80_0000, so the bit pattern alone separates the two encodings. */
fn stretch_time_units(d7: &[u8]) -> Option<f64> {
    if d7.len() < 100 {
        return None;
    }
    let raw = u32::from_le_bytes(d7[96..100].try_into().unwrap());
    let units = if raw < 0x1000_0000 {
        raw as f64
    } else {
        f32::from_bits(raw) as f64
    };
    (units.is_finite() && units > 0.0 && units < 10_000_000.0).then_some(units)
}

fn fix_stretch_time(
    d7: &mut [u8],
    scale: Option<f64>,
    fixed: &mut usize,
    scales_folded: &mut usize,
) {
    let Some(mut units) = stretch_time_units(d7) else {
        return;
    };
    let raw = u32::from_le_bytes(d7[96..100].try_into().unwrap());
    if raw >= 0x1000_0000 {
        *fixed += 1;
    }
    if let Some(scale) = scale {
        units *= scale;
        *scales_folded += 1;
    }
    let v = units.round() as u32;
    d7[96..100].copy_from_slice(&v.to_le_bytes());
}

/* 21+ audio clips carry fade-in/fade-out times (f32 milliseconds at record offsets 36/44)
   that v20 has no field for. The fades are emulated the way a v20 user would build them by
   hand: one automation channel per distinct (channel, length, fades) clip shape, linked to
   the audio channel's volume, with a matching clip on a free playlist lane under every
   faded instance. All template bytes below are copied from a genuine 20.8.4.2576 save of
   exactly this construction. */
const FADE_TENSION: f32 = -0.280_755;
const POINT_MID: u32 = 0x0200_0000;
const POINT_LAST: u32 = 0xFF00_0000;
const EA_HEADER: [u8; 17] = [
    0x01, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x03, 0x00,
    0x00, 0x00,
];
const EA_TAIL: [u8; 112] = [
    0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xF0, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFB, 0xB2, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const AUTO_D4: [u8; 52] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
    0x00, 0x50, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA8, 0x00, 0x00, 0x00, 0xFC, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const AUTO_D1: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
    0x00, 0x90, 0x00, 0x00, 0x00,
];
const AUTO_DB: [u8; 24] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const AUTO_E5: [u8; 20] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00,
];
const AUTO_DD: [u8; 9] = [0x01, 0x00, 0x00, 0x00, 0xF4, 0x01, 0x00, 0x00, 0x00];
const AUTO_D7: [u8; 158] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
    0xFF, 0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00,
    0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x04, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0xA7, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF0, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x01,
];
const AUTO_E4_0: [u8; 16] = [
    0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00,
];
const AUTO_E4_1: [u8; 16] = [
    0x3C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00,
];
const AUTO_DA: [u8; 68] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x20, 0x4E, 0x00,
    0x00, 0x20, 0x4E, 0x00, 0x00, 0x30, 0x75, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x20, 0x4E,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x20, 0x4E, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xB6, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const AUTO_DA_1: [u8; 68] = [
    0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x20, 0x4E, 0x00,
    0x00, 0x20, 0x4E, 0x00, 0x00, 0x30, 0x75, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x20, 0x4E,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x20, 0x4E, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xB6, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x9B, 0xFF, 0xFF, 0xFF,
];

struct FadeGroup {
    chan: u16,
    len_ticks: u32,
    fade_in_bits: u32,
    fade_out_bits: u32,
    auto_idx: u16,
    lane: i32,
    level: f64,
    colour: u32,
    name: String,
    len_beats: f64,
    fade_in_beats: f64,
    fade_out_beats: f64,
}

const CLIP_FADE_IN_OFFSET: usize = 36;
const CLIP_FADE_OUT_OFFSET: usize = 44;
const CLIP_STRETCH_SCALE_OFFSET: usize = 64;

fn fade_time(bits: u32) -> Option<f32> {
    let ms = f32::from_bits(bits);
    (ms.is_finite() && ms > 0.05 && ms < 600_000.0).then_some(ms)
}

fn clip_fade_bits(rec: &[u8]) -> (u32, u32) {
    (
        u32::from_le_bytes(
            rec[CLIP_FADE_IN_OFFSET..CLIP_FADE_IN_OFFSET + 4]
                .try_into()
                .unwrap(),
        ),
        u32::from_le_bytes(
            rec[CLIP_FADE_OUT_OFFSET..CLIP_FADE_OUT_OFFSET + 4]
                .try_into()
                .unwrap(),
        ),
    )
}

fn clip_stretch_scale(rec: &[u8], clip_size: usize) -> Option<f64> {
    if clip_size < 80 {
        return None;
    }
    let scale = f64::from_le_bytes(
        rec[CLIP_STRETCH_SCALE_OFFSET..CLIP_STRETCH_SCALE_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

fn channels_with_stretch_time(src: &Flp) -> Vec<bool> {
    let mut flags = vec![false; usize::from(src.n_channels)];
    let mut channel = None;
    for event in &src.events {
        match event.op {
            op::CHANNEL_NEW => channel = event.value().map(|value| value as usize),
            op::CHANNEL_DECO => {
                if let (Some(channel), Some(d7)) = (channel, event.blob()) {
                    if let Some(slot) = flags.get_mut(channel) {
                        *slot = stretch_time_units(d7).is_some();
                    }
                }
            }
            _ => {}
        }
    }
    flags
}

fn channel_stretch_scales(
    src: &Flp,
    clip_size: usize,
    warnings: &mut Vec<String>,
) -> Vec<Option<f64>> {
    let mut observed: Vec<Option<f64>> = vec![None; usize::from(src.n_channels)];
    let mut conflicts = vec![false; observed.len()];
    for event in src.events.iter().filter(|event| event.op == op::PLAYLIST) {
        let Some(blob) = event.blob() else { continue };
        for rec in blob.chunks_exact(clip_size) {
            let channel = usize::from(u16::from_le_bytes([rec[6], rec[7]]));
            let Some(slot) = observed.get_mut(channel) else {
                continue;
            };
            let Some(scale) = clip_stretch_scale(rec, clip_size) else {
                continue;
            };
            if let Some(previous) = *slot {
                if (previous - scale).abs() > 1e-9 {
                    conflicts[channel] = true;
                }
            } else {
                *slot = Some(scale);
            }
        }
    }

    let has_stretch_time = channels_with_stretch_time(src);

    for channel in 0..observed.len() {
        let unity = observed[channel].is_some_and(|scale| (scale - 1.0).abs() <= 1e-12);
        if conflicts[channel] {
            observed[channel] = None;
            warnings.push(format!(
                "channel {channel} uses multiple v25 clip stretch scales; retimed its clips individually"
            ));
        } else if unity {
            observed[channel] = None;
        } else if observed[channel].is_some() && !has_stretch_time[channel] {
            observed[channel] = None;
            warnings.push(format!(
                "channel {channel} has a v25 clip stretch scale without a sampler stretch time; retimed its clips individually"
            ));
        }
    }
    observed
}

fn fl20_clip_length(
    rec: &[u8],
    clip_size: usize,
    channel_scales: &[Option<f64>],
) -> (u32, bool) {
    let len = u32::from_le_bytes(rec[8..12].try_into().unwrap());
    let channel = usize::from(u16::from_le_bytes([rec[6], rec[7]]));
    if channel_scales.get(channel).is_some_and(Option::is_some) {
        return (len, false);
    }

    let Some(scale) = clip_stretch_scale(rec, clip_size) else {
        return (len, false);
    };
    if (scale - 1.0).abs() <= 1e-12 {
        return (len, false);
    }

    let minimum = u32::from(len > 0) as f64;
    let converted = (len as f64 / scale).floor().clamp(minimum, u32::MAX as f64) as u32;
    (converted, true)
}

const CLIP_START_TRIM_OFFSET: usize = 24;
const CLIP_END_TRIM_OFFSET: usize = 28;

fn tick_milliseconds(src: &Flp) -> Option<f64> {
    let bpm = src
        .events
        .iter()
        .find(|event| event.op == op::TEMPO)
        .and_then(Event::value)? as f64
        / 1000.0;
    (bpm > 0.0 && src.ppq > 0).then(|| 60_000.0 / (bpm * f64::from(src.ppq)))
}

/* the trims at record offsets 24/28 are milliseconds into the source sample, not ticks, in
   every version from 20 through 25. they are carried through untouched. */
fn clip_trim_window(
    rec: &[u8],
    clip_size: usize,
    channel_scales: &[Option<f64>],
) -> Option<(f64, f64, u32)> {
    let (len, _) = fl20_clip_length(rec, clip_size, channel_scales);
    if len == 0 {
        return None;
    }
    let read = |off: usize| f64::from(f32::from_le_bytes(rec[off..off + 4].try_into().unwrap()));
    let start = read(CLIP_START_TRIM_OFFSET);
    let end = read(CLIP_END_TRIM_OFFSET);
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    (end - start > 0.0).then_some((start, end - start, len))
}

/* v20.8 recomputes every audio clip length on load when the clip's channel has a nonzero sampler
   stretch time: len = round((end_ms - start_ms) / (R * tick_ms)), with R the source sample length
   divided by its stretched length. R lives in the sample, not the project, so it is estimated per
   channel from the channel's own clips. point estimates (window/len ratios) are biased by the
   rounding phase of the stored lengths — the program rounds len up or down, so every ratio sits
   off R to one side. instead, each clip constrains R to the open interval
   (window/((len+0.5)*tick), window/((len-0.5)*tick)): any R inside makes the recompute land back
   on the stored length. the midpoint of the deepest interval overlap is the estimate; clips whose
   interval misses it are exactly the ones v20 would resize. */
fn channel_playback_ratios(
    src: &Flp,
    clip_size: usize,
    channel_scales: &[Option<f64>],
    tick_ms: f64,
) -> Vec<Option<f64>> {
    let stretched = channels_with_stretch_time(src);
    let mut observed: Vec<Vec<(f64, f64)>> = vec![Vec::new(); stretched.len()];
    for event in src.events.iter().filter(|event| event.op == op::PLAYLIST) {
        let Some(blob) = event.blob() else { continue };
        for rec in blob.chunks_exact(clip_size) {
            let channel = u16::from_le_bytes([rec[6], rec[7]]);
            if channel >= 0x5000 {
                continue;
            }
            let Some(slot) = observed.get_mut(usize::from(channel)) else {
                continue;
            };
            let Some((_, window, len)) = clip_trim_window(rec, clip_size, channel_scales) else {
                continue;
            };
            slot.push((window, f64::from(len)));
        }
    }

    observed
        .into_iter()
        .zip(stretched)
        .map(|(clips, stretched)| {
            if !stretched || clips.len() < 2 {
                return None;
            }
            let mut bounds: Vec<(f64, i32)> = Vec::with_capacity(clips.len() * 2);
            for (window, len) in &clips {
                let lo = window / ((len + 0.5) * tick_ms);
                let hi = window / ((len - 0.5).max(0.5) * tick_ms);
                if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > lo {
                    bounds.push((lo, 1));
                    bounds.push((hi, -1));
                }
            }
            bounds.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
            let (mut depth, mut best, mut lo, mut span) = (0i32, 0i32, 0.0f64, None);
            for (edge, step) in bounds {
                if step > 0 {
                    depth += step;
                    if depth > best {
                        best = depth;
                        lo = edge;
                        span = None;
                    }
                } else {
                    if depth == best && span.is_none() {
                        span = Some((lo, edge));
                    }
                    depth += step;
                }
            }
            (best >= 2)
                .then_some(span)
                .flatten()
                .map(|(lo, hi)| (lo + hi) / 2.0)
        })
        .collect()
}

fn plan_fades(
    src: &Flp,
    clip_size: usize,
    channel_scales: &[Option<f64>],
    warnings: &mut Vec<String>,
) -> Vec<FadeGroup> {
    struct ChanMeta {
        vol: u32,
        colour: u32,
        name: String,
        stem: String,
    }

    let mut chans: Vec<ChanMeta> = Vec::new();
    let bpm = src
        .events
        .iter()
        .find(|e| e.op == op::TEMPO)
        .and_then(|e| e.value())
        .map(|v| v as f64 / 1000.0)
        .unwrap_or(120.0);

    for ev in &src.events {
        match ev.op {
            op::CHANNEL_NEW => chans.push(ChanMeta {
                vol: 10000,
                colour: 6316174,
                name: String::new(),
                stem: String::new(),
            }),
            0x63 => break,
            0x80 => {
                if let (Some(c), Some(v)) = (chans.last_mut(), ev.value()) {
                    c.colour = v;
                }
            }
            0xDB => {
                if let (Some(c), Some(b)) = (chans.last_mut(), ev.blob()) {
                    if b.len() >= 8 {
                        c.vol = u32::from_le_bytes(b[4..8].try_into().unwrap());
                    }
                }
            }
            op::NAME => {
                if let (Some(c), Some(b)) = (chans.last_mut(), ev.blob()) {
                    c.name = crate::flp::utf16z(b);
                }
            }
            op::SAMPLE_PATH => {
                if let (Some(c), Some(b)) = (chans.last_mut(), ev.blob()) {
                    let path = crate::flp::utf16z(b);
                    let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
                    c.stem = file.rsplit_once('.').map(|(s, _)| s).unwrap_or(file).to_string();
                }
            }
            _ => {}
        }
    }

    let mut groups: Vec<FadeGroup> = Vec::new();
    let mut min_lane = i32::MAX;
    for ev in src.events.iter().filter(|e| e.op == op::PLAYLIST) {
        let Some(b) = ev.blob() else { continue };
        for rec in b.chunks_exact(clip_size) {
            min_lane = min_lane.min(i32::from_le_bytes(rec[12..16].try_into().unwrap()));
            let chan = u16::from_le_bytes([rec[6], rec[7]]);
            if chan >= 0x5000 || usize::from(chan) >= chans.len() {
                continue;
            }
            let (fi, fo) = clip_fade_bits(rec);
            let (fi_ms, fo_ms) = (fade_time(fi), fade_time(fo));
            if fi_ms.is_none() && fo_ms.is_none() {
                continue;
            }
            let (len_ticks, _) = fl20_clip_length(rec, clip_size, channel_scales);
            let fi = fi_ms.map_or(0, |_| fi);
            let fo = fo_ms.map_or(0, |_| fo);
            if groups.iter().any(|g| {
                g.chan == chan
                    && g.len_ticks == len_ticks
                    && g.fade_in_bits == fi
                    && g.fade_out_bits == fo
            }) {
                continue;
            }
            let meta = &chans[usize::from(chan)];
            let base = if !meta.name.is_empty() {
                meta.name.clone()
            } else if !meta.stem.is_empty() {
                meta.stem.clone()
            } else {
                format!("Channel {}", chan + 1)
            };
            let len_beats = len_ticks as f64 / src.ppq as f64;
            let ms_to_beats = |ms: Option<f32>| ms.map_or(0.0, |m| m as f64 / 1000.0 * bpm / 60.0);
            let (mut fib, mut fob) = (ms_to_beats(fi_ms), ms_to_beats(fo_ms));
            if fib + fob > len_beats && fib + fob > 0.0 {
                let s = len_beats / (fib + fob);
                fib *= s;
                fob *= s;
            }
            groups.push(FadeGroup {
                chan,
                len_ticks,
                fade_in_bits: fi,
                fade_out_bits: fo,
                auto_idx: 0,
                lane: 0,
                level: (meta.vol.min(12800)) as f64 / 12800.0,
                colour: meta.colour,
                name: format!("{base} - Channel volume"),
                len_beats,
                fade_in_beats: fib,
                fade_out_beats: fob,
            });
        }
    }

    let n_chans = chans.len() as u16;
    let mut kept = Vec::new();
    for (i, mut g) in groups.into_iter().enumerate() {
        let lane = min_lane - 1 - i as i32;
        if lane < 0 {
            warnings.push(format!(
                "no free playlist lane left to emulate fades on \"{}\" — clip fades dropped",
                g.name
            ));
            continue;
        }
        g.auto_idx = n_chans + kept.len() as u16;
        g.lane = lane;
        kept.push(g);
    }
    kept
}

fn fade_points(g: &FadeGroup) -> Vec<(f64, f64, f32, u32)> {
    let mut pts = Vec::new();
    if g.fade_in_beats > 0.0 {
        pts.push((0.0, 0.0, 0.0, 0));
        pts.push((g.fade_in_beats, g.level, FADE_TENSION, POINT_MID));
    } else {
        pts.push((0.0, g.level, 0.0, 0));
    }
    if g.fade_out_beats > 0.0 {
        let hold = g.len_beats - g.fade_in_beats - g.fade_out_beats;
        if hold > 1e-9 {
            pts.push((hold, g.level, 0.0, POINT_MID));
        }
        pts.push((g.fade_out_beats, 0.0, FADE_TENSION, POINT_MID));
        /* zero-width final point: snaps the channel volume back to its knob value the
           instant the clip ends, so later un-faded clips of the same channel stay audible */
        pts.push((0.0, g.level, 0.0, POINT_LAST));
    } else {
        let m = pts.len() - 1;
        pts[m].3 = POINT_LAST;
        let rest = g.len_beats - g.fade_in_beats;
        if rest > 1e-9 {
            pts[m].3 = POINT_MID;
            pts.push((rest, g.level, 0.0, POINT_LAST));
        }
    }
    pts
}

fn fade_automation_events(g: &FadeGroup) -> Vec<Event> {
    let pts = fade_points(g);
    let mut ea = Vec::with_capacity(21 + 24 * pts.len() + EA_TAIL.len());
    ea.extend_from_slice(&EA_HEADER);
    ea.extend_from_slice(&(pts.len() as u32).to_le_bytes());
    for (pos, val, tension, flags) in pts {
        ea.extend_from_slice(&pos.to_le_bytes());
        ea.extend_from_slice(&val.to_le_bytes());
        ea.extend_from_slice(&tension.to_le_bytes());
        ea.extend_from_slice(&flags.to_le_bytes());
    }
    ea.extend_from_slice(&EA_TAIL);

    let mut name: Vec<u8> = g.name.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    name.extend_from_slice(&[0, 0]);

    let u8e = |o: u8, v: u8| Event { op: o, payload: Payload::U8(v) };
    let u16e = |o: u8, v: u16| Event { op: o, payload: Payload::U16(v) };
    let u32e = |o: u8, v: u32| Event { op: o, payload: Payload::U32(v) };
    let blob = |o: u8, b: &[u8]| Event { op: o, payload: Payload::Blob(b.to_vec()) };

    vec![
        u16e(op::CHANNEL_NEW, g.auto_idx),
        u8e(op::CHANNEL_KIND, 5),
        blob(op::PLUGIN_INTERNAL_NAME, &[0, 0]),
        blob(0xD4, &AUTO_D4),
        blob(op::NAME, &name),
        u32e(0x9B, 0),
        u32e(0x80, g.colour),
        u8e(0x00, 1),
        blob(0xD1, &AUTO_D1),
        u32e(0x8A, 8388736),
        u32e(0x8B, 65536),
        u16e(0x59, 0),
        u16e(0x61, 128),
        u16e(0x45, 128),
        u16e(0x56, 256),
        u16e(0x47, 1024),
        u16e(0x53, 0),
        u16e(0x4A, 0),
        u16e(0x4B, 0),
        u16e(0x4C, 0),
        u16e(0x55, 2048),
        u32e(0x83, 8388608),
        u16e(0x46, 0),
        u8e(op::CHANNEL_ROUTE, 0),
        blob(0xDB, &AUTO_DB),
        blob(0xE5, &AUTO_E5),
        blob(0xDD, &AUTO_DD),
        blob(op::CHANNEL_DECO, &AUTO_D7),
        u32e(0x84, 0),
        u32e(0x90, 0),
        u32e(0x91, 1),
        blob(0xEA, &ea),
        u8e(0x20, 0),
        blob(0xE4, &AUTO_E4_0),
        blob(0xE4, &AUTO_E4_1),
        blob(0xDA, &AUTO_DA),
        blob(0xDA, &AUTO_DA_1),
        blob(0xDA, &AUTO_DA),
        blob(0xDA, &AUTO_DA),
        blob(0xDA, &AUTO_DA),
        u32e(0x8F, 3),
        u8e(0x14, 0),
    ]
}

fn fade_link_event(g: &FadeGroup) -> Event {
    let mut b = vec![0u8; 20];
    b[2..4].copy_from_slice(&g.auto_idx.to_le_bytes());
    b[10..12].copy_from_slice(&g.chan.to_le_bytes());
    b[12..16].copy_from_slice(&8u32.to_le_bytes());
    b[16..20].copy_from_slice(&469u32.to_le_bytes());
    Event { op: op::AUTOMATION_LINK, payload: Payload::Blob(b) }
}

fn fade_clip_record(g: &FadeGroup, src_rec: &[u8]) -> [u8; 32] {
    let mut r = [0u8; 32];
    r[0..6].copy_from_slice(&src_rec[0..6]);
    r[6..8].copy_from_slice(&g.auto_idx.to_le_bytes());
    r[8..12].copy_from_slice(&g.len_ticks.to_le_bytes());
    r[12..16].copy_from_slice(&g.lane.to_le_bytes());
    r[16..24].copy_from_slice(&[0x78, 0x00, 0x40, 0x00, 0x40, 0x64, 0x80, 0x80]);
    r[28..32].copy_from_slice(&(g.len_beats as f32).to_le_bytes());
    r
}

pub fn to_fl20(src: &Flp) -> Result<Outcome, String> {
    let version = src.version().ok_or("file has no version event (0xC7)")?;
    let major = src
        .version_major()
        .ok_or_else(|| format!("cannot parse version \"{version}\""))?;
    if major <= 20 {
        return Err(format!("already v{version}, nothing to convert"));
    }

    let playlist_len = src
        .events
        .iter()
        .filter(|e| e.op == op::PLAYLIST)
        .filter_map(|e| e.blob())
        .map(|b| b.len())
        .find(|&l| l > 0)
        .unwrap_or(0);
    let clip_size = info::clip_record_size(major, playlist_len)
        .ok_or("playlist blob length matches no known clip record size")?;

    /* 0x68 replaces 0x16 in 25 at the same position in the channel param block, so an in-place
       rewrite lands the route where 20.8 expects it. a file carrying both per channel (some
       dual-write exports do this) keeps its 0x16 and drops the 0x68. */
    let mut channel_has_16 = vec![false];
    for ev in &src.events {
        match ev.op {
            op::CHANNEL_NEW => channel_has_16.push(false),
            op::CHANNEL_ROUTE => *channel_has_16.last_mut().unwrap() = true,
            _ => {}
        }
    }

    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    let mut out: Vec<Event> = Vec::with_capacity(src.events.len());

    let channel_scales = channel_stretch_scales(src, clip_size, &mut warnings);
    let tick_ms = (clip_size >= 60).then(|| tick_milliseconds(src)).flatten();
    let end_trim_ratios = match tick_ms {
        Some(tick) => channel_playback_ratios(src, clip_size, &channel_scales, tick),
        None => Vec::new(),
    };
    let fade_groups = if clip_size >= 60 {
        plan_fades(src, clip_size, &channel_scales, &mut warnings)
    } else {
        Vec::new()
    };
    let mut links_inserted = false;
    let mut channels_inserted = false;
    let mut d8_extended = false;

    let mut chan_idx = 0usize;
    let mut deleted = 0usize;
    let mut routes_converted = 0usize;
    let mut routes_dropped = 0usize;
    let mut markers = 0usize;
    let mut d7_truncated = 0usize;
    let mut clips_converted = 0usize;
    let mut lanes_truncated = 0usize;
    let mut tables_expanded = 0usize;
    let mut params_rebased = 0usize;
    let mut links_rebased = 0usize;
    let mut e1_rebuilt = 0usize;
    let mut clip_scales_applied = 0usize;
    let mut channel_scales_folded = 0usize;
    let mut stretch_fixed = 0usize;
    let mut fades_emulated = 0usize;
    let mut end_trims_reconciled = 0usize;
    let mut controls_rebased = 0usize;
    let mut current_channel = None;
    let mut current_plugin = String::new();

    for ev in &src.events {
        if ev.op == op::CHANNEL_NEW && !links_inserted {
            links_inserted = true;
            for g in &fade_groups {
                out.push(fade_link_event(g));
            }
        }
        if ev.op == 0x63 && !channels_inserted {
            channels_inserted = true;
            for g in &fade_groups {
                out.extend(fade_automation_events(g));
            }
        }
        match ev.op {
            o if POST_FL20_OPS.contains(&o) => deleted += 1,
            op::CHANNEL_NEW => {
                chan_idx += 1;
                current_channel = ev.value().map(|value| value as usize);
                out.push(ev.clone());
            }
            op::CHANNEL_ROUTE_FL25 => {
                if channel_has_16[chan_idx] {
                    routes_dropped += 1;
                } else {
                    let v = ev.value().unwrap_or(0).min(255) as u8;
                    out.push(Event { op: op::CHANNEL_ROUTE, payload: Payload::U8(v) });
                    routes_converted += 1;
                }
            }
            op::VERSION => {
                let mut b = FL20_VERSION.as_bytes().to_vec();
                b.push(0);
                out.push(Event { op: op::VERSION, payload: Payload::Blob(b) });
            }
            op::BUILD => out.push(Event { op: op::BUILD, payload: Payload::U32(FL20_BUILD) }),
            0xC8 => out.push(Event {
                op: 0xC8,
                payload: Payload::Blob(FL20_REGISTRATION.to_vec()),
            }),
            // saves carry out-of-range selected insert values (216 in both truth files); 126, the "current" strip, is the hand-verified one
            0x85 => out.push(Event {
                op: 0x85,
                payload: Payload::U32(ev.value().unwrap_or(126).min(126)),
            }),
            op::PLUGIN_INTERNAL_NAME => {
                current_plugin = ev.blob().map(crate::flp::utf16z).unwrap_or_default();
                out.push(ev.clone());
            }
            /* only the VST host's 0xD5 opens with a state version (12 in 25, 10 in 20.8). a native
               plugin's first u32 is its own state header — 786435 for Fruity Love Philter, 171 for
               Fruity Fast Dist — and clamping it corrupts the state. the owner is the plugin named
               by the preceding 0xC9. */
            op::WRAPPER => {
                let mut b = ev.blob().ok_or("0xD5 without blob payload")?.to_vec();
                if current_plugin == "Fruity Wrapper" && b.len() >= 4 {
                    let marker = u32::from_le_bytes(b[0..4].try_into().unwrap());
                    if marker > 10 {
                        b[0..4].copy_from_slice(&10u32.to_le_bytes());
                        markers += 1;
                    }
                }
                out.push(Event { op: op::WRAPPER, payload: Payload::Blob(b) });
            }
            op::CHANNEL_DECO => {
                let b = ev.blob().ok_or("0xD7 without blob payload")?;
                let mut nb = if b.len() > 158 {
                    d7_truncated += 1;
                    b[..158].to_vec()
                } else {
                    b.to_vec()
                };
                let scale = current_channel
                    .and_then(|channel| channel_scales.get(channel))
                    .copied()
                    .flatten();
                fix_stretch_time(
                    &mut nb,
                    scale,
                    &mut stretch_fixed,
                    &mut channel_scales_folded,
                );
                out.push(Event { op: op::CHANNEL_DECO, payload: Payload::Blob(nb) });
            }
            op::PLAYLIST => {
                let b = ev.blob().ok_or("0xE9 without blob payload")?;
                if clip_size == 32 || b.is_empty() {
                    out.push(ev.clone());
                } else {
                    if b.len() % clip_size != 0 {
                        return Err(format!(
                            "playlist blob of {} bytes is not a multiple of {clip_size}",
                            b.len()
                        ));
                    }
                    let mut nb = Vec::with_capacity(b.len() / clip_size * 32);
                    for rec in b.chunks_exact(clip_size) {
                        let (len, scale_applied) =
                            fl20_clip_length(rec, clip_size, &channel_scales);
                        let chan = u16::from_le_bytes([rec[6], rec[7]]);
                        let mut converted = rec[..32].to_vec();
                        converted[8..12].copy_from_slice(&len.to_le_bytes());
                        if let (Some(tick), Some(ratio)) = (
                            tick_ms,
                            end_trim_ratios.get(usize::from(chan)).copied().flatten(),
                        ) {
                            if let Some((start, window, final_len)) =
                                clip_trim_window(rec, clip_size, &channel_scales)
                            {
                                let implied = window / (ratio * tick);
                                let final_len = f64::from(final_len);
                                if implied.round() != final_len {
                                    if implied > final_len {
                                        let end = start + final_len * ratio * tick;
                                        converted[CLIP_END_TRIM_OFFSET..CLIP_END_TRIM_OFFSET + 4]
                                            .copy_from_slice(&(end as f32).to_le_bytes());
                                        end_trims_reconciled += 1;
                                    } else if implied < final_len - 0.5 {
                                        let pos =
                                            u32::from_le_bytes(rec[0..4].try_into().unwrap());
                                        warnings.push(format!(
                                            "channel {chan} clip at {pos}: trim window shorter than the clip; v20 shortens it on load"
                                        ));
                                    }
                                }
                            }
                        }
                        nb.extend_from_slice(&converted);
                        clips_converted += 1;
                        if scale_applied {
                            clip_scales_applied += 1;
                        }
                        let (fi, fo) = clip_fade_bits(rec);
                        let fi = fade_time(fi).map_or(0, |_| fi);
                        let fo = fade_time(fo).map_or(0, |_| fo);
                        if let Some(g) = fade_groups.iter().find(|g| {
                            g.chan == chan
                                && g.len_ticks == len
                                && g.fade_in_bits == fi
                                && g.fade_out_bits == fo
                                && (fi != 0 || fo != 0)
                        }) {
                            nb.extend_from_slice(&fade_clip_record(g, rec));
                            fades_emulated += 1;
                        }
                    }
                    out.push(Event { op: op::PLAYLIST, payload: Payload::Blob(nb) });
                }
            }
            op::LANE => {
                let b = ev.blob().ok_or("0xEE without blob payload")?;
                if b.len() > 66 {
                    lanes_truncated += 1;
                    out.push(Event { op: op::LANE, payload: Payload::Blob(b[..66].to_vec()) });
                } else {
                    out.push(ev.clone());
                }
            }
            op::ROUTE_TABLE => {
                let b = ev.blob().ok_or("0xEB without blob payload")?;
                if b.len() < 127 {
                    tables_expanded += 1;
                    let mut nb = b.to_vec();
                    nb.resize(127, 0);
                    out.push(Event { op: op::ROUTE_TABLE, payload: Payload::Blob(nb) });
                } else {
                    out.push(ev.clone());
                }
            }
            op::MIXER_PARAMS => {
                let b = ev.blob().ok_or("0xE1 without blob payload")?;
                if b.len() % 12 != 0 {
                    warnings.push(format!(
                        "mixer param blob of {} bytes is not a multiple of 12 — left unchanged",
                        b.len()
                    ));
                    out.push(ev.clone());
                } else {
                    let nb = rebuild_mixer_params(b, &mut params_rebased);
                    e1_rebuilt += 1;
                    out.push(Event { op: op::MIXER_PARAMS, payload: Payload::Blob(nb) });
                }
            }
            0xD8 => {
                let mut nb = ev.blob().ok_or("0xD8 without blob payload")?.to_vec();
                if nb.len() % 12 == 0 {
                    for rec in nb.chunks_exact_mut(12) {
                        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
                        if tgt >= 0x7000 {
                            let strip = ((tgt - 0x7000) >> 6).min(126);
                            let off = (tgt - 0x7000) & 0x3F;
                            let nt = 0x2000 + strip * 0x40 + off;
                            rec[6..8].copy_from_slice(&nt.to_le_bytes());
                            controls_rebased += 1;
                        }
                    }
                } else {
                    warnings.push(format!(
                        "initialised control blob of {} bytes is not a multiple of 12 — left unchanged",
                        nb.len()
                    ));
                }
                if !fade_groups.is_empty() && !d8_extended {
                    d8_extended = true;
                    for g in &fade_groups {
                        nb.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
                        nb.extend_from_slice(&g.chan.to_le_bytes());
                        nb.extend_from_slice(&((g.level * 12800.0).round() as i32).to_le_bytes());
                    }
                }
                out.push(Event { op: 0xD8, payload: Payload::Blob(nb) });
            }
            op::AUTOMATION_LINK => {
                let b = ev.blob().ok_or("0xE3 without blob payload")?;
                let tgt = if b.len() == 20 {
                    u16::from_le_bytes([b[10], b[11]])
                } else {
                    0
                };
                if tgt >= 0x7000 {
                    let strip = ((tgt - 0x7000) >> 6).min(126);
                    let off = (tgt - 0x7000) & 0x3F;
                    let nt = 0x2000 + strip * 0x40 + off;
                    let mut nb = b.to_vec();
                    nb[10..12].copy_from_slice(&nt.to_le_bytes());
                    links_rebased += 1;
                    out.push(Event { op: op::AUTOMATION_LINK, payload: Payload::Blob(nb) });
                } else {
                    out.push(ev.clone());
                }
            }
            _ => out.push(ev.clone()),
        }
    }

    let push = |n: usize, msg: String, to: &mut Vec<String>| {
        if n > 0 {
            to.push(msg);
        }
    };
    push(1, format!("version {version} -> {FL20_VERSION}"), &mut notes);
    push(deleted, format!("deleted {deleted} post-v20 events"), &mut notes);
    push(
        routes_converted,
        format!("converted {routes_converted} channel routes 0x68 -> 0x16"),
        &mut notes,
    );
    push(
        routes_dropped,
        format!("dropped {routes_dropped} duplicate 0x68 routes (0x16 already present)"),
        &mut notes,
    );
    push(markers, format!("wrapper marker 12 -> 10 on {markers} plugins"), &mut notes);
    push(
        clips_converted,
        format!("rewrote {clips_converted} playlist clips {clip_size} -> 32 bytes"),
        &mut notes,
    );
    push(
        lanes_truncated,
        format!("truncated {lanes_truncated} lane records 70 -> 66 bytes"),
        &mut notes,
    );
    push(
        tables_expanded,
        format!("expanded {tables_expanded} mixer route tables to 127 bytes"),
        &mut notes,
    );
    push(
        e1_rebuilt,
        "rebuilt mixer param table to the v20 canonical 4697-record shape".into(),
        &mut notes,
    );
    push(
        params_rebased,
        format!("rebased {params_rebased} mixer param targets 0x7000 -> 0x2000"),
        &mut notes,
    );
    push(
        links_rebased,
        format!("rebased {links_rebased} automation link targets 0x7000 -> 0x2000"),
        &mut notes,
    );
    push(
        controls_rebased,
        format!("rebased {controls_rebased} initialised control targets 0x7000 -> 0x2000"),
        &mut notes,
    );
    push(
        d7_truncated,
        format!("truncated {d7_truncated} channel blobs 0xD7 to 158 bytes"),
        &mut notes,
    );
    push(
        clip_scales_applied,
        format!(
            "retimed {clip_scales_applied} playlist clips whose v25 stretch scale could not move to the channel"
        ),
        &mut notes,
    );
    push(
        channel_scales_folded,
        format!(
            "folded {channel_scales_folded} v25 clip stretch scales into v20 channel stretch times"
        ),
        &mut notes,
    );
    push(
        stretch_fixed,
        format!("rewrote {stretch_fixed} sampler stretch times f32 -> u32"),
        &mut notes,
    );
    push(
        end_trims_reconciled,
        format!(
            "reconciled {end_trims_reconciled} audio clip end trims to the v20 length recompute"
        ),
        &mut notes,
    );
    push(
        fades_emulated,
        format!(
            "emulated {fades_emulated} clip fades with {} channel-volume automation clips",
            fade_groups.len()
        ),
        &mut notes,
    );

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
            "events kept that v20.8 is not known to write: {}",
            leftovers
                .iter()
                .map(|o| format!("0x{o:02X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let n_channels = src.n_channels + fade_groups.len() as u16;
    let mut header_raw = src.header_raw.clone();
    header_raw[2..4].copy_from_slice(&n_channels.to_le_bytes());

    Ok(Outcome {
        flp: Flp {
            format: src.format,
            n_channels,
            ppq: src.ppq,
            header_raw,
            events: out,
            trailing: src.trailing.clone(),
        },
        notes,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v25_clip(len: u32, fade_in: f32, fade_out: f32, scale: f64) -> Vec<u8> {
        let mut rec = vec![0u8; 80];
        rec[8..12].copy_from_slice(&len.to_le_bytes());
        rec[12..16].copy_from_slice(&499i32.to_le_bytes());
        rec[24..28].copy_from_slice(&1234.5f32.to_le_bytes());
        rec[28..32].copy_from_slice(&2345.5f32.to_le_bytes());
        rec[CLIP_FADE_IN_OFFSET..CLIP_FADE_IN_OFFSET + 4].copy_from_slice(&fade_in.to_le_bytes());
        rec[40..44].copy_from_slice(&(-0.25f32).to_le_bytes());
        rec[CLIP_FADE_OUT_OFFSET..CLIP_FADE_OUT_OFFSET + 4]
            .copy_from_slice(&fade_out.to_le_bytes());
        rec[52..56].copy_from_slice(&1.0f32.to_le_bytes());
        rec[CLIP_STRETCH_SCALE_OFFSET..CLIP_STRETCH_SCALE_OFFSET + 8]
            .copy_from_slice(&scale.to_le_bytes());
        rec
    }

    fn trimmed_clip(pos: u32, len: u32, start_ms: f32, end_ms: f32) -> Vec<u8> {
        let mut rec = v25_clip(len, 0.0, 0.0, 1.0);
        rec[0..4].copy_from_slice(&pos.to_le_bytes());
        rec[CLIP_START_TRIM_OFFSET..CLIP_START_TRIM_OFFSET + 4]
            .copy_from_slice(&start_ms.to_le_bytes());
        rec[CLIP_END_TRIM_OFFSET..CLIP_END_TRIM_OFFSET + 4].copy_from_slice(&end_ms.to_le_bytes());
        rec
    }

    fn test_tick_ms() -> f64 {
        60_000.0 / (148.0 * 96.0)
    }

    fn converted_playlist(flp: &Flp) -> (Vec<u8>, Outcome) {
        let outcome = to_fl20(flp).unwrap();
        let blob = outcome
            .flp
            .events
            .iter()
            .find(|event| event.op == op::PLAYLIST)
            .and_then(Event::blob)
            .unwrap()
            .to_vec();
        (blob, outcome)
    }

    fn source_flp(playlist: Vec<u8>) -> Flp {
        source_flp_with_stretch(playlist, 24576.0)
    }

    fn source_flp_with_stretch(playlist: Vec<u8>, stretch: f32) -> Flp {
        let mut d7 = vec![0u8; 168];
        d7[96..100].copy_from_slice(&stretch.to_le_bytes());
        Flp {
            format: 0,
            n_channels: 1,
            ppq: 96,
            header_raw: vec![0, 0, 1, 0, 96, 0],
            events: vec![
                Event {
                    op: op::VERSION,
                    payload: Payload::Blob(b"25.1.3.4922\0".to_vec()),
                },
                Event {
                    op: op::TEMPO,
                    payload: Payload::U32(148_000),
                },
                Event {
                    op: op::CHANNEL_NEW,
                    payload: Payload::U16(0),
                },
                Event {
                    op: op::CHANNEL_KIND,
                    payload: Payload::U8(4),
                },
                Event { op: op::CHANNEL_DECO, payload: Payload::Blob(d7) },
                Event {
                    op: 0x63,
                    payload: Payload::U16(0),
                },
                Event {
                    op: op::PLAYLIST,
                    payload: Payload::Blob(playlist),
                },
            ],
            trailing: Vec::new(),
        }
    }

    #[test]
    fn converts_v25_fade_in_and_stretch_scale() {
        let source = v25_clip(192, 500.0, 250.0, 0.5);
        let outcome = to_fl20(&source_flp(source.clone())).unwrap();
        let playlist = outcome
            .flp
            .events
            .iter()
            .find(|event| event.op == op::PLAYLIST)
            .and_then(Event::blob)
            .unwrap();

        assert_eq!(outcome.flp.n_channels, 2);
        assert_eq!(playlist.len(), 64);
        assert_eq!(u32::from_le_bytes(playlist[8..12].try_into().unwrap()), 192);
        assert_eq!(&playlist[24..32], &source[24..32]);
        let d7 = outcome
            .flp
            .events
            .iter()
            .find(|event| event.op == op::CHANNEL_DECO)
            .and_then(Event::blob)
            .unwrap();
        assert_eq!(u32::from_le_bytes(d7[96..100].try_into().unwrap()), 12288);
        assert!(outcome.flp.events.iter().any(|event| event.op == 0xEA));
        assert!(outcome
            .notes
            .iter()
            .any(|note| note.contains("emulated 1 clip fades")));
    }

    #[test]
    fn reconciles_wide_end_trim_on_stretched_channel() {
        let ratio = 0.5;
        let span = ratio * test_tick_ms();
        let mut playlist = Vec::new();
        for (i, len) in [96u32, 192, 384].into_iter().enumerate() {
            playlist.extend(trimmed_clip(i as u32 * 1000, len, 0.0, (len as f64 * span) as f32));
        }
        playlist.extend(trimmed_clip(9000, 192, 0.0, (194.0 * span) as f32));

        let (blob, outcome) = converted_playlist(&source_flp(playlist.clone()));
        assert_eq!(blob.len(), 4 * 32);
        for i in 0..3 {
            assert_eq!(&blob[i * 32 + 24..i * 32 + 32], &playlist[i * 80 + 24..i * 80 + 32]);
        }
        let end = f32::from_le_bytes(blob[3 * 32 + 28..3 * 32 + 32].try_into().unwrap());
        assert!((f64::from(end) - 192.0 * span).abs() < 0.05, "end trim was {end}");
        assert!(outcome
            .notes
            .iter()
            .any(|note| note == "reconciled 1 audio clip end trims to the v20 length recompute"));
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    }

    /* stored lengths come from the program rounding window/(R*tick) up: every window sits ~0.7
       of a tick short of its length. a ratio-averaging estimate of R lands low and rewrites
       trims that v20 would have reproduced exactly; the interval estimate must not touch them. */
    #[test]
    fn keeps_end_trims_whose_lengths_round_back_exactly() {
        let ratio = 1.15191;
        let span = ratio * test_tick_ms();
        let mut playlist = Vec::new();
        for (i, len) in [96u32, 192, 385, 24, 48, 128].into_iter().enumerate() {
            let window = (f64::from(len) - 0.3) * span;
            playlist.extend(trimmed_clip(i as u32 * 1000, len, 3615.0, 3615.0 + window as f32));
        }

        let (blob, outcome) = converted_playlist(&source_flp(playlist.clone()));
        for i in 0..6 {
            assert_eq!(
                &blob[i * 32 + 24..i * 32 + 32],
                &playlist[i * 80 + 24..i * 80 + 32],
                "clip {i} trim bytes changed"
            );
        }
        assert!(!outcome.notes.iter().any(|note| note.starts_with("reconciled")));
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    }

    #[test]
    fn warns_about_narrow_end_trim_on_stretched_channel() {
        let ratio = 0.5;
        let span = ratio * test_tick_ms();
        let mut playlist = Vec::new();
        for (i, len) in [96u32, 192, 384].into_iter().enumerate() {
            playlist.extend(trimmed_clip(i as u32 * 1000, len, 0.0, (len as f64 * span) as f32));
        }
        playlist.extend(trimmed_clip(9000, 192, 0.0, (180.0 * span) as f32));

        let (blob, outcome) = converted_playlist(&source_flp(playlist.clone()));
        assert_eq!(&blob[3 * 32 + 24..3 * 32 + 32], &playlist[3 * 80 + 24..3 * 80 + 32]);
        assert!(!outcome
            .notes
            .iter()
            .any(|note| note.starts_with("reconciled")));
        assert_eq!(
            outcome.warnings,
            vec![
                "channel 0 clip at 9000: trim window shorter than the clip; v20 shortens it on load"
                    .to_string()
            ]
        );
    }

    #[test]
    fn leaves_unstretched_channel_end_trims_alone() {
        let span = 0.5 * test_tick_ms();
        let mut playlist = Vec::new();
        for (i, len) in [96u32, 192, 384].into_iter().enumerate() {
            playlist.extend(trimmed_clip(i as u32 * 1000, len, 0.0, (len as f64 * span) as f32));
        }
        playlist.extend(trimmed_clip(9000, 192, 0.0, (194.0 * span) as f32));

        let source = source_flp_with_stretch(playlist.clone(), 0.0);
        let (blob, outcome) = converted_playlist(&source);
        for i in 0..4 {
            assert_eq!(&blob[i * 32 + 24..i * 32 + 32], &playlist[i * 80 + 24..i * 80 + 32]);
        }
        assert!(!outcome
            .notes
            .iter()
            .any(|note| note.starts_with("reconciled")));
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    }

    #[test]
    fn rebases_initialised_control_targets() {
        let span = 0.5 * test_tick_ms();
        let playlist = trimmed_clip(0, 192, 0.0, (192.0 * span) as f32);
        let mut d8 = vec![0u8; 24];
        d8[4] = 0;
        d8[6..8].copy_from_slice(&0x7001u16.to_le_bytes());
        d8[8..12].copy_from_slice(&12800i32.to_le_bytes());
        d8[16] = 1;
        d8[18..20].copy_from_slice(&0x2005u16.to_le_bytes());
        d8[20..24].copy_from_slice(&6400i32.to_le_bytes());

        let mut source = source_flp(playlist);
        source.events.push(Event { op: 0xD8, payload: Payload::Blob(d8) });
        let outcome = to_fl20(&source).unwrap();
        let blob = outcome
            .flp
            .events
            .iter()
            .find(|event| event.op == 0xD8)
            .and_then(Event::blob)
            .unwrap();

        assert_eq!(blob.len(), 24);
        assert_eq!(u16::from_le_bytes(blob[6..8].try_into().unwrap()), 0x2001);
        assert_eq!(u16::from_le_bytes(blob[18..20].try_into().unwrap()), 0x2005);
        assert!(outcome
            .notes
            .iter()
            .any(|note| note == "rebased 1 initialised control targets 0x7000 -> 0x2000"));
    }

    fn wrapper_flp(plugin: &str, marker: u32) -> Flp {
        let span = 0.5 * test_tick_ms();
        let mut source = source_flp(trimmed_clip(0, 192, 0.0, (192.0 * span) as f32));
        let mut name: Vec<u8> = plugin.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        name.extend_from_slice(&[0, 0]);
        let mut state = vec![0u8; 12];
        state[0..4].copy_from_slice(&marker.to_le_bytes());
        state[4..8].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        source.events.push(Event {
            op: op::PLUGIN_INTERNAL_NAME,
            payload: Payload::Blob(name),
        });
        source.events.push(Event { op: op::WRAPPER, payload: Payload::Blob(state) });
        source
    }

    fn converted_wrapper(plugin: &str, marker: u32) -> (Vec<u8>, Outcome) {
        let outcome = to_fl20(&wrapper_flp(plugin, marker)).unwrap();
        let blob = outcome
            .flp
            .events
            .iter()
            .find(|event| event.op == op::WRAPPER)
            .and_then(Event::blob)
            .unwrap()
            .to_vec();
        (blob, outcome)
    }

    #[test]
    fn keeps_native_plugin_wrapper_state_unchanged() {
        let (blob, outcome) = converted_wrapper("Fruity Love Philter", 786_435);
        assert_eq!(
            blob,
            wrapper_flp("Fruity Love Philter", 786_435)
                .events
                .iter()
                .find(|event| event.op == op::WRAPPER)
                .and_then(Event::blob)
                .unwrap()
        );
        assert!(!outcome.notes.iter().any(|note| note.contains("wrapper marker")));
    }

    #[test]
    fn rewrites_fruity_wrapper_state_version() {
        let (blob, outcome) = converted_wrapper("Fruity Wrapper", 12);
        assert_eq!(u32::from_le_bytes(blob[0..4].try_into().unwrap()), 10);
        assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 0x1234_5678);
        assert!(outcome
            .notes
            .iter()
            .any(|note| note == "wrapper marker 12 -> 10 on 1 plugins"));
    }

    #[test]
    fn uses_fl20_length_floor_for_v25_scale() {
        for (len, expected) in [(96, 97), (48, 48), (192, 194), (385, 389)] {
            let rec = v25_clip(len, 0.0, 0.0, 0.988_417_187_181_706);
            assert_eq!(fl20_clip_length(&rec, 80, &[]), (expected, true));
        }
    }
}
