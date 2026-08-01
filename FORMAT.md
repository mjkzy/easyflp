# FLP conversion knowledge

Plugin wrapper records (`0xD5` records 30/32/50/51/52/57, and `0xD4` field B) are byte-identical across versions. They depend on the plugin format, not the program version. The converter carries them through unchanged.

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
| `0xD5` wrapper marker | 12 | 10 | rewrite first u32 |
| `0xD7` channel blob | 168 bytes | 158 bytes | truncate (leading bytes agree) |
| `0xD7` stretch time (offset 96) | f32 ticks (*25*) | u32, 1/768 bar | reinterpret and rescale (see below) |
| `0xE9` clip records | 60 B (*21-24*) / 80 B (*25*) | 32 B | keep bytes 0-31; emulate fades (see below) |
| `0xEE` lane records | 70 bytes × 500 | 66 bytes × 500 | truncate each |
| `0xEB` route table | 1 byte (*25*) | 127 bytes | pad with zeros |
| `0xE1` param targets | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE3` link destination (offset 10) | base `0x7000` (*25*) | base `0x2000` | rebase, stride `0x40` |
| `0xE1` table shape | tail records dropped | 4697 records | rebuild canonical table |
| `0x85` selected insert | out-of-range values occur | ≤ 126 | clamp to 126 |
| stream tail | `0xE1 0x85 0xF3 0x2F` | `0xE1 0x85` | `0xF3`/`0x2F` are in the delete set |

The deleted event set is the opcode difference between the two truth files, plus `0xAC`, minus `0xD8`: `0x29 0x2A 0x2B 0x2C 0x2F 0x30 0x31 0x32 0x33 0x67 0xA5 0xA6 0xA7 0xA9 0xAA 0xAC 0xF2 0xF3`. Two genuine *20.8* saves proved that *20.8* writes `0xD8` (see below).

*25* addresses the mixer "current strip" as strip 501 (`0xE1` target `0xED40`). *20.8* addresses it as strip 126. The rebase clamps strip indexes above 126 to 126.

The canonical `0xE1` table is a header record, then per strip 0..126: ten slot pairs (pid 0 enabled, pid 1 mix), volume 192, pan 193, stereo separation 194, EQ 208-210 / 216-218 / 224-226, then a tail that shortens on high strips: sends 164-168 through strip 99, 168 only through 104, and 190 on every strip. Source values are kept where present. The program's defaults fill the rest.

## 0xD7 sampler stretch time

`0xD7` offset 96 holds the sampler "time stretching: time" value. *20.8* reads a `u32le` in units of 1/768 bar (PyFLP `LinearMusical`: bars×768 + steps×48, one unit = 5 ticks at the internal PPQ 960). *25* writes an `f32le` tick count (project PPQ) in the same four bytes.

Left as-is, *20.8* reads the f32 bit pattern as a u32 (about 1.1e9), stretches the sample to millions of bars, and resizes every playlist clip of that channel to match. The converter rescales: `u32 = round(f32 × 192 / ppq)`. The two encodings never overlap: a real u32 stays below `0x1000000` for any real song, and an f32 of one tick or more has bits ≥ `0x3F800000`. The converter treats values ≥ `0x10000000` as f32.

## Clip fades (21+) and the channel-volume emulation

The 80-byte clip record tail (*25*): u32 clip uid at 32, f32 fade-in ms at 40, f32 fade-out ms at 44, f32 gain at 52 (1.0 default), u32 fade flags at 56 (2 observed on faded clips), f64 stretch scale at 64. Offsets 24-31 keep the v20 meaning (f32 start/end trim, ms for audio clips, -1 = untrimmed).

v20 has no fade fields. The converter emulates each distinct (channel, length, fade-in, fade-out) clip shape with one automation channel linked to the audio channel's volume, plus one playlist clip per faded instance on a free lane. All template bytes come from a genuine *20.8.4.2576* save of exactly this construction:

- **Channel block**: kind (`0x15`) = 5, standard channel param run, `0xEA` curve, `0x8F` = 3, `0x91` = 1. Inserted before the first `0x63`. `FLhd` nChannels counts it.
- **`0xEA` points**: f64 delta position in beats, f64 value (0..1 of the 0..12800 knob), f32 tension, u32 flags. Flags: first point 0, middle points `0x02000000`, last point `0xFF000000`. The 17-byte header and the 112-byte tail are constant across every observed `0xEA`, in every version, whatever the point count. The fade shape uses tension -0.2807 (the value *20.8* writes for a default-looking fade). A zero-width final point restores the knob value so clips after a fade-out stay audible.
- **`0xE3` link**, channel volume form: u16 0 at 0, u16 automation channel index at 2, u16 0 at 8 (group 0 = channel space), u16 target channel index at 10, then `08 00 00 00 D5 01 00 00`.
- **`0xD8`**: one appended record per link — pid 0, group 0, target = channel index, value = the channel's `0xDB` volume.
- **Playlist record**: same pos/length as the audio clip, item index = automation channel, bytes 16-23 = `78 00 40 00 40 64 80 80`, f32 0.0 at 24, f32 length-in-beats at 28.

The automation value must equal the channel's own `0xDB` volume (default 10000/12800 = 0.78125) — automation replaces the knob, it does not scale it. Fade ms convert to beats through the project tempo (`0x9C` / 1000).
## Version and structure are independent

The program's loaders select nothing from the claimed `0xC7` version. *24* rejects 66-byte lane records in a file that claims *21*, and accepts 70-byte lanes in the same file. A converter must rewrite structures, not headers.

## Safety gates

- The parser must reproduce the input byte-exact (`serialize(parse(x)) == x`) before the convert button enables. This proves no event was misread.
- Events that survive conversion but are outside the known *20.8* opcode set are reported as warnings, never deleted silently.
