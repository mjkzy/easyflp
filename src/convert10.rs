use crate::convert::{self, Outcome};
use crate::flp::{self, op, Event, Flp, Payload};
use crate::wrapper;

pub const FL10_VERSION: &str = "10.0.9";

/* every constant below is copied from a genuine 10.0.9 save (fl10.flp / fl10 v2 truth pair)
   unless the comment says otherwise. */
const FL10_REGISTRATION: [u8; 22] = [
    0x5B, 0x3A, 0x32, 0x44, 0x68, 0x39, 0x3C, 0x74, 0x33, 0x76, 0x38, 0x3F, 0x3E, 0x41, 0x42,
    0x41, 0x43, 0x3D, 0x3F, 0x3B, 0x3D, 0x00,
];

/* 10 stores project comments as RTF (0xC6). 20 stores plain UTF-16 (0xC3). */
const FL10_COMMENT_HEAD: &str =
    "{\\rtf1\\ansi\\ansicpg1252\\deff0\\deflang2060{\\fonttbl{\\f0\\fswiss Arial;}}\r\n\\viewkind4\\uc1\\pard\\f0\\fs16 ";
const FL10_COMMENT_TAIL: &str = "\r\n\\par }\r\n";

/* opcodes 20.8 writes that never appear in a 10.0.9 save of the same project. derived by
   byte-diffing the truth pair. 0x62 (slot close), 0x9C (tempo) and 0xC3 (comment) are handled
   structurally and are absent here on purpose. */
const POST_FL10_OPS: [u8; 26] = [
    0x1F, 0x20, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2F, 0x61, 0x63,
    0x64, 0x9D, 0x9E, 0x9F, 0xA4, 0xA5, 0xA6, 0xA7, 0xF1, 0xF2, 0xF3,
];

/* every opcode observed in a 10.0.9 save, plus 0x94/0xCD (playlist time markers, a 10-era
   feature) which the truth project does not use. leftovers outside this set are reported. */
const FL10_KNOWN_OPS: [u8; 82] = [
    0x00, 0x09, 0x0A, 0x0B, 0x11, 0x12, 0x14, 0x15, 0x16, 0x17, 0x1B, 0x1C, 0x1D, 0x40, 0x41,
    0x42, 0x43, 0x45, 0x46, 0x47, 0x4A, 0x4B, 0x4C, 0x50, 0x53, 0x55, 0x56, 0x59, 0x5D, 0x80,
    0x83, 0x84, 0x85, 0x8A, 0x8B, 0x8F, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x9A,
    0x9B, 0xC0, 0xC1, 0xC2, 0xC4, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF,
    0xD1, 0xD4, 0xD5, 0xD7, 0xD8, 0xDA, 0xDB, 0xDD, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE7,
    0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0x87,
];

const TEMPO_COARSE: u8 = 0x42;
const TEMPO_FINE: u8 = 0x5D;
const CHANNEL_NAME_FL10: u8 = 0xC0;
const COMMENT_FL10: u8 = 0xC6;
const COMMENT: u8 = 0xC3;
const INSERT_SLOT_COUNT: u8 = 0x1B;
const INSERT_NAME: u8 = 0xCC;
const INSERT_COLOUR: u8 = 0x95;
const PATTERN_COLOUR: u8 = 0x96;
const PATTERN_FL10_TAIL: u8 = 0x97;
const LANE_NAME: u8 = 0xEF;
const MARKER_NAME: u8 = 0xCD;

const FL10_INSERTS: u16 = 99;
const FL10_STRIPS: u16 = 105;
const FL10_CURRENT_STRIP: u16 = 104;
const FL20_CURRENT_STRIP: u16 = 126;
const FL10_SLOTS: usize = 8;

/* the Plugins\Fruity\Effects folder of a 10.0.9 install. an effect outside the list and outside
   LEGACY_EFFECTS loads as an empty slot in 10. */
const FL10_EFFECTS: [&str; 44] = [
    "Buzz Effect Adapter",
    "EQUO",
    "Edison",
    "Fruity Big Clock",
    "Fruity Convolver",
    "Fruity Delay 2",
    "Fruity Delay Bank",
    "Fruity Fast Dist",
    "Fruity Flangus",
    "Fruity Formula Controller",
    "Fruity HTML NoteBook",
    "Fruity LSD",
    "Fruity Limiter",
    "Fruity Love Philter",
    "Fruity Multiband Compressor",
    "Fruity NoteBook",
    "Fruity PanOMatic",
    "Fruity Parametric EQ",
    "Fruity Parametric EQ 2",
    "Fruity Peak Controller",
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
    "Fruity dB Meter",
    "Gross Beat",
    "Hardcore",
    "Maximus",
    "Newtone",
    "Patcher",
    "Pitcher",
    "Soundgoodizer",
    "SynthMaker",
    "Vocodex",
    "Wave Candy",
    "ZGameEditor Visualizer",
];

/* Gross Beat in 10 leads with a u32 state version 8; 26 leads with u16 3, u16 9 and one extra
   trailing byte. a 10 save of the converted state is the 26 body with that byte removed. */
const GROSS_BEAT_STATE_FL10: u32 = 8;
const GROSS_BEAT_HEAD_NEWER: [u8; 4] = [3, 0, 9, 0];

/* Fruity Convolver stores the impulse path as a length-prefixed string at offset 21. 26 writes
   the stock impulses under %FLStudioFactoryData%\Data\; 10 knows only %FLStudioData%\ and
   falls back to Default.wav when the path does not resolve. 10 ships a fraction of the newer
   impulse library, so the converter points the path at the file of a newer install on this
   machine when one has it. */
const CONVOLVER_PATH_OFFSET: usize = 21;
const IMPULSE_ROOT_NEWER: &[u8] = br"%FLStudioFactoryData%\Data\";
const IMPULSE_ROOT_FL10: &[u8] = br"%FLStudioData%\";
const IMAGE_LINE_DIRS: [&str; 2] = [r"C:\Program Files\Image-Line", r"C:\Program Files (x86)\Image-Line"];
const FL10_D7_LEN: usize = 112;
const FL10_LANE_LEN: usize = 22;
const LANE_SHIFT: i32 = 401;
const STRETCH_MODE_OFFSET: usize = 108;

/* 10 hosts its legacy Fruity effects as VST DLLs inside Fruity Wrapper. 20 hosts the same
   effects natively with a small int-parameter state. the wrapper states below are the 10.0.9
   defaults for each effect; chunk 53 holds the DLL's own f32 parameter list and is patched
   with the values mapped from the native state. */
struct LegacyEffect {
    name: &'static str,
    state: &'static [u8],
    /* per native parameter index (after the leading state version word, if any): the divisor
       that maps the native integer onto the VST 0..1 float. None = not mappable. */
    map: Option<&'static [Option<f32>]>,
    has_version_word: bool,
}

const LEGACY_EFFECTS: [LegacyEffect; 5] = [
    LegacyEffect {
        name: "Fruity Blood Overdrive",
        state: &FRUITY_BLOOD_OVERDRIVE,
        map: None,
        has_version_word: true,
    },
    LegacyEffect {
        name: "Fruity Reeverb",
        state: &FRUITY_REEVERB,
        map: Some(&[
            Some(65536.0),
            Some(65536.0),
            Some(65536.0),
            Some(65536.0),
            Some(65536.0),
            Some(40.0),
            Some(65536.0),
            Some(65536.0),
            Some(65536.0),
            Some(65536.0),
        ]),
        has_version_word: true,
    },
    LegacyEffect {
        name: "Fruity Phaser",
        state: &FRUITY_PHASER,
        map: Some(&[
            Some(5000.0),
            Some(1000.0),
            Some(1000.0),
            Some(1000.0),
            Some(1024.0),
            Some(23.0),
            Some(1000.0),
            Some(1024.0),
            Some(5000.0),
        ]),
        has_version_word: true,
    },
    LegacyEffect {
        name: "Fruity Chorus",
        state: &FRUITY_CHORUS,
        map: Some(&[
            Some(1024.0),
            Some(5000.0),
            Some(1024.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
            Some(5000.0),
        ]),
        has_version_word: true,
    },
    LegacyEffect {
        name: "Fruity Balance",
        state: &FRUITY_BALANCE,
        map: Some(&[None, Some(320.0)]),
        has_version_word: false,
    },
];

const FRUITY_BLOOD_OVERDRIVE: [u8; 400] = [
    0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0xE2, 0x01, 0x00, 0x00, 0x5B, 0x00, 0x00, 0x00, 0xA0, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
    0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x33, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x44, 0x4F, 0x4C,
    0x42, 0x36, 0x00, 0x00, 0x00, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, 0x72, 0x75,
    0x69, 0x74, 0x79, 0x20, 0x42, 0x6C, 0x6F, 0x6F, 0x64, 0x20, 0x4F, 0x76, 0x65, 0x72, 0x64, 0x72,
    0x69, 0x76, 0x65, 0x37, 0x00, 0x00, 0x00, 0x4F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43,
    0x3A, 0x5C, 0x50, 0x72, 0x6F, 0x67, 0x72, 0x61, 0x6D, 0x20, 0x46, 0x69, 0x6C, 0x65, 0x73, 0x5C,
    0x49, 0x6D, 0x61, 0x67, 0x65, 0x2D, 0x4C, 0x69, 0x6E, 0x65, 0x5C, 0x46, 0x4C, 0x20, 0x53, 0x74,
    0x75, 0x64, 0x69, 0x6F, 0x20, 0x31, 0x30, 0x5C, 0x50, 0x6C, 0x75, 0x67, 0x69, 0x6E, 0x73, 0x5C,
    0x56, 0x53, 0x54, 0x5C, 0x46, 0x72, 0x75, 0x69, 0x74, 0x79, 0x20, 0x42, 0x6C, 0x6F, 0x6F, 0x64,
    0x20, 0x4F, 0x76, 0x65, 0x72, 0x64, 0x72, 0x69, 0x76, 0x65, 0x2E, 0x64, 0x6C, 0x6C, 0x35, 0x00,
    0x00, 0x00, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0xFF, 0xFF, 0xFF, 0x05, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x3F, 0x00, 0x00, 0x00, 0x00, 0xC3, 0xF5, 0xA8, 0x3E, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x80, 0x3F, 0x01, 0x00, 0x00, 0x00, 0x52, 0x65, 0x73, 0x65, 0x74, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const FRUITY_REEVERB: [u8; 400] = [
    0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x41, 0x01, 0x00, 0x00, 0xE5, 0x00, 0x00, 0x00, 0x80, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
    0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x33, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x65, 0x52, 0x66,
    0x55, 0x36, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, 0x72, 0x75,
    0x69, 0x74, 0x79, 0x20, 0x52, 0x65, 0x65, 0x76, 0x65, 0x72, 0x62, 0x37, 0x00, 0x00, 0x00, 0x47,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x3A, 0x5C, 0x50, 0x72, 0x6F, 0x67, 0x72, 0x61,
    0x6D, 0x20, 0x46, 0x69, 0x6C, 0x65, 0x73, 0x5C, 0x49, 0x6D, 0x61, 0x67, 0x65, 0x2D, 0x4C, 0x69,
    0x6E, 0x65, 0x5C, 0x46, 0x4C, 0x20, 0x53, 0x74, 0x75, 0x64, 0x69, 0x6F, 0x20, 0x31, 0x30, 0x5C,
    0x50, 0x6C, 0x75, 0x67, 0x69, 0x6E, 0x73, 0x5C, 0x56, 0x53, 0x54, 0x5C, 0x46, 0x72, 0x75, 0x69,
    0x74, 0x79, 0x20, 0x52, 0x65, 0x65, 0x76, 0x65, 0x72, 0x62, 0x2E, 0x64, 0x6C, 0x6C, 0x35, 0x00,
    0x00, 0x00, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0xFF, 0xFF, 0xFF, 0x05, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x19, 0xE5, 0x99, 0x3C, 0x79,
    0xE9, 0x26, 0x3E, 0x00, 0x00, 0x00, 0x00, 0xF2, 0xD2, 0xCD, 0x3E, 0x00, 0x00, 0x80, 0x3F, 0x00,
    0x00, 0x00, 0x3F, 0xE0, 0x2D, 0x90, 0x3D, 0xE7, 0xFB, 0x29, 0x3E, 0x00, 0x00, 0x80, 0x3F, 0x17,
    0xD9, 0x4E, 0x3E, 0x01, 0x00, 0x00, 0x00, 0x5B, 0x31, 0x5D, 0x20, 0x44, 0x65, 0x66, 0x61, 0x75,
    0x6C, 0x74, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const FRUITY_PHASER: [u8; 394] = [
    0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0xE2, 0x01, 0x00, 0x00, 0x89, 0x00, 0x00, 0x00, 0x80, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
    0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x33, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78, 0x34, 0x31,
    0x33, 0x36, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, 0x72, 0x75,
    0x69, 0x74, 0x79, 0x20, 0x50, 0x68, 0x61, 0x73, 0x65, 0x72, 0x37, 0x00, 0x00, 0x00, 0x46, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x3A, 0x5C, 0x50, 0x72, 0x6F, 0x67, 0x72, 0x61, 0x6D,
    0x20, 0x46, 0x69, 0x6C, 0x65, 0x73, 0x5C, 0x49, 0x6D, 0x61, 0x67, 0x65, 0x2D, 0x4C, 0x69, 0x6E,
    0x65, 0x5C, 0x46, 0x4C, 0x20, 0x53, 0x74, 0x75, 0x64, 0x69, 0x6F, 0x20, 0x31, 0x30, 0x5C, 0x50,
    0x6C, 0x75, 0x67, 0x69, 0x6E, 0x73, 0x5C, 0x56, 0x53, 0x54, 0x5C, 0x46, 0x72, 0x75, 0x69, 0x74,
    0x79, 0x20, 0x50, 0x68, 0x61, 0x73, 0x65, 0x72, 0x2E, 0x64, 0x6C, 0x6C, 0x35, 0x00, 0x00, 0x00,
    0x52, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0xFF, 0xFF, 0xFF, 0x05, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0xCD, 0xCC, 0xCC,
    0x3D, 0xCD, 0xCC, 0x4C, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0x43, 0x16, 0xB2,
    0x3E, 0xCD, 0xCC, 0xCC, 0x3E, 0x00, 0x00, 0x00, 0x3F, 0xCD, 0xCC, 0x4C, 0x3F, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const FRUITY_CHORUS: [u8; 406] = [
    0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0xC8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xA0, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
    0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x33, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x55, 0x52,
    0x46, 0x36, 0x00, 0x00, 0x00, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, 0x72, 0x75,
    0x69, 0x74, 0x79, 0x20, 0x43, 0x68, 0x6F, 0x72, 0x75, 0x73, 0x37, 0x00, 0x00, 0x00, 0x46, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x3A, 0x5C, 0x50, 0x72, 0x6F, 0x67, 0x72, 0x61, 0x6D,
    0x20, 0x46, 0x69, 0x6C, 0x65, 0x73, 0x5C, 0x49, 0x6D, 0x61, 0x67, 0x65, 0x2D, 0x4C, 0x69, 0x6E,
    0x65, 0x5C, 0x46, 0x4C, 0x20, 0x53, 0x74, 0x75, 0x64, 0x69, 0x6F, 0x20, 0x31, 0x30, 0x5C, 0x50,
    0x6C, 0x75, 0x67, 0x69, 0x6E, 0x73, 0x5C, 0x56, 0x53, 0x54, 0x5C, 0x46, 0x72, 0x75, 0x69, 0x74,
    0x79, 0x20, 0x43, 0x68, 0x6F, 0x72, 0x75, 0x73, 0x2E, 0x64, 0x6C, 0x6C, 0x35, 0x00, 0x00, 0x00,
    0x5E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0xFF, 0xFF, 0xFF, 0x05, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0x66, 0x66, 0xE6,
    0x3E, 0xFA, 0x7E, 0xAA, 0x3E, 0x9A, 0x99, 0x99, 0x3E, 0x00, 0x00, 0x00, 0x3F, 0x33, 0x33, 0x33,
    0x3F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x3F, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const FRUITY_BALANCE: [u8; 368] = [
    0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x41, 0x01, 0x00, 0x00, 0x2D, 0x00, 0x00, 0x00, 0x80, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
    0x00, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x33, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6C, 0x61, 0x42,
    0x46, 0x36, 0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46, 0x72, 0x75,
    0x69, 0x74, 0x79, 0x20, 0x42, 0x61, 0x6C, 0x61, 0x6E, 0x63, 0x65, 0x37, 0x00, 0x00, 0x00, 0x47,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x3A, 0x5C, 0x50, 0x72, 0x6F, 0x67, 0x72, 0x61,
    0x6D, 0x20, 0x46, 0x69, 0x6C, 0x65, 0x73, 0x5C, 0x49, 0x6D, 0x61, 0x67, 0x65, 0x2D, 0x4C, 0x69,
    0x6E, 0x65, 0x5C, 0x46, 0x4C, 0x20, 0x53, 0x74, 0x75, 0x64, 0x69, 0x6F, 0x20, 0x31, 0x30, 0x5C,
    0x50, 0x6C, 0x75, 0x67, 0x69, 0x6E, 0x73, 0x5C, 0x56, 0x53, 0x54, 0x5C, 0x46, 0x72, 0x75, 0x69,
    0x74, 0x79, 0x20, 0x42, 0x61, 0x6C, 0x61, 0x6E, 0x63, 0x65, 0x2E, 0x64, 0x6C, 0x6C, 0x35, 0x00,
    0x00, 0x00, 0x36, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0xFF, 0xFF, 0xFF, 0x05, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3F, 0xCD,
    0xCC, 0x4C, 0x3F, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

struct Counts {
    deleted: usize,
    strings: usize,
    lossy_chars: usize,
    channels: usize,
    d7: usize,
    stretch_unmapped: Vec<u32>,
    routes_lost: usize,
    clips: usize,
    clips_clamped: usize,
    lanes: usize,
    lanes_dropped: usize,
    lane_names_lost: usize,
    inserts_dropped: usize,
    inserts_dropped_used: usize,
    slots_dropped: usize,
    bus_routes_lost: usize,
    wrappers: usize,
    native_states: usize,
    legacy_mapped: Vec<String>,
    legacy_reset: Vec<String>,
    unverified_plugins: Vec<String>,
    effects_missing: Vec<String>,
    impulses_resolved: Vec<String>,
    impulses_missing: Vec<String>,
    insert_flags_lost: usize,
    params_lost: usize,
    sends_lost: usize,
    links_dropped: usize,
    controls_dropped: usize,
    markers_dropped: usize,
    marker_nondefault: bool,
    tempo_fraction: bool,
    patterns: usize,
    sample_paths_rewritten: usize,
}

impl Counts {
    fn new() -> Self {
        Counts {
            deleted: 0,
            strings: 0,
            lossy_chars: 0,
            channels: 0,
            d7: 0,
            stretch_unmapped: Vec::new(),
            routes_lost: 0,
            clips: 0,
            clips_clamped: 0,
            lanes: 0,
            lanes_dropped: 0,
            lane_names_lost: 0,
            inserts_dropped: 0,
            inserts_dropped_used: 0,
            slots_dropped: 0,
            bus_routes_lost: 0,
            wrappers: 0,
            native_states: 0,
            legacy_mapped: Vec::new(),
            legacy_reset: Vec::new(),
            unverified_plugins: Vec::new(),
            effects_missing: Vec::new(),
            impulses_resolved: Vec::new(),
            impulses_missing: Vec::new(),
            insert_flags_lost: 0,
            params_lost: 0,
            sends_lost: 0,
            links_dropped: 0,
            controls_dropped: 0,
            markers_dropped: 0,
            marker_nondefault: false,
            tempo_fraction: false,
            patterns: 0,
            sample_paths_rewritten: 0,
        }
    }
}

fn u8e(o: u8, v: u8) -> Event {
    Event { op: o, payload: Payload::U8(v) }
}
fn u16e(o: u8, v: u16) -> Event {
    Event { op: o, payload: Payload::U16(v) }
}
fn u32e(o: u8, v: u32) -> Event {
    Event { op: o, payload: Payload::U32(v) }
}
fn blob(o: u8, b: Vec<u8>) -> Event {
    Event { op: o, payload: Payload::Blob(b) }
}

/* 10 reads every text event as a NUL-terminated ANSI string. code points above U+00FF have no
   representation and become '?'. */
fn ansiz(s: &str, c: &mut Counts) -> Vec<u8> {
    c.strings += 1;
    let mut out = Vec::with_capacity(s.len() + 1);
    for ch in s.chars() {
        let cp = ch as u32;
        if cp <= 0xFF {
            out.push(cp as u8);
        } else {
            c.lossy_chars += 1;
            out.push(b'?');
        }
    }
    out.push(0);
    out
}

fn text_event(ev: &Event, new_op: u8, c: &mut Counts) -> Event {
    let s = ev.blob().map(flp::utf16z).unwrap_or_default();
    blob(new_op, ansiz(&s, c))
}

fn comment_event(text: &str, c: &mut Counts) -> Event {
    let mut body = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => body.push_str("\\\\"),
            '{' => body.push_str("\\{"),
            '}' => body.push_str("\\}"),
            '\r' => {}
            '\n' => body.push_str("\\par\r\n"),
            _ => body.push(ch),
        }
    }
    let rtf = format!("{FL10_COMMENT_HEAD}{body}{FL10_COMMENT_TAIL}");
    blob(COMMENT_FL10, ansiz(&rtf, c))
}

/* 10.0.9 writes wrapper state version 7 and no chunk 56 (vendor name). chunk 2 bytes 12 and 17
   are zero in every 10 save and 0xA0 / 1 in every 20+ save of the same plugin. */
fn wrapper_v7(b: &[u8]) -> Option<Vec<u8>> {
    let (_, chunks) = wrapper::parse_state(b)?;
    let mut kept: Vec<wrapper::Chunk> = Vec::with_capacity(chunks.len());
    for mut c in chunks {
        if c.id == 56 {
            continue;
        }
        if c.id == 2 && c.data.len() >= 18 {
            c.data[12] = 0;
            c.data[17] = 0;
        }
        kept.push(c);
    }
    Some(wrapper::build_state(7, &kept))
}

/* returns the 10 wrapper state for a legacy effect. the native 20 state is a list of i32
   (optionally led by a version word); each maps onto one f32 of the DLL's parameter list in
   chunk 53 (13-byte head, u32 count, f32 * count). */
fn legacy_state(fx: &LegacyEffect, native: &[u8], c: &mut Counts) -> Vec<u8> {
    let mut state = fx.state.to_vec();
    let Some(map) = fx.map else {
        c.legacy_reset.push(fx.name.to_string());
        return state;
    };
    let ints: Vec<i32> = native
        .chunks_exact(4)
        .map(|w| i32::from_le_bytes(w.try_into().unwrap()))
        .collect();
    let params = if fx.has_version_word { &ints[1.min(ints.len())..] } else { &ints[..] };
    let Some((off, len)) = wrapper::chunk_offset(&state, 53) else {
        c.legacy_reset.push(fx.name.to_string());
        return state;
    };
    if len < 17 {
        c.legacy_reset.push(fx.name.to_string());
        return state;
    }
    let count = u32::from_le_bytes(state[off + 13..off + 17].try_into().unwrap()) as usize;
    if params.len() != map.len() || count != map.len() || len < 17 + 4 * count {
        c.legacy_reset.push(fx.name.to_string());
        return state;
    }
    for (i, (&v, &div)) in params.iter().zip(map.iter()).enumerate() {
        let f = match (fx.name, i, div) {
            ("Fruity Balance", 0, _) => (v as f32 + 128.0) / 256.0,
            (_, _, Some(d)) => v as f32 / d,
            _ => continue,
        };
        let f = f.clamp(0.0, 1.0);
        let at = off + 17 + 4 * i;
        state[at..at + 4].copy_from_slice(&f.to_le_bytes());
    }
    c.legacy_mapped.push(fx.name.to_string());
    state
}

/* native effect states whose 10 <-> 20 relation is known from the truth pair: same layout
   and a different leading version word. Limiter and Parametric EQ 2 append their new fields
   after the 10 length; Reeverb 2 inserts 8 bytes at offset 56 ahead of its 2-byte tail. */
fn native_state_fl10(name: &str, b: &[u8], c: &mut Counts) -> Option<Vec<u8>> {
    /* 24 writes Parametric EQ 2 state 8 at the same 354-byte length as 7; it is treated as 7 */
    let (from, to, len10, insert_at): (&[u32], u32, usize, Option<usize>) = match name {
        "Fruity Limiter" => (&[7, 6], 6, 168, None),
        "Fruity Parametric EQ 2" => (&[7, 8, 2], 2, 305, None),
        "Fruity Reeverb 2" => (&[0x2711, 0x2710], 0x2710, 58, Some(56)),
        "Fruity Delay 2" | "Fruity Soft Clipper" => return Some(b.to_vec()),
        "Gross Beat" => return gross_beat_fl10(b, c),
        "Fruity Convolver" => return convolver_fl10(b, c),
        _ => return None,
    };
    if b.len() < len10 || b.len() < 4 {
        return None;
    }
    let marker = u32::from_le_bytes(b[0..4].try_into().unwrap());
    if !from.contains(&marker) {
        return None;
    }
    let mut nb = match insert_at {
        Some(at) if b.len() > len10 => {
            let mut v = b[..at].to_vec();
            v.extend_from_slice(&b[b.len() - (len10 - at)..]);
            v
        }
        _ => b[..len10].to_vec(),
    };
    nb[0..4].copy_from_slice(&to.to_le_bytes());
    c.native_states += 1;
    Some(nb)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn gross_beat_fl10(b: &[u8], c: &mut Counts) -> Option<Vec<u8>> {
    if b.len() < 5 || b[..4] != GROSS_BEAT_HEAD_NEWER {
        return None;
    }
    let mut nb = b[..b.len() - 1].to_vec();
    nb[0..4].copy_from_slice(&GROSS_BEAT_STATE_FL10.to_le_bytes());
    c.native_states += 1;
    Some(nb)
}

/* newest install first, so a 26 file wins over a 20 copy of the same name */
fn installed_factory_file(rest: &str) -> Option<String> {
    let mut installs: Vec<std::path::PathBuf> = IMAGE_LINE_DIRS
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("FL Studio")))
        .collect();
    let release = |p: &std::path::Path| -> u32 {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.trim_start_matches("FL Studio").trim().parse().ok())
            .unwrap_or(0)
    };
    installs.sort_by_key(|p| release(p));
    installs
        .into_iter()
        .rev()
        .map(|p| p.join("Data").join(rest))
        .find(|p| p.is_file())
        .and_then(|p| p.to_str().map(str::to_owned))
}

fn convolver_fl10(b: &[u8], c: &mut Counts) -> Option<Vec<u8>> {
    let len = usize::from(*b.get(CONVOLVER_PATH_OFFSET)?);
    let start = CONVOLVER_PATH_OFFSET + 1;
    let path = b.get(start..start + len)?;
    if !path.starts_with(IMPULSE_ROOT_NEWER) {
        return None;
    }
    let rest = String::from_utf8_lossy(&path[IMPULSE_ROOT_NEWER.len()..]).into_owned();
    let new_path: Vec<u8> = match installed_factory_file(&rest) {
        Some(abs) if abs.is_ascii() && abs.len() <= u8::MAX as usize => {
            c.impulses_resolved.push(abs.clone());
            abs.into_bytes()
        }
        _ => {
            c.impulses_missing.push(rest.clone());
            [IMPULSE_ROOT_FL10, rest.as_bytes()].concat()
        }
    };
    let mut nb = b[..CONVOLVER_PATH_OFFSET].to_vec();
    nb.push(u8::try_from(new_path.len()).ok()?);
    nb.extend_from_slice(&new_path);
    nb.extend_from_slice(&b[start + len..]);
    Some(nb)
}

struct Plugin {
    internal: String,
    display: Option<String>,
    d4: Option<Vec<u8>>,
    d5: Option<Vec<u8>>,
}

/* rewrites one plugin (a generator channel's or a mixer slot's) into its 10 form. mixer slots
   carry the display name as 0xCB between 0xD4 and 0xD5; generator channels carry it as 0xC0 at
   the end of the channel block, so the caller keeps it. */
fn plugin_events(p: Plugin, mixer: bool, c: &mut Counts) -> Vec<Event> {
    let mut out = Vec::with_capacity(4);
    let mut internal = p.internal.clone();
    let mut display = p.display.clone();
    let mut d4 = p.d4.unwrap_or_default();
    let mut d5 = p.d5;

    if let Some(fx) = LEGACY_EFFECTS.iter().find(|fx| fx.name == p.internal) {
        internal = "Fruity Wrapper".into();
        display = Some(fx.name.to_string());
        d5 = Some(legacy_state(fx, d5.as_deref().unwrap_or(&[]), c));
        if d4.len() > 17 {
            d4[17] = 0;
        }
    } else if internal == "Fruity Wrapper" {
        if let Some(b) = d5.as_deref() {
            match wrapper_v7(b) {
                Some(nb) => {
                    d5 = Some(nb);
                    c.wrappers += 1;
                }
                None => c.unverified_plugins.push(display.clone().unwrap_or_else(|| internal.clone())),
            }
        }
    } else if let Some(b) = d5.as_deref() {
        match native_state_fl10(&internal, b, c) {
            Some(nb) => d5 = Some(nb),
            None => {
                if !c.unverified_plugins.contains(&internal) {
                    c.unverified_plugins.push(internal.clone());
                }
            }
        }
    }

    out.push(blob(op::PLUGIN_INTERNAL_NAME, ansiz(&internal, c)));
    if !d4.is_empty() {
        out.push(blob(0xD4, d4));
    }
    if mixer {
        if let Some(name) = display.filter(|n| !n.is_empty()) {
            out.push(blob(op::NAME, ansiz(&name, c)));
        }
    }
    if let Some(b) = d5 {
        out.push(blob(op::WRAPPER, b));
    }
    out
}

fn stretch_mode_fl10(mode: u32) -> Option<u32> {
    match mode {
        0 => Some(0),
        6 => Some(1),
        _ => None,
    }
}

/* one generator/sampler/automation channel block. 20 order:
     40 15 C9 D4 CB 9B 80 [D5] 00 D1 ... 16 ... D7 ... [EA] 20 E4 E4 DA*5 8F 14 [C4]
   10 order:
     40 15 [C9 D4 D5] 00 D1 ... 16 ... D7 ... [EA] E4 E4 DA*5 8F 14 [C4] C0 9B 80 */
fn channel_block(block: &[Event], c: &mut Counts, warnings: &mut Vec<String>) -> Vec<Event> {
    c.channels += 1;
    let kind = block
        .iter()
        .find(|e| e.op == op::CHANNEL_KIND)
        .and_then(Event::value)
        .unwrap_or(0);
    let index = block.first().and_then(Event::value).unwrap_or(0);
    let mut plugin = Plugin { internal: String::new(), display: None, d4: None, d5: None };
    let mut tail: Vec<Event> = Vec::new();
    let mut body: Vec<Event> = Vec::with_capacity(block.len());

    for ev in block {
        match ev.op {
            op::PLUGIN_INTERNAL_NAME => plugin.internal = ev.blob().map(flp::utf16z).unwrap_or_default(),
            0xD4 => plugin.d4 = ev.blob().map(<[u8]>::to_vec),
            op::WRAPPER => plugin.d5 = ev.blob().map(<[u8]>::to_vec),
            op::NAME => plugin.display = ev.blob().map(flp::utf16z),
            0x9B | 0x80 => tail.push(ev.clone()),
            0x20 | 0x61 | 0x29 => c.deleted += 1,
            o if POST_FL10_OPS.contains(&o) => c.deleted += 1,
            op::CHANNEL_ROUTE => {
                let v = ev.value().unwrap_or(0);
                if v > u32::from(FL10_INSERTS) {
                    c.routes_lost += 1;
                    warnings.push(format!(
                        "channel {index} is routed to insert {v}; FL 10 has 99 inserts, so the route now goes to the master"
                    ));
                    body.push(u8e(op::CHANNEL_ROUTE, 0));
                } else {
                    body.push(ev.clone());
                }
            }
            op::CHANNEL_DECO => {
                let b = ev.blob().unwrap_or(&[]);
                let mut nb = b[..b.len().min(FL10_D7_LEN)].to_vec();
                if nb.len() >= FL10_D7_LEN {
                    c.d7 += 1;
                    let mode = u32::from_le_bytes(
                        nb[STRETCH_MODE_OFFSET..STRETCH_MODE_OFFSET + 4].try_into().unwrap(),
                    );
                    let mapped = stretch_mode_fl10(mode).unwrap_or_else(|| {
                        if !c.stretch_unmapped.contains(&mode) {
                            c.stretch_unmapped.push(mode);
                        }
                        1
                    });
                    nb[STRETCH_MODE_OFFSET..STRETCH_MODE_OFFSET + 4]
                        .copy_from_slice(&mapped.to_le_bytes());
                }
                body.push(blob(op::CHANNEL_DECO, nb));
            }
            op::SAMPLE_PATH => {
                let mut path = ev.blob().map(flp::utf16z).unwrap_or_default();
                const FACTORY: &str = "%FLStudioFactoryData%\\Data\\";
                if path.starts_with(FACTORY) {
                    path = format!("%FLStudioData%\\{}", &path[FACTORY.len()..]);
                    c.sample_paths_rewritten += 1;
                    warnings.push(format!(
                        "channel {index}: factory sample path rewritten to the FL 10 data folder; the pack layout differs between versions, so the file may need relocating"
                    ));
                }
                body.push(blob(op::SAMPLE_PATH, ansiz(&path, c)));
            }
            _ => body.push(ev.clone()),
        }
    }

    let mut out = Vec::with_capacity(body.len() + 6);
    let mut rest = body.into_iter();
    /* 40 and 15 always lead the block */
    for _ in 0..2 {
        if let Some(e) = rest.next() {
            out.push(e);
        }
    }
    let name = plugin.display.clone().unwrap_or_default();
    if kind == 2 {
        out.extend(plugin_events(plugin, false, c));
    } else if !plugin.internal.is_empty() {
        c.unverified_plugins.push(plugin.internal.clone());
        out.extend(plugin_events(plugin, false, c));
    }
    out.extend(rest);
    out.push(blob(CHANNEL_NAME_FL10, ansiz(&name, c)));
    out.extend(tail);
    out
}

/* 10 mixer: 105 strips (master, 99 inserts, 4 send buses, current). 20: 127 (master, 125
   inserts, current). inserts 100..125 have no home and are dropped; 126 becomes 104. */
fn fl10_strip(strip20: u16) -> Option<u16> {
    if strip20 <= FL10_INSERTS {
        Some(strip20)
    } else if strip20 == FL20_CURRENT_STRIP {
        Some(FL10_CURRENT_STRIP)
    } else {
        None
    }
}

fn route_table_fl10(strip10: u16, b: &[u8]) -> Vec<u8> {
    let mut nb = vec![0u8; usize::from(FL10_STRIPS)];
    let n = b.len().min(100);
    nb[..n].copy_from_slice(&b[..n]);
    if (1..=FL10_INSERTS).contains(&strip10) {
        for v in &mut nb[100..104] {
            *v = 1;
        }
    }
    nb
}

fn send_bus_block(strip10: u16) -> Vec<Event> {
    let mut eb = vec![0u8; usize::from(FL10_STRIPS)];
    if strip10 != FL10_CURRENT_STRIP {
        eb[0] = 1;
    }
    vec![
        u8e(INSERT_SLOT_COUNT, 12),
        blob(op::INSERT_FLAGS, vec![0, 0, 0, 0, 1]),
        blob(op::ROUTE_TABLE, eb),
        u32e(0x9A, 0xFFFF_FFFF),
        u32e(0x93, 0xFFFF_FFFF),
    ]
}

struct InsertState {
    used: Vec<bool>,
}

/* one insert block. 20 order: [95] [2A] [CC] EC (C9 D4 [CB] 9B 80 D5 62 | 62)*10 EB 9A 93
   10 order:                   [95] [CC] 1B EC (C9 D4 [CB] D5)* EB 9A 93 */
fn insert_block(
    strip20: u16,
    block: &[Event],
    insert_state: &InsertState,
    c: &mut Counts,
    warnings: &mut Vec<String>,
) -> Vec<Event> {
    let Some(strip10) = fl10_strip(strip20) else {
        c.inserts_dropped += 1;
        if insert_state.used[usize::from(strip20)] {
            c.inserts_dropped_used += 1;
            warnings.push(format!(
                "insert {strip20} carries plugins or a name; FL 10 stops at insert 99, so it is dropped"
            ));
        }
        return Vec::new();
    };

    let mut out = Vec::with_capacity(block.len());
    let mut slot = 0usize;
    let mut plugin: Option<Plugin> = None;

    for ev in block {
        match ev.op {
            INSERT_COLOUR => out.push(ev.clone()),
            INSERT_NAME => out.push(text_event(ev, INSERT_NAME, c)),
            op::INSERT_FLAGS => {
                let b = ev.blob().unwrap_or(&[]);
                if b.len() >= 8 {
                    let flags = u32::from_le_bytes(b[4..8].try_into().unwrap());
                    if flags & 0x08 == 0 {
                        c.insert_flags_lost += 1;
                        warnings.push(format!(
                            "insert {strip20} is disabled (muted) in the source; FL 10 stores no such flag and loads it enabled"
                        ));
                    }
                }
                out.push(u8e(INSERT_SLOT_COUNT, 12));
                out.push(blob(op::INSERT_FLAGS, vec![0, 0, 0, 0, 1]));
            }
            op::PLUGIN_INTERNAL_NAME => {
                let name = ev.blob().map(flp::utf16z).unwrap_or_default();
                plugin = Some(Plugin { internal: name, display: None, d4: None, d5: None });
            }
            0xD4 => {
                if let Some(p) = plugin.as_mut() {
                    p.d4 = ev.blob().map(<[u8]>::to_vec);
                }
            }
            op::NAME => {
                if let Some(p) = plugin.as_mut() {
                    p.display = ev.blob().map(flp::utf16z);
                }
            }
            op::WRAPPER => {
                if let Some(p) = plugin.as_mut() {
                    p.d5 = ev.blob().map(<[u8]>::to_vec);
                }
            }
            0x9B | 0x80 => {}
            op::SLOT_CLOSE => {
                if let Some(p) = plugin.take() {
                    let shipped = FL10_EFFECTS.iter().any(|n| n.eq_ignore_ascii_case(&p.internal))
                        || LEGACY_EFFECTS.iter().any(|fx| fx.name == p.internal);
                    if !shipped && !c.effects_missing.contains(&p.internal) {
                        c.effects_missing.push(p.internal.clone());
                    }
                    if slot < FL10_SLOTS {
                        out.extend(plugin_events(p, true, c));
                    } else {
                        c.slots_dropped += 1;
                        warnings.push(format!(
                            "insert {strip20} slot {}: \"{}\" dropped; FL 10 has 8 effect slots",
                            slot + 1,
                            p.display.filter(|n| !n.is_empty()).unwrap_or(p.internal)
                        ));
                    }
                }
                slot += 1;
            }
            op::ROUTE_TABLE => {
                let b = ev.blob().unwrap_or(&[]);
                for (target, &on) in b.iter().enumerate().skip(100) {
                    if on != 0
                        && target != usize::from(FL20_CURRENT_STRIP)
                        && insert_state.used.get(target).copied().unwrap_or(false)
                    {
                        c.bus_routes_lost += 1;
                        warnings.push(format!(
                            "insert {strip20} routes to insert {target}, which FL 10 does not have; route dropped"
                        ));
                    }
                }
                out.push(blob(op::ROUTE_TABLE, route_table_fl10(strip10, b)));
            }
            o if POST_FL10_OPS.contains(&o) => c.deleted += 1,
            _ => out.push(ev.clone()),
        }
    }
    out
}

/* 10 writes 2941 records: the 0x4000 header, then per strip eight slot pairs (enable, mix)
   and the 12-pid run. no send levels (164..168) and no pid 190. */
fn mixer_params_fl10(b: &[u8], c: &mut Counts, warnings: &mut Vec<String>) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut existing: BTreeMap<(u16, u16, u8), i32> = BTreeMap::new();
    for rec in b.chunks_exact(12) {
        let pid = rec[4];
        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
        let val = i32::from_le_bytes(rec[8..12].try_into().unwrap());
        if !(0x2000..0x2000 + 127 * 0x40).contains(&tgt) {
            continue;
        }
        let strip = (tgt - 0x2000) >> 6;
        let off = (tgt - 0x2000) & 0x3F;
        existing.insert((strip, off, pid), val);
    }

    let mut out = Vec::with_capacity(1 + usize::from(FL10_STRIPS) * 28);
    push_param(0, 0x00, 0x4000, 12800, &mut out);
    let defaults = STRIP_PID_DEFAULTS;

    for strip10 in 0..FL10_STRIPS {
        let source: Option<u16> = if strip10 <= FL10_INSERTS {
            Some(strip10)
        } else if strip10 == FL10_CURRENT_STRIP {
            Some(FL20_CURRENT_STRIP)
        } else {
            None
        };
        push_strip_fl10(&existing, source, strip10, &mut out);
    }

    /* what did not make it: strips 100..125, slots 8/9, send levels */
    for (&(strip, off, pid), &val) in &existing {
        let default = match (off, pid) {
            (_, 0) if off < 10 => 1,
            (_, 1) if off < 10 => 12800,
            (0, p) => defaults.iter().find(|(d, _)| *d == p).map(|(_, v)| *v).unwrap_or(0),
            _ => 0,
        };
        if val == default {
            continue;
        }
        if fl10_strip(strip).is_none() {
            c.params_lost += 1;
        } else if pid <= 1 && off >= 8 {
            c.params_lost += 1;
            warnings.push(format!(
                "insert {strip} slot {} mix/enable value lost (FL 10 has 8 slots)",
                off + 1
            ));
        } else if matches!(pid, 164..=168 | 190) {
            c.sends_lost += 1;
            warnings.push(format!(
                "insert {strip} send level (pid {pid}) is not stored by FL 10; value lost"
            ));
        }
    }
    out
}

const STRIP_PID_DEFAULTS: [(u8, i32); 12] = [
    (192, 12800),
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

fn push_param(pid: u8, group: u8, tgt: u16, val: i32, out: &mut Vec<u8>) {
    out.extend_from_slice(&[0, 0, 0, 0, pid, group]);
    out.extend_from_slice(&tgt.to_le_bytes());
    out.extend_from_slice(&val.to_le_bytes());
}

fn push_strip_fl10(
    existing: &std::collections::BTreeMap<(u16, u16, u8), i32>,
    source: Option<u16>,
    strip10: u16,
    out: &mut Vec<u8>,
) {
    let base = 0x2000 + strip10 * 0x40;
    for slot in 0u16..8 {
        let en = source.and_then(|s| existing.get(&(s, slot, 0)).copied()).unwrap_or(1);
        let mix = source.and_then(|s| existing.get(&(s, slot, 1)).copied()).unwrap_or(12800);
        push_param(0, 0x1F, base + slot, en, out);
        push_param(1, 0x1F, base + slot, mix, out);
    }
    for (pid, default) in STRIP_PID_DEFAULTS {
        let v = source.and_then(|s| existing.get(&(s, 0, pid)).copied()).unwrap_or(default);
        push_param(pid, 0x1F, base, v, out);
    }
}

/* a 10 mixer preset holds one strip: eight slot pairs and the 12-pid run, 28 records, no
   header record. the strip index is kept when 10 has that strip, else the preset moves to
   insert 1 — the program ignores the index on load. */
fn preset_mixer_params_fl10(b: &[u8], c: &mut Counts, warnings: &mut Vec<String>) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut existing: BTreeMap<(u16, u16, u8), i32> = BTreeMap::new();
    for rec in b.chunks_exact(12) {
        let pid = rec[4];
        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
        let val = i32::from_le_bytes(rec[8..12].try_into().unwrap());
        if !(0x2000..0x2000 + 127 * 0x40).contains(&tgt) {
            continue;
        }
        existing.insert(((tgt - 0x2000) >> 6, (tgt - 0x2000) & 0x3F, pid), val);
    }
    let mut out = Vec::with_capacity(28 * 12);
    let mut strips: Vec<u16> = existing.keys().map(|k| k.0).collect();
    strips.dedup();
    for strip in strips {
        push_strip_fl10(&existing, Some(strip), fl10_strip(strip).unwrap_or(1), &mut out);
        for slot in 8u16..10 {
            let en = existing.get(&(strip, slot, 0)).copied().unwrap_or(1);
            let mix = existing.get(&(strip, slot, 1)).copied().unwrap_or(12800);
            if en != 1 || mix != 12800 {
                c.params_lost += 1;
                warnings.push(format!(
                    "preset slot {} mix/enable value lost (FL 10 has 8 slots)",
                    slot + 1
                ));
            }
        }
    }
    out
}

fn remap_target(tgt: u16) -> Option<u16> {
    if !(0x2000..0x2000 + 127 * 0x40).contains(&tgt) {
        return Some(tgt);
    }
    let strip = (tgt - 0x2000) >> 6;
    let off = (tgt - 0x2000) & 0x3F;
    fl10_strip(strip).map(|s| 0x2000 + s * 0x40 + off)
}

pub fn to_fl10(src: &Flp) -> Result<Outcome, String> {
    let version = src.version().ok_or("file has no version event (0xC7)")?;
    let major = src
        .version_major()
        .ok_or_else(|| format!("cannot parse version \"{version}\""))?;
    if major <= 10 {
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
    if major != 20 && major <= 20 {
        warnings.push(format!(
            "source is v{version}; only the 20.8 layout is verified as a starting point for the FL 10 rewrite"
        ));
    }
    let base_version = base.version().unwrap_or_default();
    let out = fl20_to_fl10(&base, &mut notes, &mut warnings)?;
    notes.insert(0, format!("version {base_version} -> {FL10_VERSION}"));
    Ok(Outcome { flp: out, notes, warnings })
}

#[derive(PartialEq, Clone, Copy)]
enum Section {
    Header,
    Channel,
    Pattern,
    Playlist,
    Mixer,
    Tail,
}

fn fl20_to_fl10(
    src: &Flp,
    notes: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<Flp, String> {
    let mut c = Counts::new();
    let mut out: Vec<Event> = Vec::with_capacity(src.events.len());

    /* which 20 strips hold anything worth warning about when they vanish */
    let mut insert_state = InsertState { used: vec![false; 127] };
    {
        let mut strip: isize = -1;
        let mut in_mixer = false;
        let mut pending_name = false;
        for ev in &src.events {
            match ev.op {
                op::INSERT_FLAGS => {
                    in_mixer = true;
                    strip += 1;
                    if pending_name {
                        if let Some(u) = insert_state.used.get_mut(strip as usize) {
                            *u = true;
                        }
                    }
                    pending_name = false;
                }
                INSERT_NAME if ev.blob().map(flp::utf16z).is_some_and(|s| !s.is_empty()) => {
                    pending_name = true
                }
                op::PLUGIN_INTERNAL_NAME if in_mixer => {
                    if ev.blob().map(flp::utf16z).is_some_and(|s| !s.is_empty()) {
                        if let Some(u) = insert_state.used.get_mut(strip.max(0) as usize) {
                            *u = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut section = Section::Header;
    let mut block: Vec<Event> = Vec::new();
    let mut strip20: u16 = 0;
    let mut last_lane_kept = false;
    let mut in_marker = false;

    let flush = |section: Section,
                 block: &mut Vec<Event>,
                 strip20: &mut u16,
                 out: &mut Vec<Event>,
                 c: &mut Counts,
                 warnings: &mut Vec<String>| {
        if block.is_empty() {
            return;
        }
        match section {
            Section::Channel => out.extend(channel_block(block, c, warnings)),
            Section::Mixer => {
                let s = *strip20;
                out.extend(insert_block(s, block, &insert_state, c, warnings));
                if s == FL10_INSERTS {
                    for bus in 100..FL10_CURRENT_STRIP {
                        out.extend(send_bus_block(bus));
                    }
                }
                *strip20 += 1;
            }
            _ => out.append(block),
        }
        block.clear();
    };

    for ev in &src.events {
        /* section transitions */
        match ev.op {
            op::CHANNEL_NEW => {
                flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                section = Section::Channel;
            }
            op::PATTERN_NEW => {
                flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                section = Section::Pattern;
                c.patterns += 1;
            }
            0x63 | 0xF1 | op::PLAYLIST | op::LANE | 0x64 | 0x1D if section != Section::Mixer => {
                flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                section = Section::Playlist;
            }
            INSERT_COLOUR | INSERT_NAME | op::INSERT_FLAGS => {
                if section != Section::Mixer {
                    flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                    section = Section::Mixer;
                }
                if ev.op == INSERT_COLOUR || ev.op == INSERT_NAME {
                    /* a colour/name opens the next block: close the previous one first */
                    if block.iter().any(|e| e.op == 0x93) {
                        flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                    }
                } else if block.iter().any(|e| e.op == 0x93) {
                    flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                }
            }
            op::MIXER_PARAMS | 0x85 => {
                flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);
                section = Section::Tail;
            }
            _ => {}
        }

        match section {
            Section::Channel | Section::Mixer => {
                block.push(ev.clone());
                continue;
            }
            _ => {}
        }

        /* time-signature markers (0x21/0x22 after a 0x94) did not exist in 10 */
        if ev.op == 0x94 {
            in_marker = true;
        } else if in_marker {
            match ev.op {
                0x21 | 0x22 => {
                    if ev.value() != Some(4) {
                        c.marker_nondefault = true;
                    }
                    c.markers_dropped += 1;
                    continue;
                }
                MARKER_NAME => in_marker = false,
                _ => in_marker = false,
            }
        }

        match ev.op {
            o if POST_FL10_OPS.contains(&o) => c.deleted += 1,
            op::VERSION => {
                let mut b = FL10_VERSION.as_bytes().to_vec();
                b.push(0);
                out.push(blob(op::VERSION, b));
            }
            0x1C => out.push(u8e(0x1C, 3)),
            0xC8 => out.push(blob(0xC8, FL10_REGISTRATION.to_vec())),
            op::TEMPO => {
                let v = ev.value().unwrap_or(120_000);
                let coarse = (v / 1000) as u16;
                let fine = (v % 1000) as u16;
                if fine != 0 {
                    c.tempo_fraction = true;
                }
                out.push(u16e(TEMPO_FINE, fine));
                out.push(u16e(TEMPO_COARSE, coarse));
            }
            COMMENT => {
                let text = ev.blob().map(flp::utf16z).unwrap_or_default();
                out.push(comment_event(&text, &mut c));
            }
            op::TITLE | 0xCE | 0xCF | 0xCA | 0xE7 | op::PATTERN_NAME | MARKER_NAME => {
                out.push(text_event(ev, ev.op, &mut c))
            }
            LANE_NAME => {
                if last_lane_kept {
                    let name = ev.blob().map(flp::utf16z).unwrap_or_default();
                    if !name.is_empty() {
                        c.lane_names_lost += 1;
                    }
                }
                c.deleted += 1;
            }
            PATTERN_COLOUR => {
                out.push(ev.clone());
                out.push(u32e(PATTERN_FL10_TAIL, 0));
            }
            op::PLAYLIST => {
                let b = ev.blob().ok_or("0xE9 without blob payload")?;
                if b.len() % 32 != 0 {
                    return Err(format!(
                        "playlist blob of {} bytes is not a multiple of 32",
                        b.len()
                    ));
                }
                let mut nb = Vec::with_capacity(b.len());
                for rec in b.chunks_exact(32) {
                    let mut r = rec.to_vec();
                    let lane = i32::from_le_bytes(rec[12..16].try_into().unwrap());
                    let mut l10 = lane - LANE_SHIFT;
                    if l10 < 0 {
                        c.clips_clamped += 1;
                        l10 = 0;
                    }
                    r[12..16].copy_from_slice(&l10.to_le_bytes());
                    nb.extend_from_slice(&r);
                    c.clips += 1;
                }
                out.push(blob(op::PLAYLIST, nb));
            }
            op::LANE => {
                let b = ev.blob().ok_or("0xEE without blob payload")?;
                let idx = if b.len() >= 4 {
                    u32::from_le_bytes(b[0..4].try_into().unwrap())
                } else {
                    0
                };
                if (1..=u32::from(FL10_INSERTS)).contains(&idx) && b.len() >= FL10_LANE_LEN {
                    out.push(blob(op::LANE, b[..FL10_LANE_LEN].to_vec()));
                    c.lanes += 1;
                    last_lane_kept = true;
                } else {
                    c.lanes_dropped += 1;
                    last_lane_kept = false;
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
                } else if src.is_mixer_preset() {
                    out.push(blob(op::MIXER_PARAMS, preset_mixer_params_fl10(b, &mut c, warnings)));
                } else {
                    out.push(blob(op::MIXER_PARAMS, mixer_params_fl10(b, &mut c, warnings)));
                }
            }
            0xD8 => {
                let b = ev.blob().ok_or("0xD8 without blob payload")?;
                if b.len() % 12 != 0 {
                    warnings.push(format!(
                        "initialised control blob of {} bytes is not a multiple of 12 — left unchanged",
                        b.len()
                    ));
                    out.push(ev.clone());
                } else {
                    let mut nb = Vec::with_capacity(b.len());
                    for rec in b.chunks_exact(12) {
                        let tgt = u16::from_le_bytes([rec[6], rec[7]]);
                        match remap_target(tgt) {
                            Some(nt) => {
                                let mut r = rec.to_vec();
                                r[6..8].copy_from_slice(&nt.to_le_bytes());
                                nb.extend_from_slice(&r);
                            }
                            None => c.controls_dropped += 1,
                        }
                    }
                    out.push(blob(0xD8, nb));
                }
            }
            op::AUTOMATION_LINK => {
                let b = ev.blob().ok_or("0xE3 without blob payload")?;
                if b.len() == 20 {
                    let tgt = u16::from_le_bytes([b[10], b[11]]);
                    match remap_target(tgt) {
                        Some(nt) => {
                            let mut nb = b.to_vec();
                            nb[10..12].copy_from_slice(&nt.to_le_bytes());
                            out.push(blob(op::AUTOMATION_LINK, nb));
                        }
                        None => {
                            c.links_dropped += 1;
                            warnings.push(
                                "an automation link targets a mixer insert above 99; link dropped".into(),
                            );
                        }
                    }
                } else {
                    out.push(ev.clone());
                }
            }
            _ => out.push(ev.clone()),
        }
    }
    flush(section, &mut block, &mut strip20, &mut out, &mut c, warnings);

    let push = |n: usize, msg: String, to: &mut Vec<String>| {
        if n > 0 {
            to.push(msg);
        }
    };
    push(c.deleted, format!("deleted {} post-v10 events", c.deleted), notes);
    push(
        c.strings,
        format!("rewrote {} strings UTF-16 -> ANSI", c.strings),
        notes,
    );
    if c.lossy_chars > 0 {
        warnings.push(format!(
            "{} characters outside the ANSI range became '?'",
            c.lossy_chars
        ));
    }
    push(
        c.channels,
        format!("restructured {} channel blocks to the v10 layout", c.channels),
        notes,
    );
    push(
        c.d7,
        format!("truncated {} channel blobs 0xD7 to 112 bytes", c.d7),
        notes,
    );
    if !c.stretch_unmapped.is_empty() {
        warnings.push(format!(
            "stretch mode(s) {} have no verified FL 10 equivalent; set to mode 1 (generic)",
            c.stretch_unmapped
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    push(
        c.patterns,
        format!("rewrote {} pattern blocks (added 0x97, dropped 0x9D/0x9E/0xA4)", c.patterns),
        notes,
    );
    push(
        c.clips,
        format!("moved {} playlist clips to v10 lanes (lane - 401)", c.clips),
        notes,
    );
    if c.clips_clamped > 0 {
        warnings.push(format!(
            "{} playlist clips sat on tracks above 99 and were moved to track 99",
            c.clips_clamped
        ));
    }
    push(
        c.lanes,
        format!(
            "kept {} playlist lanes as 22-byte records, dropped {} (index 0 and above 99)",
            c.lanes, c.lanes_dropped
        ),
        notes,
    );
    if c.lane_names_lost > 0 {
        warnings.push(format!(
            "{} playlist lane names dropped (FL 10 stores none)",
            c.lane_names_lost
        ));
    }
    push(
        c.markers_dropped,
        format!("removed {} time-signature marker fields", c.markers_dropped),
        notes,
    );
    if c.marker_nondefault {
        warnings.push("a removed time-signature marker was not 4/4 — the signature is lost".into());
    }
    if c.tempo_fraction {
        warnings.push(
            "tempo has a fractional part; the FL 10 fine-tempo field (0x5D) is unverified".into(),
        );
    }
    push(
        c.inserts_dropped,
        format!(
            "dropped {} mixer inserts above 99 and added the 4 v10 send buses",
            c.inserts_dropped
        ),
        notes,
    );
    push(
        c.slots_dropped,
        format!("dropped {} effects in slots 9 and 10", c.slots_dropped),
        notes,
    );
    push(
        c.wrappers,
        format!("wrapper state version 10 -> 7 on {} plugins (chunk 56 removed)", c.wrappers),
        notes,
    );
    push(
        c.native_states,
        format!("rewrote {} native effect states to their v10 versions", c.native_states),
        notes,
    );
    if !c.legacy_mapped.is_empty() {
        notes.push(format!(
            "re-wrapped {} as FL 10 VST DLLs with mapped parameters",
            c.legacy_mapped.join(", ")
        ));
        warnings.push(
            "legacy effect parameters were mapped from one truth pair; check them in FL 10".into(),
        );
    }
    if !c.legacy_reset.is_empty() {
        warnings.push(format!(
            "{} re-wrapped as FL 10 VST DLL(s) with default parameters (no mapping known)",
            c.legacy_reset.join(", ")
        ));
    }
    c.unverified_plugins.sort();
    c.unverified_plugins.dedup();
    if !c.unverified_plugins.is_empty() {
        warnings.push(format!(
            "plugin state carried unchanged, unverified in FL 10: {}",
            c.unverified_plugins.join(", ")
        ));
    }
    notes.push(if src.is_mixer_preset() {
        "rebuilt mixer preset params to the v10 single-strip 28-record shape".into()
    } else {
        "rebuilt mixer param table to the v10 2941-record shape".into()
    });
    if !c.impulses_resolved.is_empty() {
        notes.push(format!(
            "Fruity Convolver impulse path pointed at the file of a newer install on this machine: {}",
            c.impulses_resolved.join(", ")
        ));
    }
    if !c.impulses_missing.is_empty() {
        warnings.push(format!(
            "Fruity Convolver impulse not found in any FL Studio install on this machine; FL 10 loads Default.wav instead. copy the file into FL 10's Data\\Patches folder: {}",
            c.impulses_missing.join(", ")
        ));
    }
    if !c.effects_missing.is_empty() {
        warnings.push(format!(
            "{} mixer effect{} did not ship with FL 10, the slot loads empty: {}",
            c.effects_missing.len(),
            plural(c.effects_missing.len()),
            c.effects_missing.join(", ")
        ));
    }
    if c.params_lost > 0 {
        warnings.push(format!(
            "{} non-default mixer parameter values had no FL 10 home and were lost",
            c.params_lost
        ));
    }
    push(
        c.controls_dropped,
        format!(
            "dropped {} initialised controls targeting inserts above 99",
            c.controls_dropped
        ),
        notes,
    );
    push(
        c.inserts_dropped_used,
        format!("{} of the dropped inserts carried plugins or a name", c.inserts_dropped_used),
        notes,
    );
    push(
        c.routes_lost,
        format!("rerouted {} channels from inserts above 99 to the master", c.routes_lost),
        notes,
    );
    push(
        c.bus_routes_lost,
        format!("dropped {} insert-to-insert routes above 99", c.bus_routes_lost),
        notes,
    );
    push(
        c.insert_flags_lost,
        format!("{} disabled inserts load enabled", c.insert_flags_lost),
        notes,
    );
    push(c.sends_lost, format!("{} send levels lost", c.sends_lost), notes);
    push(
        c.links_dropped,
        format!("dropped {} automation links to inserts above 99", c.links_dropped),
        notes,
    );
    push(
        c.sample_paths_rewritten,
        format!("rewrote {} factory sample paths", c.sample_paths_rewritten),
        notes,
    );

    let leftovers: Vec<u8> = {
        let mut seen: Vec<u8> = out
            .iter()
            .map(|e| e.op)
            .filter(|o| !FL10_KNOWN_OPS.contains(o))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    if !leftovers.is_empty() {
        warnings.push(format!(
            "events kept that FL 10 is not known to write: {}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_v7_drops_vendor_chunk_and_clears_chunk2_flags() {
        let mut b = 10u32.to_le_bytes().to_vec();
        let chunk = |id: u32, data: &[u8], out: &mut Vec<u8>| {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(&(data.len() as u64).to_le_bytes());
            out.extend_from_slice(data);
        };
        let mut c2 = vec![0u8; 25];
        c2[12] = 0xA0;
        c2[17] = 1;
        chunk(1, &[1; 20], &mut b);
        chunk(2, &c2, &mut b);
        chunk(56, b"Vendor", &mut b);
        chunk(53, &[9; 5], &mut b);
        let v7 = wrapper_v7(&b).unwrap();
        assert_eq!(u32::from_le_bytes(v7[0..4].try_into().unwrap()), 7);
        assert!(wrapper::chunk_offset(&v7, 56).is_none());
        let (off, _) = wrapper::chunk_offset(&v7, 2).unwrap();
        assert_eq!(v7[off + 12], 0);
        assert_eq!(v7[off + 17], 0);
        assert_eq!(wrapper::chunk_offset(&v7, 53).map(|(o, l)| v7[o..o + l].to_vec()), Some(vec![9; 5]));
    }

    #[test]
    fn balance_native_defaults_map_onto_template_floats() {
        let mut c = Counts::new();
        let fx = LEGACY_EFFECTS.iter().find(|f| f.name == "Fruity Balance").unwrap();
        let native = [0i32, 256].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>();
        let state = legacy_state(fx, &native, &mut c);
        assert_eq!(state, FRUITY_BALANCE.to_vec());
        let native = [-128i32, 0].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>();
        let state = legacy_state(fx, &native, &mut c);
        let (off, _) = wrapper::chunk_offset(&state, 53).unwrap();
        assert_eq!(f32::from_le_bytes(state[off + 17..off + 21].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(state[off + 21..off + 25].try_into().unwrap()), 0.0);
    }

    #[test]
    fn phaser_native_defaults_map_onto_template_floats() {
        let mut c = Counts::new();
        let fx = LEGACY_EFFECTS.iter().find(|f| f.name == "Fruity Phaser").unwrap();
        let native = [1i32, 2500, 100, 800, 0, 512, 8, 400, 512, 4000]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let state = legacy_state(fx, &native, &mut c);
        let (off, _) = wrapper::chunk_offset(&state, 53).unwrap();
        let (toff, _) = wrapper::chunk_offset(&FRUITY_PHASER, 53).unwrap();
        for i in 0..9 {
            let a = f32::from_le_bytes(state[off + 17 + 4 * i..off + 21 + 4 * i].try_into().unwrap());
            let b = f32::from_le_bytes(
                FRUITY_PHASER[toff + 17 + 4 * i..toff + 21 + 4 * i].try_into().unwrap(),
            );
            assert!((a - b).abs() < 1e-3, "param {i}: {a} vs {b}");
        }
    }

    #[test]
    fn route_table_marks_send_buses_for_inserts_only() {
        let mut src = vec![0u8; 127];
        src[0] = 1;
        src[110] = 1;
        let t = route_table_fl10(5, &src);
        assert_eq!(t.len(), 105);
        assert_eq!(t[0], 1);
        assert_eq!(&t[100..105], &[1, 1, 1, 1, 0]);
        let m = route_table_fl10(0, &vec![0u8; 127]);
        assert!(m.iter().all(|&v| v == 0));
    }

    #[test]
    fn mixer_params_have_fl10_shape() {
        let mut c = Counts::new();
        let mut w = Vec::new();
        let mut src = Vec::new();
        let push = |pid: u8, tgt: u16, val: i32, out: &mut Vec<u8>| {
            out.extend_from_slice(&[0, 0, 0, 0, pid, 0x1F]);
            out.extend_from_slice(&tgt.to_le_bytes());
            out.extend_from_slice(&val.to_le_bytes());
        };
        push(192, 0x2000 + 13 * 0x40, 7200, &mut src);
        push(192, 0x2000 + 126 * 0x40, 6400, &mut src);
        push(192, 0x2000 + 110 * 0x40, 100, &mut src);
        let t = mixer_params_fl10(&src, &mut c, &mut w);
        assert_eq!(t.len(), 2941 * 12);
        let find = |strip: u16, pid: u8| {
            t.chunks_exact(12)
                .find(|r| r[4] == pid && u16::from_le_bytes([r[6], r[7]]) == 0x2000 + strip * 0x40)
                .map(|r| i32::from_le_bytes(r[8..12].try_into().unwrap()))
        };
        assert_eq!(find(13, 192), Some(7200));
        assert_eq!(find(104, 192), Some(6400));
        assert_eq!(find(100, 192), Some(12800));
        assert_eq!(c.params_lost, 1);
    }
}
