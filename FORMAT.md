# FLP conversion knowledge

Plugin wrapper records (`0xD5` records 30/32/50/51/52, and `0xD4` field B) depend on the plugin format, not the program version. The converter carries them through unchanged.

The "Fruity Wrapper" `0xD5` state holds three version-dependent host fields: the leading state version, chunk 57, and byte 12 of chunk 2. *20.0.5* writes no chunk 57, and writes chunk 2 byte 12 as 0. Its first `u32` is a state version. The ladder is 7 in *10.0.9*, 8 in *20.0.5*, 10 in *20.8*, and 12 in *25*. The state layout is the version `u32`, then repeated `(u32 chunk id, u64 length, payload)`. The length field is 64-bit. Every `0xD5` belongs to the plugin that the preceding `0xC9` internal name gives. The converter rewrites the first `u32` only for a "Fruity Wrapper" record.

A native plugin keeps its own state header in the same four bytes. The observed first `u32` values are plugin data, not a version: Fruity Love Philter 786435, Maximus 983043, Gross Beat 524291, Edison 851971, Fruity Fast Dist 171, Fruity Delay Bank 16, Fruity Parametric EQ 2 with 8, Fruity Limiter 7, Soundgoodizer 3, Fruity Compressor 2, Fruity Mute 2 with 1, Fruity Delay 2 with 0. Fruity Soft Clipper writes an 8-byte record of two parameters, 90 and 127. A clamp of these values corrupts the plugin state and the *20.8* loader stops.

## File envelope

```
"FLhd" u32:len u16:format u16:nChannels u16:ppq
"FLdt" u32:len <TLV event stream>
```

Event encoding by opcode range: `0x00-0x3F` u8, `0x40-0x7F` u16le, `0x80-0xBF` u32le, `0xC0-0xFF` LEB128 varint length + blob. One violation: *25*'s `0xAC` sits in the u32 range but carries a fixed 3-byte payload.

## The transform, any newer version to 20.8

This table is the intermediate stage. The tool writes the *20.0.5* layout, and the *20.0.5* post-pass and the *10.0.9* profile both start from the output of this stage.

| structure | newer form | 20.8 form | action |
|---|---|---|---|
| `0xC7` version | ASCII, e.g. `"25.1.4.4951"` | `"20.8.4.2576"` | rewrite |
| `0x9F` build | 4951 | 2576 | rewrite |
| `0xC8` registration | 10 bytes | 50 bytes | replace with the *20.8* blob |
| post-20 events | present | absent | delete (see set below) |
| `0x68` channel route (*25*) | u16, in the param block | `0x16` u8, same slot | rewrite in place; drop if `0x16` exists |
| `0xD5` wrapper marker, "Fruity Wrapper" only | 12 | 10 | rewrite first u32; other plugins pass through |
| `0xD7` channel blob | 168 bytes | 158 bytes | truncate (leading bytes agree) |
| `0xD7` stretch time (offset 96) | f32, 1/768 bar (*24.2*+) | u32, 1/768 bar | reinterpret f32 -> u32, same unit (see below) |
| `0xE9` clip records | 60 B (*21-24*) / 80 B (*25*) / 88 B (*26*) | 32 B | keep positions, lengths, and trims; reconcile end trims; move scale to `0xD7`; emulate fades |
| `0xEE` lane records | 70 bytes × 500 | 66 bytes × 500 | truncate each |
| `0xEB` route table | 1 byte (*25*) | 127 bytes | pad with zeros |
| `0xE1` param targets | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE3` link destination (offset 10) | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE1` table shape | tail records dropped | 4697 records | rebuild canonical table |
| `0xE1` in a mixer preset (`FLhd` format `0x40`) | one strip, 32 records | one strip, 32 records | rebase and rebuild that strip only (see below) |
| `0x85` selected insert | out-of-range values occur | ≤ 126 | clamp to 126 |
| pattern time marker | `0x94` run inside the pattern block | absent | delete the run (see below) |
| stream tail | `0xE1 0x85 0xF3 0x2F` | `0xE1 0x85` | `0xF3`/`0x2F` are in the delete set |

The deleted event set is the opcode difference between the two truth files, plus `0xAC`, minus `0xD8`, plus the four marker sub-records and the *25.2* additions: `0x29 0x2A 0x2B 0x2C 0x2D 0x2E 0x2F 0x30 0x31 0x32 0x33 0x34 0x65 0x67 0xA5 0xA6 0xA7 0xA8 0xA9 0xAA 0xAB 0xAC 0xC0 0xF2 0xF3 0xFC 0xFD`. Two genuine *20.8* saves proved that *20.8* writes `0xD8` (see below). `0xC0` carries the project's "FL Studio" name string; `0x34`, `0xAB`, `0xFC`, `0xFD` are per-pattern metadata, all zero in the observed *25.2.4* save.

*25* addresses the mixer "current strip" as strip 501 (`0xE1` target `0xED40`). *20.8* addresses it as strip 126. The rebase clamps strip indexes above 126 to 126.

A `.fst` preset is the same TLV stream with a different `FLhd` format field: `0x20` channel preset, `0x30` plugin preset, `0x40` mixer preset. A mixer preset stores the one strip it was saved from: the ten slot pairs and the 12-pid run (`192..226`), 32 records, no `0x4000` header record and no send tail. Every version from *12* to *26* writes this shape (*10* and *11* write 28 records: eight slot pairs). The strip index is whatever insert the preset was saved from; the program ignores it on load. A mixer preset rebuilt as the full 4697-record project table loads with strip 0's stock values (volume 100%), so the preset path rebuilds only the strips present in the source. *20.0.5* writes effect slot names (`0xCB`) into its own mixer presets but discards them when it loads any mixer preset, its own included; *10* and *26* read them. The converter keeps them and warns.

The canonical `0xE1` table is a header record, then per strip 0..126: ten slot pairs (pid 0 enabled, pid 1 mix), volume 192, pan 193, stereo separation 194, EQ 208-210 / 216-218 / 224-226, then a tail that shortens on high strips: sends 164-168 through strip 99, 168 only through 104, and 190 on every strip. Source values are kept where present. The program's defaults fill the rest.

## The 26 layout

One truth pair: a project saved by *25.2.4.5242* and the same project saved by *26.1.5.5618*. The event streams are identical except for four things. The `0xE9` clip record grows from 80 to 88 bytes: the first 72 bytes keep the *25* layout, the new bytes at 72..80 are `00 00 00 00 FF FF FF FF` on every clip, and the last 8 bytes are the zero tail of the *25* record. The header holds a second `0xAC` (`00 01 00`) after `0xC3`, followed by a `0x00` u8 event with value 0. `0xA9` is 15 instead of 7. `0xF2` byte 11 is `0x40` instead of 0. Every `0xD5` state, `0xD7` blob, `0xEE` lane record, `0xEB` table, and `0xE1` record is byte-identical to the *25* save, so the 20.8 transform applies unchanged beyond the clip size and the header `0x00`. The converter output for the *26* save matches the output for the *25* save except the `0xED` run-time bytes.

## The 24 and 21 targets

`convert::retarget` applies one `Layout` per target: `FL208`, `FL24`, or `FL21`. `convert24::to_fl24` and `convert24::to_fl21` run `retarget`, then a post-pass for plugin states and plugin gates. The *20.8* rules above apply unless this section names a difference.

The evidence is a corpus survey and one FL 21 load test. The survey compares opcode sets, blob sizes, and plugin state headers of genuine *21.x*, *24.x*, *25.x*, and *26.x* saves. No output was load-tested in FL 24. Treat each mapping below as open unless the load test below confirms it.

FL 21 load test (FL 21.2.3, November 2026). Three projects were converted with `--fl21`, opened in FL 21, and saved back unchanged:

| source | result | what the FL 21 save shows |
|---|---|---|
| *24.2.2*, 16 effects, 29 audio clips | loads | Parametric EQ 2 (4 states) and Compressor byte-identical to the output |
| *25.2.0*, 28 effects, 53 audio clips | loads | Parametric EQ 2 (6 states, 3 with offset 292 = 3 or offset 296 = 1) and Compressor (2 states) byte-identical; Fruity Convolver state 3 rewritten from 612 to 576 bytes; Gross Beat state 8 grows by 24 bytes |
| *26.1.6*, 27 effects, 56 audio clips | loads, then crashes in `Fruity Parametric EQ 2_x64.dll` (access violation) or fails with "Invalid pointer operation" when the project closes | fixed: EQ 2 offset 292 = 3 set to 1 (see below) |

In both loaded saves, VST plugins that FL 21 did not find (Auto-Tune Pro, Roland D-50) collapse to a 73-byte wrapper stub. That is a missing plugin in the test install, not a converter fault.

The *26* crash was bisected in FL 21.2.3 with variants of the output. Each variant was opened, played, and closed with File > New:

| variant | result |
|---|---|
| every Gross Beat removed | stable |
| wrapper chunk 2 flag byte restored from the source | crashes |
| every playlist clip removed | stable |
| clip offset 52 (per-clip pitch ratio, *25+*) reset to 1.0 | crashes |
| Gross Beat states replaced by an FL 21 save | crashes |
| every Parametric EQ 2 removed | stable |
| only the EQ 2 in front of a Gross Beat on insert 2 removed | stable |
| every EQ 2 kept, offset 292 = 3 set to 1 (2 states) | stable |

The EQ 2 on insert 2 held offset 292 = 3. The crash needs audio through that EQ, which is why removing the clips or the Gross Beat after it also hid the crash. The *24.2.2* and *25.2.0* projects hold offset 292 = 3 too, but on EQs that got little or no audio in the test. The converter output now equals the last variant byte for byte.

| structure | 25/26 form | 24.2.2 form | 21.2.3 form |
|---|---|---|---|
| `0xC7` version / `0x9F` build | `25.x` / `26.x` | `"24.2.2.4597"` / 4597 | `"21.2.3.4004"` / 4004 |
| `0xC8` registration | several forms | 10 bytes `20 00 20 00 20 00 20 00 00 00` | the same 10 bytes |
| `0xA9` header event | 7 (*25*), 15 (*26*) | 5 | deleted |
| deleted opcodes | present | `0x33 0x34 0x67 0x8E 0xAA 0xAB 0xAC 0xC0 0xFB 0xFC 0xFD` | the *24* set plus `0x30 0x31 0x32 0xA9` |
| `0xD5` wrapper state version | 12 | 12 | 10 |
| wrapper chunk 57 | present | kept | kept |
| wrapper chunk 2 byte 12 | can set `0x01`, `0x10` | clear bit `0x01` | clear bits `0x11` |
| `0xD7` channel blob | 168 bytes | 168 bytes | 168 bytes |
| `0xD7` stretch time | f32 | f32 | u32 |
| `0xE9` clip record | 80 or 88 bytes | 60 bytes, the leading 60 | 60 bytes, the leading 60 |
| `0xE9` stretch scale (offset 64) | f64 | folded into the `0xD7` stretch time | folded into the `0xD7` stretch time |
| clip fades (offsets 36 and 44) | native | native, no emulation | native, no emulation |
| `0xEE` lane record | 70 bytes | 70 bytes | 66 bytes |
| `0xEB` | 1 byte | padded to 127 bytes | padded to 127 bytes |
| `0xE1` mixer table | base `0x7000` | 4697 records, base `0x2000` | 4697 records, base `0x2000` |
| pattern time markers | present | kept | kept |

Evidence notes:

- Delete sets: each deleted opcode occurs in saves of a newer version and in no save of the target version. `0x30 0x31 0x32 0xA9` occur in *24.2* saves and in no *21.2* save. Survivors outside `FL20_KNOWN_OPS` and the layout's `known_extra_ops` stay and produce a warning. `0xA0` and `0xA1` occur in two *25.2* saves only; their meaning is unknown.
- `0xC8`: 53 of 66 *24* saves hold the 10-byte stand-in form.
- The 60-byte clip record is a prefix of the 80- and 88-byte records. Fade lengths at offsets 36 and 44 have the same meaning in all four versions.
- The *20.8* end-trim reconcile and the fade emulation do not run for these targets. No evidence shows that *21* or *24* recompute stretched clip lengths at load. This is an open question.
- Some *25* and *26* saves hold wrapper state 10. The converter keeps that value.

Native plugin states (leading `u32` / length):

| plugin | 21 | 24 | 25 | 26 | rule |
|---|---|---|---|---|---|
| Fruity Compressor | 2 / 32 | 2 / 32 | 3 / 36 | 3 / 36 | 3 -> 2, drop the trailing `u32`; warn when it is nonzero |
| Fruity Parametric EQ 2 | 7 | 8 | 8 | 8 | *21* only: rewrite the version word 8 -> 7, set offset 292 to 1 when it is not 0 or 1, keep the rest (load-tested) |
| Gross Beat | 8 | 8 | 8 | 9 | 9 -> 8, the *20.0.5* rule |
| FLEX | — | — | — | 1 / 2707 | carried unchanged, warned (the *26* rework) |

EQ 2 offset 292 is a `u32` setting. All 71 corpus *21* states hold 0 or 1. *24* to *26* states also hold 3 (61 of 216 *24* states, 394 of 639 *26* states). FL 21.2.3 loads a 3 and saves it back unchanged, but the plugin crashes when it processes audio. The converter sets it to 1, the *21* value in 68 of 71 states, and warns. The meaning of the setting is not known; a truth pair of one *24* EQ 2 with that setting changed would name it. Offset 296 = 1 occurs in *26* states only, and FL 21 load-tested it without a fault; it is kept.

The states named in `NATIVE_STABLE` (`convert24.rs`) hold the same leading `u32` and length in every corpus version that holds them, and pass through without a warning. Examples: Low Lifter 6 / 82 in *24* and *26*, EQUO equal in *20* and *25*, Fruity Reeverb equal in *21* and *24*. Every other native state passes through and is named in one warning. LuxeVerb, Distructor, Spreader, and Hyper Chorus have no cross-version evidence.

Plugin gates come from the release history of the official plugins. A missing effect is dropped from its slot with a warning. A missing generator stays, and the warning names it.

- Missing in *24.2* (`MISSING_IN_FL24`): Emphasis, Emphasizer, FL Studio Mobile, FL Studio Mobile Rack, Fruity Slicer 2, Transmitter.
- Missing in *21.2* (`MISSING_IN_FL21`): the *24* list plus Kepler Exo, Low Lifter, Spreader, Transporter.

Open questions:

- A load test in FL 24.
- The meaning of EQ 2 offset 292 (value 3 in *24+*), and whether 1 is the closest *21* value.
- The meaning of `0xA0` and `0xA1`.
- Whether *24* reads the *26* FLEX state.
- Per-clip audio editing (*25*) and Stretch Pro (*20.9*): the survey found no field beyond the ones above.

## 0xD7 sampler stretch time

`0xD7` offset 96 holds the sampler "time stretching: time" value. Both versions use the same unit: 1/768 bar (PyFLP `LinearMusical`, bars×768 + steps×48 + ticks, 16 steps per bar, 48 ticks per step). Only the storage type differs. *20.8* stores a `u32le`. *24.2* and later store an `f32le` in the same four bytes. The survey pins the switch between build 24.1.1.4285, which still writes u32, and build 24.2.2.4597, which writes f32. The conversion is `u32 = round(f32)`. The project PPQ does not enter it.

Evidence: a *25.1.3* project at PPQ 96 in 4/4 stores the f32 24576.0 on a channel whose Time knob shows 32:00:00 in *25*. 32 bars × 768 = 24576, so the stored number is the display unit. A tick count contradicts the display, because 24576 ticks at PPQ 96 is 64 bars. An earlier converter rescaled with `u32 = round(f32 × 192 / ppq)` and wrote 49152, and *20.8* showed 48:00:00 — the knob maximum, clamped. That formula came from a truth pair at PPQ 192, where one bar is 768 ticks. At that PPQ alone, the tick reading and the 1/768-bar reading give the same number.

Left as-is, *20.8* reads the f32 bit pattern as a u32 (about 1.1e9), stretches the sample to millions of bars, and resizes every playlist clip of that channel to match. The two encodings never overlap: a real u32 stays below `0x1000000` for any real song, and an f32 of one unit or more has bits ≥ `0x3F800000`. The converter treats values ≥ `0x10000000` as f32.

## 0xE9 clip stretch scale

*25* stores an f64 stretch scale at offset 64 of each clip record. *20.8* has no per-clip scale field.

When all clips of a channel share one scale, fold the scale into the channel: multiply the `0xD7` stretch time by it. Keep clip positions, lengths, and trims unchanged. *20.8* derives clip playback from the channel stretch time (see the recompute below), so the fold gives the same sound.

Do not retime clip lengths instead. A new length moves the clip's right edge, and *20.8* shows the wrong source segment. Use that fallback only when clips of one channel carry different scales.

## 0xE9 audio clip trims and the v20 length recompute

Record offsets 24 and 28 hold `f32` start and end trims, in source-sample milliseconds. Every version from *20* to *25* keeps this meaning. The converter copies both values unchanged.

*20.8* recomputes each audio clip length at load time, when the clip's channel has a nonzero sampler stretch time. The rule is `len = round((end - start) / (R × tick_ms))`, with `R = sample_ms / stretched_ms` and `tick_ms = 60000 / (tempo × ppq)`. Channels without a stretch time keep the stored lengths.

This recompute is why the scale fold above works: the folded stretch time gives the same effective R as the *25* per-clip scale.

The recompute also means a clip changes length on load when its trim window disagrees with its stored length. The converter reconciles end trims so the recompute lands on the stored length. It estimates R per stretched channel from that channel's own clips. A point estimate `window / (len × tick)` is biased, because the stored length is a rounded value. Each clip instead bounds R to the interval `(window / ((len + 0.5) × tick), window / ((len - 0.5) × tick))`. Any R inside that interval makes the recompute return the stored length. The estimate is the midpoint of the deepest interval overlap, with a minimum overlap of two clips.

Where the window is too long, *20.8* lengthens the clip, and the converter rewrites the end trim. Where the window is too short, *20.8* shortens the clip. The converter only warns there, because the sample data to extend the clip does not exist. Channels with fewer than two usable clips pass through unchanged.

## Clip fades (21+) and the channel-volume emulation

The 80-byte clip record tail (*25*): u32 clip uid at 32, f32 fade-in ms at 36, f32 fade-out ms at 44, f32 gain at 52, u32 fade flags at 56, and f64 stretch scale at 64. Offsets 24-31 keep the v20 trim meaning.

v20 has no fade fields. The converter emulates each distinct (channel, length, fade-in, fade-out) clip shape with one automation channel linked to the audio channel's volume, plus one playlist clip per faded instance on a free lane. All template bytes come from a genuine *20.8.4.2576* save of exactly this construction:

- **Channel block**: kind (`0x15`) = 5, standard channel param run, `0xEA` curve, `0x8F` = 3, `0x91` = 1. Inserted before the first `0x63`. `FLhd` nChannels counts it.
- **`0xEA` points**: f64 delta position in beats, f64 value (0..1 of the 0..12800 knob), f32 tension, u32 flags. Flags: first point 0, middle points `0x02000000`, last point `0xFF000000`. The 17-byte header and the 112-byte tail are constant across every observed `0xEA`, in every version, whatever the point count. The knob maps to amplitude as approximately `knob^2.42` (0 dB at 100%, -5.2 dB at the 0.78125 default). The emulation writes a fade that is linear in amplitude, with the knob values `level*x^(1/2.42)` for the fade progress `x`. The curve uses up to 11 points per fade, all at tension 0, spaced dense near the silent end. Points closer than 0.01 beats merge. A zero-width final point restores the knob value so clips after a fade-out stay audible.
- **`0xE3` link**, channel volume form: u16 0 at 0, u16 automation channel index at 2, u16 0 at 8 (group 0 = channel space), u16 target channel index at 10, then `08 00 00 00 D5 01 00 00`.
- **`0xD8`**: one appended record per link — pid 0, group 0, target = channel index, value = the channel's `0xDB` volume.
- **Playlist record**: same pos/length as the audio clip, item index = automation channel, bytes 16-23 = `78 00 40 00 40 64 80 80`, f32 0.0 at 24, f32 length-in-beats at 28.

The automation value must equal the channel's own `0xDB` volume (default 10000/12800 = 0.78125) — automation replaces the knob, it does not scale it. Fade ms convert to beats through the project tempo (`0x9C` / 1000).

## Pattern time markers

A time marker is a `0x94` position, then `0x21` numerator, `0x22` denominator, and `0xCD` name. *21* and later add four sub-records to each marker: `0x2E`, `0x2D`, `0xA8`, and `0x65`.

*21* and later write one marker into every pattern metadata block. The block shape is `0x41` pattern number, optional `0xC1` name, `0x96` colour, `0x9D`, `0x9E`, then the marker run, then `0xA4`. The marker is auto-generated: five genuine *24.2* and *25.1* saves all hold position 0, flag `0x08000000`, 4/4, and the name "4/4". The four sub-records are zero in all of them.

*20.8* writes the pattern block as `0x41`, optional `0xC1`, `0x96`, `0x9D`, `0x9E`, `0xA4`. A genuine *20.8.4.2576* save with 52 pattern blocks and 2 time markers holds no `0x94` run in any pattern block. Its markers sit in the arrangement, after the `0xE9` playlist event, as `0x94 0x21 0x22 0xCD`.

The rules follow from that evidence. Delete `0x2D`, `0x2E`, `0x65`, and `0xA8` everywhere. Drop the whole marker run inside a pattern block. Keep arrangement markers. A dropped run that is not the default 4/4 loses a pattern time signature, so the converter warns.

## The transform, 20.8 to 20.0.5

The tool writes this layout. The rules come from one truth set: a project saved by *20.0.5.681*, the same project re-saved by *21.2.3.4004*, and the same project re-saved by *25.2.4.5242*. Each rule is verified byte for byte against the *20.0.5* save.

| structure | 20.8 stage form | 20.0.5 form | action |
|---|---|---|---|
| `0xC7` version | `"20.8.4.2576"` | `"20.0.5.681"` | rewrite (11-byte NUL-terminated ASCII) |
| `0x9F` build | 2576 | 681 | rewrite |
| `0x1C` | 1 | 3 | rewrite |
| `0xC8` registration | 50 bytes | 12 bytes `3c 00 37 00 38 00 31 00 44 00 00 00` | replace |
| `0x26 0x27 0x28` | present, values 1, 0, 0 | absent | delete |
| `0xD7` channel blob | 158 bytes | 157 bytes | truncate |
| `0xEE` lane records | 66 bytes x 500, index 1..500 | 62 bytes | truncate each record; keep all 500 |
| `0xEE` default lane colour, bytes 4..6 | `34 38 3a` | `48 51 56` | rewrite only on an exact match |
| `0x80` default channel colour | `0x474541` | `0x6A655C` | rewrite only on an exact match |
| `0xE9` clip records | 32 bytes, lane = 500 - track | identical | pass through |
| `0xE1`, `0xEA`, `0xEB`, `0xEC`, `0xE0`, `0xE2`, `0xE3`, `0xE4`, `0xE5`, `0xE7`, `0xCC`, `0xD1`, `0xD4`, `0xD8`, `0xDA`, `0xDB`, `0xDD`, `0xED`, `0xF1` | any | identical | pass through |
| "Fruity Wrapper" `0xD5` | state version 10 | 8 | rewrite the first u32 |
| Fruity Wrapper chunk 57 | 4-byte payload `60 09 00 00` on VST2 hosts | absent | delete the chunk |
| Fruity Wrapper chunk 2, payload byte 12 | `A0` (*20.8*), `A4` (*21*, *25*) | `00` | set to 0; byte 17 stays 1 |
| Fruity Wrapper chunk 53 and every other chunk | any | any | pass through; never resize |
| Fruity Limiter `0xD5` | version 7, 169 bytes | version 6, 168 bytes | rewrite the version, drop the last byte |
| Fruity Delay 2 / Delay 3 / Fruity Parametric EQ `0xD5` | 32 / 108 / 116 bytes | identical | pass through |
| other native plugin `0xD5` | any | unknown | pass through byte-exact and warn |

The deleted set is `0x26 0x27 0x28`. The *20.8* stage already removes `0x29 0x2A 0x2B 0x2C 0x2F 0xA5 0xA6 0xA7 0xF2 0xF3`.

The `0xD7` tail that the truncation drops is constant: zeros, then the f64 0.5 at offset 160 of the 168-byte *21* form. The `0xEE` tail that the truncation drops is four zero bytes.

A source already at major 20 (a *20.8* save) skips the 20.8 stage and takes the post-pass only, because a genuine *20.8* save holds none of the post-*20.8* events. *20.9* saves are untested.

*20.0.5* wrote only 33 lane records, indexes 1..33. That count is a property of the session, not of the format, and it is not derivable from the project. *21* accepted the 33-record file, and *20.0.5* accepts the 500 records the converter writes.

Fruity Parametric EQ 2: *20.0.5* hangs at playback on a version 7 or 8 state (354 bytes). A *20.0.5* save of a converted plugin writes version 3 at 309 bytes: the first 305 bytes of the newer state, then a zero `u32` where the newer state holds 1. The converter writes that form. Every unverified native state is listed in one warning.

Gross Beat leads with `u16 3` and a `u16` state version: 8 in *20.0.5*, 9 in *26*. *20.0.5* resets a version 9 state to its default (6961 bytes). The 9 body is the 8 body plus one trailing byte: a *10.0.9* save of a version 9 state (loaded correctly there) is the same body without that byte, and the *20.0.5* default state ends where the 8 body ends. The converter rewrites 9 to 8 and drops the byte; *20.0.5* loads that state and saves it back unchanged.

Fruity Convolver: *26* writes version 15 (576 bytes) and *20.0.5* loads it unchanged (a *20.0.5* save returns the same bytes). *10.0.9* writes version 14 (525 bytes) and reads the 15 body; its differences in a save are the impulse path and the fields derived from the impulse. The impulse path is a length-prefixed string at offset 21. *26* writes stock impulses under `%FLStudioFactoryData%\Data\`, a macro *10* does not know, so *10* falls back to `Default.wav`.

## The transform, 20.8 to 10.0.9 (experimental profile)

The FL 10 profile runs the 20.8 transform first, then rewrites the 20.8 stream into the 10.0.9 layout. Every rule below comes from one truth pair: a project saved by *10.0.9* and the same project opened and saved by *21.2.3*. The converter output for that pair matches the *10.0.9* save event for event. The remaining differences are theme colours, window rectangles, run-time pointer bytes, and the VST plugins' own state chunks.

*10* writes every text event as a NUL-terminated ANSI string. *12* and later write UTF-16. The channel name is `0xC0` in *10* and `0xCB` in *20*.

| structure | 20.8 form | 10.0.9 form | action |
|---|---|---|---|
| `0xC7` version | `"20.8.4.2576"` | `"10.0.9"` | rewrite |
| `0x9F` build, `0x25`, `0x23`, `0x2C`, `0xA7` | present | absent | delete |
| `0x1C` | 1 | 3 | rewrite |
| `0xC8` registration | 50 bytes UTF-16 | 22 bytes ANSI | replace with the *10* blob |
| `0x9C` tempo | u32, BPM × 1000 | `0x5D` u16 fine (thousandths) then `0x42` u16 coarse BPM | split; the fine field is unverified (both truth tempos are integers) |
| `0xC3` comment | UTF-16 text | `0xC6` RTF document | wrap in the *10* RTF template |
| text events | UTF-16 | ANSI | rewrite; code points above U+00FF become `?` |
| channel block | `40 15 C9 D4 CB 9B 80 [D5] 00 … 91 [EA] 20 E4 E4 DA×5 8F 14 [C4]` | `40 15 [C9 D4 D5] 00 … 91 [EA] E4 E4 DA×5 8F 14 [C4] C0 9B 80` | drop `C9`/`D4` on non-plugin kinds; drop `0x20 0x61 0x29`; move the name (as `0xC0`), `0x9B`, `0x80` to the end |
| `0xD7` channel blob | 158 bytes | 112 bytes | truncate (leading bytes agree) |
| `0xD7` stretch mode (offset 108) | 6 = Pro default, 0 = none | 1, 0 | map 6→1, 0→0; other modes become 1 with a warning |
| pattern block | `41 [C1] 96 9D 9E A4` | `41 [C1] 96 97` | drop `9D 9E A4`; add `97` = 0 |
| `0xE9` clip records | 32 bytes, lane = 500 − track | 32 bytes, lane = 99 − track | lane − 401; lanes below 0 (track > 99) clamp to 0 |
| `0xEE` lane records | 66 bytes, index 1..500 | 22 bytes, index 1..99 | truncate; keep index 1..99 and drop the rest |
| `0xEF` lane name, `0x63 0xF1 0x24 0x64 0x27 0x28 0x1F 0x26` | present | absent | delete |
| insert block | `[95] [2A] [CC] EC(12) (C9 D4 [CB] 9B 80 D5 62 or 62)×10 EB(127) 9A 93` | `[95] [CC] 1B(12) EC(5) (C9 D4 [CB] D5)* EB(105) 9A 93` | rebuild; slots 9 and 10 are dropped with a warning |
| `0xEC` insert flags | 12 bytes, flags at offset 4 | `00 00 00 00 01` | replace; a disabled insert (flag 0x08 clear) loads enabled |
| mixer strips | 127: master, inserts 1..125, current 126 | 105: master, inserts 1..99, send buses 100..103, current 104 | drop 100..125; 126 → 104; insert the four send-bus blocks |
| `0xEB` route table | 127 bytes | 105 bytes | copy 0..99; bytes 100..103 are 1 on every insert 1..99 (the *10* convention), 0 elsewhere |
| `0xE1` param table | 4697 records | 2941 records: header + 105 × (8 slot pairs + 12 pids) | rebuild; no send levels (164..168) and no pid 190 |
| `0xD8`, `0xE3` mixer targets | strips 0..126 | strips 0..104 | remap 126 → 104; drop targets on 100..125 |
| "Fruity Wrapper" `0xD5` | state version 10, chunk 56 present, chunk 2 bytes 12/17 = `A0`/`01` | version 7, no chunk 56, bytes 12/17 = 0 | rewrite |
| Fruity Limiter `0xD5` | version 7, 169 bytes | version 6, 168 bytes | rewrite the version, truncate |
| Fruity Parametric EQ 2 `0xD5` | version 7 or 8, 354 bytes | version 2, 305 bytes | rewrite the version, truncate |
| Gross Beat `0xD5` | `u16 3, u16 9`, trailing byte | `u32 8`, no trailing byte | rewrite the head, drop the byte (matches a *10* save byte for byte except float rounding) |
| Fruity Convolver `0xD5` | impulse under `%FLStudioFactoryData%\Data\` | absolute path | point at the file of the newest `Image-Line\FL Studio *` install on this machine that has it; else `%FLStudioData%\` and a warning |
| Fruity Reeverb 2 `0xD5` | `0x2711`, 66 bytes | `0x2710`, 58 bytes | rewrite the version; remove the 8 bytes inserted at offset 56 |
| Fruity Delay 2, Fruity Soft Clipper | 32 / 8 bytes | identical | pass through |
| Fruity Blood Overdrive, Reeverb, Phaser, Chorus, Balance | native `C9 "<name>"` + int-parameter `0xD5` | `C9 "Fruity Wrapper"` + `CB "<name>"` + wrapper `0xD5` around a VST DLL | replace with the *10* wrapper template; map parameters (see below) |
| stream tail | `0xE1 0x85` | `0xE1 0x85` | `0x85` passes through |

The deleted event set is the opcode difference between the two truth files, minus the structurally handled `0x62`, `0x9C`, and `0xC3`: `0x1F 0x20 0x23 0x24 0x25 0x26 0x27 0x28 0x29 0x2A 0x2B 0x2C 0x2F 0x61 0x63 0x64 0x9D 0x9E 0x9F 0xA4 0xA5 0xA6 0xA7 0xF1 0xF2 0xF3`. Time-signature marker fields (`0x21`/`0x22` after a `0x94`) did not exist in *10* and are dropped with a warning when they are not 4/4.

### The "routed to insert 100" symptom

A *10* save sets bytes 100..103 of every insert's `0xEB` route table. These are the four fixed send buses, which every insert feeds at level 0. *21* copies the table verbatim and shows the inserts routed to inserts 100..103. The *20.8* `0xE1` send pids 164..168 (route levels to strips 100..104) are a fossil of the same layout.

### Legacy effects hosted as VST DLLs

In *10* the Fruity Blood Overdrive, Fruity Reeverb, Fruity Phaser, Fruity Chorus, and Fruity Balance effects are VST DLLs under `%FLStudioPlugins%\Fruity\Effects\`, hosted by Fruity Wrapper. Their wrapper chunk 53 is the DLL's state: a 13-byte head `F7 FF FF FF 05 00 00 00 00 00 00 00 00`, a u32 parameter count, that many f32 values in 0..1, a u32 1, and a preset name. *21* hosts them natively with an int-parameter state.

The truth pair holds each effect once, at default settings, so each parameter mapping rests on one data point. The mappings the converter applies:

- Balance (no version word): pan `(v + 128) / 256`, volume `v / 320`.
- Phaser (version word, 9 params): `v / 5000`, `v / 1000`, `v / 1000`, `v / 1000`, `v / 1024`, stages `v / 23`, `v / 1000`, `v / 1024`, `v / 5000`.
- Reeverb (version word, 10 params): `v / 65536` for all but index 5, which is `v / 40`.
- Chorus (version word, 12 params): `v / 1024`, `v / 5000`, `v / 1024`, then `v / 5000` for the rest.
- Blood Overdrive: the native state has 8 parameters and the DLL 6; no mapping. The *10* default state is written and the converter warns.

Unknown plugins keep their state unchanged and are reported.

### Open questions for the 10 profile

- `0x5D` fine tempo: the PyFLP name is "TempoFine"; the encoding as thousandths is unverified.
- `0x1B` = 12 before every insert, `0x97` = 0 after every pattern colour: meaning unknown, values constant.
- Send levels: *10* stores none in `0xE1`. Where the send knobs live is unknown.
- `0xEC` 5-byte flags: only `00 00 00 00 01` observed.
- Stretch modes other than 0 and 6.
- *10* omitted three of the 99 lane records for a reason not understood; *21* filled them with byte 12 = 1. The converter writes all 99.

## Native effect states: Fruity Parametric EQ 2, Soft Clipper, Reeverb 2

A native effect slot carries `0xC9` name, `0xD4` runtime descriptor, `0x9B`, `0x80`, `0xD5` state, then `0x62`. It has no `0xCB`. The layouts below are little-endian, from one *20.0.5* save re-saved by *26.1.5* (mikeDAW `.knowledge/FLP.md` has the same table and the converter in `Source/Import/FLNativeFx.cpp`).

**Fruity Parametric EQ 2.** The version 3 blob is 309 bytes. *26* writes version 8: the same 309 bytes plus a 45-byte tail. Offsets: 0 u32 version. 4 seven i32 gains in centi-dB (-1800..1800). 32 seven i32 frequencies, 0..65536 on a log scale, `Hz = 8.413 * 3411.65^(v/65536)`, defaults 10923, 18204, 25486, 32768, 40050, 47332, 54613. 60 seven i32 bandwidths 0..65536, defaults 40124 for bands 1 and 7 and 25412 for the others. 88 seven i32 types: 0 Off, 1 Low Pass, 2 Band Pass, 3 High Pass, 4 Notch, 5 Low Shelf, 6 Peaking, 7 High Shelf, defaults 5,6,6,6,6,6,7. 116 seven i32 orders -3..3: negative = Steep 8/6/4, 0 = 2-pole, positive = Gentle 4/6/8. 144 eight zero i32. 176 a second freq/bw/type block that always holds the defaults (the compare snapshot). 260 seven zero i32. 288 i32, assumed master level in centi-dB, unverified. 292 u32 3. 296 u32 0. 300 bytes `01 02 00 00`. 304 four zero bytes and a final u8 0.

**Fruity Soft Clipper.** 8 bytes: u32 threshold, u32 post gain, both in 1/128 units (defaults 100 and 128).

**Gross Beat.** *26* writes 35967 bytes at factory slots: a 49-byte header, 72 slot records (0..35 time, 36..71 volume), and a final u8 1. Header offsets: 0 u16 3 and u16 9 (older saves write u16 8). 4 i32 time slot, 0-based. 8 i32 volume slot, 0-based. 12 i32 time mix and 16 i32 volume mix, 256 = 100 %. 20 i32 attack, default 1765, and 24 i32 release, default 1640; knob normalized = v/8192, attack ms = 500 n^3, release ms = 1000 n^3. 28 i32 tension, default 0. 32..48 flags, unresolved. Default header: `03 00 09 00 00 00 00 00 02 00 00 00 00 01 00 00 00 01 00 00 e5 06 00 00 68 06 00 00 00 00 00 00 00 00 00 00 00 00 01 00 01 00 00 00 01 00 01 01 01`.

A slot record is u8 name length, the name, 15 flag bytes (`01`, twelve `00`, `01`, `00`; "Empty" slots have `00` at byte 13), u32 3, u32 3, u32 point count, the points, a u32 trailer (0, or 3 for Drum Loop and Copter), and sixteen `FF` bytes. A point is f64 dx, f64 y, f32 tension, u32 type. dx is the delta from the previous point x; the first dx is absolute and can be above 0. x spans 0..4 beats. type = (aux << 24) | curve, with aux `FF` for tension below 0, 1 above 0, 2 at 0, and sometimes 0. Curve and tension shape the segment into the point. Curves: 0 single, 1 double, 2 hold, 3 stairs, 4 smooth stairs, 5 pulse, 6 wave, 7 half sine, 8 alt single, 9 alt double. A time y is an offset of -(1 - y) * 8 beats. A volume y is the gain.

**Fruity Reeverb 2.** 66 bytes: sixteen u32 `10001, 75, 40, 0, 87, 100, 15, 40, 100, 500, 0, 37, 46, 97, 684, 0` then u16 0 at default settings. The knob order in the manual is Mid/Side, H.Cut, L.Cut, Delay, Size, Diff, Mod, Speed, Bass, Cross, Dec, Damp, Dry, ER, Wet, Sep. The field mapping is unresolved.

## Cut groups (`0x84`)

`0x84` is a u32 in the channel block: the cut group in the low u16, the cut-by group in the high u16. "Cut itself" is a group of the channel's own, with both words equal and nonzero. The program numbers the groups by creation order and never shares one. Every plugin generator receives the next free group on creation (`0x10001`, `0x20002`, `0x30003`, `0x40004` across four generators in one *24* save). A sampler receives the next free group when the user ticks the box, and stays at 0 otherwise. The layout is the same in *10*, *20.0.5*, *21*, *24*, *25*, and *26*. The converter passes the event through unchanged. A writer that puts one number on several channels makes them choke each other.

## Lane count by version

The `0xE9` lane field is `laneCount - 1 - lane`. The lane count is 500 from *20*, 199 in *12* to *19*, and 99 in *10*. The count of `0xEE` records is not the lane count: *20.0.5* wrote 199 of its 500 lanes in one save and 33 in another, and *10* writes none. *10* has no `0x63` arrangement event; its `0xE9` sits in the header.

## Version and structure are independent

The program's loaders select nothing from the claimed `0xC7` version. *24* rejects 66-byte lane records in a file that claims *21*, and accepts 70-byte lanes in the same file. A converter must rewrite structures, not headers.

## Safety gates

- The parser must reproduce the input byte-exact (`serialize(parse(x)) == x`) before the convert button enables. This proves no event was misread.
- Events that survive conversion but are outside the known *20.8* opcode set are reported as warnings, never deleted silently. The *20.0.5* gate is the same set minus `0x26 0x27 0x28`. The 10 profile applies the same gate against the *10.0.9* opcode set.
