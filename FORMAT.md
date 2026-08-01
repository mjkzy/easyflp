# FLP conversion knowledge

Plugin wrapper records (`0xD5` records 30/32/50/51/52/57, and `0xD4` field B) are byte-identical across versions. They depend on the plugin format, not the program version. The converter carries them through unchanged.

One record is version dependent: the `0xD5` state of the VST host plugin "Fruity Wrapper". Its first `u32` is a state version, 12 in *25* and 10 in *20.8*. Every `0xD5` belongs to the plugin that the preceding `0xC9` internal name gives. The converter rewrites the first `u32` only for a "Fruity Wrapper" record.

A native plugin keeps its own state header in the same four bytes. The observed first `u32` values are plugin data, not a version: Fruity Love Philter 786435, Maximus 983043, Gross Beat 524291, Edison 851971, Fruity Fast Dist 171, Fruity Delay Bank 16, Fruity Parametric EQ 2 with 8, Fruity Limiter 7, Soundgoodizer 3, Fruity Compressor 2, Fruity Mute 2 with 1, Fruity Delay 2 with 0. Fruity Soft Clipper writes an 8-byte record of two parameters, 90 and 127. A clamp of these values corrupts the plugin state and the *20.8* loader stops.

## File envelope

```
"FLhd" u32:len u16:format u16:nChannels u16:ppq
"FLdt" u32:len <TLV event stream>
```

Event encoding by opcode range: `0x00-0x3F` u8, `0x40-0x7F` u16le, `0x80-0xBF` u32le, `0xC0-0xFF` LEB128 varint length + blob. One violation: *25*'s `0xAC` sits in the u32 range but carries a fixed 3-byte payload.

## The transform, any newer version to 20.8

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
| `0xE9` clip records | 60 B (*21-24*) / 80 B (*25*) | 32 B | keep positions, lengths, and trims; reconcile end trims; move scale to `0xD7`; emulate fades |
| `0xEE` lane records | 70 bytes × 500 | 66 bytes × 500 | truncate each |
| `0xEB` route table | 1 byte (*25*) | 127 bytes | pad with zeros |
| `0xE1` param targets | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE3` link destination (offset 10) | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE1` table shape | tail records dropped | 4697 records | rebuild canonical table |
| `0x85` selected insert | out-of-range values occur | ≤ 126 | clamp to 126 |
| pattern time marker | `0x94` run inside the pattern block | absent | delete the run (see below) |
| stream tail | `0xE1 0x85 0xF3 0x2F` | `0xE1 0x85` | `0xF3`/`0x2F` are in the delete set |

The deleted event set is the opcode difference between the two truth files, plus `0xAC`, minus `0xD8`, plus the four marker sub-records: `0x29 0x2A 0x2B 0x2C 0x2D 0x2E 0x2F 0x30 0x31 0x32 0x33 0x65 0x67 0xA5 0xA6 0xA7 0xA8 0xA9 0xAA 0xAC 0xF2 0xF3`. Two genuine *20.8* saves proved that *20.8* writes `0xD8` (see below).

*25* addresses the mixer "current strip" as strip 501 (`0xE1` target `0xED40`). *20.8* addresses it as strip 126. The rebase clamps strip indexes above 126 to 126.

The canonical `0xE1` table is a header record, then per strip 0..126: ten slot pairs (pid 0 enabled, pid 1 mix), volume 192, pan 193, stereo separation 194, EQ 208-210 / 216-218 / 224-226, then a tail that shortens on high strips: sends 164-168 through strip 99, 168 only through 104, and 190 on every strip. Source values are kept where present. The program's defaults fill the rest.

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
- **`0xEA` points**: f64 delta position in beats, f64 value (0..1 of the 0..12800 knob), f32 tension, u32 flags. Flags: first point 0, middle points `0x02000000`, last point `0xFF000000`. The 17-byte header and the 112-byte tail are constant across every observed `0xEA`, in every version, whatever the point count. The fade shape uses tension -0.2807 (the value *20.8* writes for a default-looking fade). A zero-width final point restores the knob value so clips after a fade-out stay audible.
- **`0xE3` link**, channel volume form: u16 0 at 0, u16 automation channel index at 2, u16 0 at 8 (group 0 = channel space), u16 target channel index at 10, then `08 00 00 00 D5 01 00 00`.
- **`0xD8`**: one appended record per link — pid 0, group 0, target = channel index, value = the channel's `0xDB` volume.
- **Playlist record**: same pos/length as the audio clip, item index = automation channel, bytes 16-23 = `78 00 40 00 40 64 80 80`, f32 0.0 at 24, f32 length-in-beats at 28.

The automation value must equal the channel's own `0xDB` volume (default 10000/12800 = 0.78125) — automation replaces the knob, it does not scale it. Fade ms convert to beats through the project tempo (`0x9C` / 1000).

## Pattern time markers

A time marker is a `0x94` position, then `0x21` numerator, `0x22` denominator, and `0xCD` name. *21* and later add four sub-records to each marker: `0x2E`, `0x2D`, `0xA8`, and `0x65`.

*21* and later write one marker into every pattern metadata block. The block shape is `0x41` pattern number, optional `0xC1` name, `0x96` colour, `0x9D`, `0x9E`, then the marker run, then `0xA4`. The marker is auto-generated: five genuine *24.2* and *25.1* saves all hold position 0, flag `0x08000000`, 4/4, and the name "4/4". The four sub-records are zero in all of them.

*20.8* writes the pattern block as `0x41`, optional `0xC1`, `0x96`, `0x9D`, `0x9E`, `0xA4`. A genuine *20.8.4.2576* save with 52 pattern blocks and 2 time markers holds no `0x94` run in any pattern block. Its markers sit in the arrangement, after the `0xE9` playlist event, as `0x94 0x21 0x22 0xCD`.

The rules follow from that evidence. Delete `0x2D`, `0x2E`, `0x65`, and `0xA8` everywhere. Drop the whole marker run inside a pattern block. Keep arrangement markers. A dropped run that is not the default 4/4 loses a pattern time signature, so the converter warns.

## Version and structure are independent

The program's loaders select nothing from the claimed `0xC7` version. *24* rejects 66-byte lane records in a file that claims *21*, and accepts 70-byte lanes in the same file. A converter must rewrite structures, not headers.

## Safety gates

- The parser must reproduce the input byte-exact (`serialize(parse(x)) == x`) before the convert button enables. This proves no event was misread.
- Events that survive conversion but are outside the known *20.8* opcode set are reported as warnings, never deleted silently.
