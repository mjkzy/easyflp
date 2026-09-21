# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

easyflp is a Rust tool that parses FL Studio `.flp` / `.fst` files and backports projects saved by FL 21 to 26 into the byte-verified FL 20.0.5.681 layout. An experimental profile writes FL 10.0.9. The work is reverse engineering: every rewrite rule rests on a byte diff between two real saves of the same project. There is no format specification. The knowledge lives in `FORMAT.md`.

Read in this order before you change a converter:

1. `AGENTS.md` — invariants, the comment rule (default: no comment), and the technical writing style. Both rules bind every edit.
2. `FORMAT.md` — the transform tables, the evidence behind each rule, and the open questions.
3. The converter file you touch.

## Commands

Cargo lives at `%USERPROFILE%\.cargo\bin`. The `.bat` and `.sh` scripts add it to PATH. From Git Bash, run `export PATH="$HOME/.cargo/bin:$PATH"` first.

```bash
cargo build --release          # both binaries: target/release/easyflp(.exe), easyflp-gui(.exe)
cargo test                     # 29 unit tests, all in src/convert.rs, about 20 s cold
cargo test reconciles_wide     # one test by name substring
cargo test convert::tests::    # the whole module
build_and_run.bat              # kill the running GUI, build release, relaunch GUI (Windows)
```

CLI for scripted checks (exit code 0 on success, notes on `-` lines, warnings on `!` lines):

```bash
target/release/easyflp info <file.flp|.fst|.zip>
target/release/easyflp convert <file>            # writes <name>_easy.flp (v20.0.5) next to the input
target/release/easyflp convert --fl10 <file>     # writes <name>_easy10.flp (v10.0.9)
```

`info` prints `roundtrip  FAILED` when the parser cannot reproduce the input byte-exact. `convert` refuses that file.

There is no lint step. CI (`.github/workflows/release.yml`) builds on Windows and, on every push to `main`, deletes and recreates the GitHub release tagged `latest`. A push to `main` is a release.

## Architecture

### Data model (`src/flp.rs`)

A file is `FLhd` (format, nChannels, ppq) then `FLdt`, a flat TLV event stream. The opcode selects the payload width: `0x00-0x3F` u8, `0x40-0x7F` u16, `0x80-0xBF` u32, `0xC0-0xFF` varint length + blob. One exception: `0xAC` (FL 25+) is in the u32 range but carries 3 fixed bytes. `Flp` keeps `header_raw` and `trailing` so `serialize(parse(x)) == x` holds for every accepted file. `flp::op` names the opcodes the code addresses by name. Other opcodes appear as hex literals; that is deliberate, the hex is what `FORMAT.md` and the diff tools speak.

The stream has no tree. Structure is positional: a `0x40` starts a channel block, `0x41` a pattern block, `0x63` the arrangement, `0xE9` the playlist, and the mixer section is inserts closed by `0x62` slot markers. Every converter walks the stream once with a small section state machine and rewrites events in place.

### Pipeline

```
parse -> roundtrip gate -> info::extract -> ops::convert_and_write
                                              |
                       convert::to_fl208  (any 21+ save -> 20.8.4.2576 stream)   src/convert.rs
                                              |
                 +----------------------------+----------------------------+
                 v                                                         v
   convert200::fl208_to_fl200  (-> 20.0.5.681, the shipped v20 output)   convert10::fl20_to_fl10  (-> 10.0.9, experimental)
```

- `to_fl208` is the heart. It deletes the post-20 opcode set, rewrites routing, clips, lanes, `0xE1` mixer tables, sampler stretch time, and emulates clip fades as volume automation. A source already at major 20 skips it.
- `to_fl200` and `to_fl10` are the public entries. Each returns `Outcome { flp, notes, warnings }`. Notes count what changed. Warnings name what could not be converted. Unknown events that survive are warned about, never deleted.
- `src/ops.rs` is the single load/convert/write implementation. `src/cli.rs` and `src/gui/app.rs` are thin callers. Never put conversion logic in either.
- `src/wrapper.rs` parses the Fruity Wrapper `0xD5` state (`u32 version`, then `(u32 id, u64 len, payload)` chunks). Both profiles use it.
- `src/info.rs` extracts the viewer summary and owns `clip_record_size`, which infers the `0xE9` record width (32/60/80/88) from the claimed major version and the blob length.
- `src/package.rs` carries a `.zip` through with only the `.flp` entry replaced.

### Gate lists, delete sets, and where they live

| constant | file | meaning |
|---|---|---|
| `POST_FL20_OPS` | convert.rs | opcodes deleted on the way to 20.8 (empirical diff, see FORMAT.md) |
| `FL20_KNOWN_OPS` | convert.rs | every opcode a 20.8 save may hold; survivors outside it become warnings |
| `POST_FL200_OPS`, `FL200_EFFECTS`, `FL200_GENERATORS`, `NATIVE_UNCHANGED` | convert200.rs | 20.0.5 delete set and the plugin names 20.0.5 ships |
| `POST_FL10_OPS`, `FL10_KNOWN_OPS`, `FL10_EFFECTS`, `LEGACY_EFFECTS` | convert10.rs | the same for 10.0.9 |

Extend a delete set only with byte-level evidence from a real save. `0xD8` was once in `POST_FL20_OPS` and was wrong; two genuine 20.8 saves proved 20.8 writes it.

## Invariants (do not weaken)

- Conversion runs only when `serialize(parse(x)) == x`.
- The claimed `0xC7` version is not the gate. FL's loaders demand version-correct structures. Rewrite structures, never only headers.
- Plugin state (`0xD5`) is plugin-format dependent, not version dependent. Pass it through byte-exact. The only exceptions are the host fields of "Fruity Wrapper" and the specific native states that `FORMAT.md` lists with evidence. A native plugin's leading `u32` is its own data, not a version. Clamping it corrupts the plugin and FL 20.8 stops loading.
- Fruity Parametric EQ 2 has crashed users before. Its 20.0.5 form is exactly the one in `FORMAT.md`.

## Reverse-engineering workflow

### Evidence

One rule needs one truth pair: a project saved by the old version and the same project opened and saved unchanged by the new version. Save both from FL itself. The user creates these; the repository holds none (`.debug/` is gitignored scratch and may not exist in a clone). Ask for the truth files and their FL versions before you infer anything. A single pair gives one data point per field; say so in `FORMAT.md` when a mapping rests on one sample.

Known residue that never matches between a converted file and a truth save, and is not a bug: `0xED` / `0xE2` run-time bytes, window rectangles, the `0xEE` lane record count (a session property), the `0x85` clamp, `%USERPROFILE%` tokens in `0xC4` sample paths, and VST plugins' own chunk 53 growth.

### Dump and diff two saves

`easyflp info` is a summary, not a dump. For research, dump both streams to text and diff. Save this as `flpdump.py` in your scratch directory (the `.debug/` scripts are local copies of the same parser):

```python
import sys
def parse(path):
    d = open(path, 'rb').read()
    hlen = int.from_bytes(d[4:8], 'little'); h = d[8:8+hlen]; p0 = 8 + hlen
    dlen = int.from_bytes(d[p0+4:p0+8], 'little'); s = d[p0+8:p0+8+dlen]
    ev, p = [], 0
    while p < len(s):
        op = s[p]; p += 1
        if op == 0xAC: ev.append((op, 'fix3', s[p:p+3])); p += 3
        elif op < 0x40: ev.append((op, 'u8', s[p])); p += 1
        elif op < 0x80: ev.append((op, 'u16', int.from_bytes(s[p:p+2], 'little'))); p += 2
        elif op < 0xC0: ev.append((op, 'u32', int.from_bytes(s[p:p+4], 'little'))); p += 4
        else:
            n = sh = 0
            while True:
                b = s[p]; p += 1; n |= (b & 0x7F) << sh; sh += 7
                if not b & 0x80: break
            ev.append((op, 'blob', s[p:p+n])); p += n
    return h, ev
def text(b):
    u = []
    for i in range(0, len(b) - 1, 2):
        c = int.from_bytes(b[i:i+2], 'little')
        if c == 0: break
        u.append(c)
    return ''.join(map(chr, u)) if u and all(32 <= c < 127 for c in u) else ''
h, ev = parse(sys.argv[1])
print(f"format={h[0]} nch={int.from_bytes(h[2:4],'little')} ppq={int.from_bytes(h[4:6],'little')} events={len(ev)}")
for op, k, v in ev:
    if k == 'blob': print(f"{op:02X} blob len={len(v):6d} {v[:24].hex()} {text(v)}")
    else: print(f"{op:02X} {k}={v if k != 'fix3' else v.hex()}")
```

```bash
python flpdump.py old.flp > a.txt && python flpdump.py new.flp > b.txt
diff a.txt b.txt | head -80                                   # aligned event diff
diff <(cut -c1-2 a.txt | sort -u) <(cut -c1-2 b.txt | sort -u) # opcode set diff: what a version adds or drops
```

For one blob type, extract every record of that opcode from both files and compare offsets. Sizes come first: a changed blob length means a changed record layout. Then locate the differing bytes and match them to a knob the user changed on purpose. Change one thing per truth pair when you can.

### Add or change a rule

1. Establish the byte evidence with the dump above. Write down the version pair and the exact offsets.
2. Implement in the right stage. Anything a 21+ save adds goes in `convert.rs`. Anything 20.8 has and 20.0.5 lacks goes in `convert200.rs`. FL 10 differences go in `convert10.rs`.
3. Count the change and push a note or a warning. A user reads those lines to know what happened.
4. Update the gate list or delete set if a new opcode is involved.
5. Record the rule and its evidence in `FORMAT.md`, in the table of the stage it belongs to. A rule that is not in `FORMAT.md` will be "fixed" back by the next reader.
6. Add a unit test in `convert.rs` when the rule is computable from a synthetic stream (the existing tests build minimal `Flp` values by hand; copy that pattern).
7. Verify: `cargo test`, then `easyflp convert` on a real project, then open the output in the target FL version. Loading without an error dialog is the only proof. Diff the converted file against a genuine target-version save and confirm the only residue is the list above.

### Add a new FL version

A new major version usually changes one or two things and keeps the rest byte-identical (FL 26 grew the clip record by 8 bytes and added one header event). Steps: get a truth pair against the previous major, run the opcode set diff, extend `clip_record_size` if `0xE9` grew, extend `POST_FL20_OPS` with the new opcodes, and add a "The N layout" section to `FORMAT.md` that lists every difference. The converter output for the new save must match the output for the previous save except run-time bytes.

### Add a native plugin state rule

Plugins fail in the target version in one of three ways: a hang, a reset to default, or silent corruption. Get the plugin's `0xD5` from the newer save and from a target-version save of the converted project (FL rewrites the state on save). Diff those two. The 20.0.5 form is usually the newer body truncated plus a version word rewritten. Add the mapping in `native_state_fl200` (or `native_state_fl10`), keep every other plugin byte-exact, and list it in `FORMAT.md`. Unknown native states pass through and are named in one warning; never clamp a leading `u32` you have not verified.

## Pitfalls that cost past sessions

- FL reinterprets `0xD7` offset 96 as f32 from 24.2 and u32 before. Same unit (1/768 bar). The converter detects f32 by magnitude (`>= 0x10000000`). Do not rescale by PPQ.
- Clip stretch scale (`0xE9` offset 64, f64) folds into the channel stretch time. Do not retime clip lengths.
- FL 20.8 recomputes stretched audio clip lengths at load from the trim window. End trims are reconciled per channel from an interval estimate; a point estimate is biased.
- Pattern blocks in 21+ carry a default 4/4 time marker run. 20.8 keeps markers in the arrangement only. Drop the run in pattern blocks, keep it after `0xE9`.
- FL 25 addresses the mixer current strip as 501; 20.8 as 126. `0xE1` and `0xE3` targets rebase from `0x7000` to `0x2000` with stride `0x40`.
- A mixer preset (`FLhd` format `0x40`) holds one strip of 32 `0xE1` records. Rebuild that strip only; a full 4697-record table loads with strip 0 defaults.
- Every FL 10 text event is ANSI, not UTF-16. The FL 10 channel name is `0xC0`, which is a different event in FL 25.
