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
| `0x85` selected insert | out-of-range values occur | ≤ 126 | clamp to 126 |
| pattern time marker | `0x94` run inside the pattern block | absent | delete the run (see below) |
| stream tail | `0xE1 0x85 0xF3 0x2F` | `0xE1 0x85` | `0xF3`/`0x2F` are in the delete set |

The deleted event set is the opcode difference between the two truth files, plus `0xAC`, minus `0xD8`, plus the four marker sub-records and the *25.2* additions: `0x29 0x2A 0x2B 0x2C 0x2D 0x2E 0x2F 0x30 0x31 0x32 0x33 0x34 0x65 0x67 0xA5 0xA6 0xA7 0xA8 0xA9 0xAA 0xAB 0xAC 0xC0 0xF2 0xF3 0xFC 0xFD`. Two genuine *20.8* saves proved that *20.8* writes `0xD8` (see below). `0xC0` carries the project's "FL Studio" name string; `0x34`, `0xAB`, `0xFC`, `0xFD` are per-pattern metadata, all zero in the observed *25.2.4* save.

*25* addresses the mixer "current strip" as strip 501 (`0xE1` target `0xED40`). *20.8* addresses it as strip 126. The rebase clamps strip indexes above 126 to 126.

The canonical `0xE1` table is a header record, then per strip 0..126: ten slot pairs (pid 0 enabled, pid 1 mix), volume 192, pan 193, stereo separation 194, EQ 208-210 / 216-218 / 224-226, then a tail that shortens on high strips: sends 164-168 through strip 99, 168 only through 104, and 190 on every strip. Source values are kept where present. The program's defaults fill the rest.

## The 26 layout

One truth pair: a project saved by *25.2.4.5242* and the same project saved by *26.1.5.5618*. The event streams are identical except for four things. The `0xE9` clip record grows from 80 to 88 bytes: the first 72 bytes keep the *25* layout, the new bytes at 72..80 are `00 00 00 00 FF FF FF FF` on every clip, and the last 8 bytes are the zero tail of the *25* record. The header holds a second `0xAC` (`00 01 00`) after `0xC3`, followed by a `0x00` u8 event with value 0. `0xA9` is 15 instead of 7. `0xF2` byte 11 is `0x40` instead of 0. Every `0xD5` state, `0xD7` blob, `0xEE` lane record, `0xEB` table, and `0xE1` record is byte-identical to the *25* save, so the 20.8 transform applies unchanged beyond the clip size and the header `0x00`. The converter output for the *26* save matches the output for the *25* save except the `0xED` run-time bytes.

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

Fruity Parametric EQ 2 keeps its state unchanged. Its *20.0*-era state size is unverified, and a changed state made the plugin stop for users of the 10 profile. Every unverified native state is listed in one warning.

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

## Version and structure are independent

The program's loaders select nothing from the claimed `0xC7` version. *24* rejects 66-byte lane records in a file that claims *21*, and accepts 70-byte lanes in the same file. A converter must rewrite structures, not headers.

## Safety gates

- The parser must reproduce the input byte-exact (`serialize(parse(x)) == x`) before the convert button enables. This proves no event was misread.
- Events that survive conversion but are outside the known *20.8* opcode set are reported as warnings, never deleted silently. The *20.0.5* gate is the same set minus `0x26 0x27 0x28`. The 10 profile applies the same gate against the *10.0.9* opcode set.
