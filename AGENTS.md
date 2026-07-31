# What this is

a Rust cross-platform tool (Windows-first) that views `.flp` project information and backports v21/24/25 projects to 20.8's format. The CLI `easyflp.exe` is the main app. The GUI `easyflp-gui.exe` (eframe/egui) is a thin wrapper over the same library crate. `FORMAT.md` is the format knowledge — read it before touching `convert.rs`.

## Build / run / verify

```bat
build_and_run.bat
```

The script kills a running easyflp-gui.exe, builds Release (both binaries), and relaunches the GUI. Scripted checks use the CLI: `easyflp.exe convert <file>` (prints notes/warnings; exit code reports success) and `easyflp.exe info <file>` (prints project information). Legacy `--convert` / `--info` spellings still work.

Cargo lives at `%USERPROFILE%\.cargo\bin`; the .bat scripts add it to PATH.

## Repository map

- `src/lib.rs` — library crate root; both binaries build on it.
- `src/flp.rs` — TLV event stream parse/serialize, opcode constants, text decoding.
- `src/info.rs` — project information extraction for the viewer.
- `src/convert.rs` — the *20.8* retarget transform. The heart of the app.
- `src/package.rs` — zip read/write (a `.zip` input carries its non-flp entries through).
- `src/ops.rs` — shared load / convert / write operations; the single implementation both binaries call.
- `src/cli.rs` — the `easyflp` binary: `info`, `convert`, `gui` subcommands.
- `src/gui/main.rs` — the `easyflp-gui` binary entry point.
- `src/gui/app.rs` — the egui UI (drop zone, info panels, convert button).

## Invariants

- **Roundtrip gate**: conversion is allowed only when `serialize(parse(x)) == x`. Never
  weaken this check.
- **The claimed version is not the gate.** The program's loaders demand version-correct
  *structures* regardless of the `0xC7` string. Rewrite structures, never just headers.
- The post-20 delete set in `convert.rs` is empirical (opcode diff of the two truth
  files). Extend it only with byte-level evidence from real program saves.
- Unknown events that survive conversion are warned about, never silently deleted.
- Wrapper records (`0xD5` sub-records, `0xD4` field B) are plugin-format dependent, not
  version dependent. The converter must carry them through unchanged (marker aside).

## Comments — the rule

Default: no comment. Code must explain itself through naming and structure. A comment is justified only when it carries information unrecoverable from the code: a non-obvious external constraint (specific version format behaviour), a deliberate deviation a reader would want to "fix" back, or a reference that saves a research session. Never write restatements, section banners, or edit narration. If unsure whether a comment qualifies, it does not.

## Technical writing

Technical text: ASD-STE100 style. Max 20 words per sentence in instructions, 25 in descriptions. Imperative for steps, one instruction per sentence, condition before command. Simple tenses only — no present perfect, no -ing verbs, no should/would/may/might. Active voice. One word per meaning — no synonym rotation. No contractions, keep articles and "that". Delete filler: simply, robust, seamlessly, leverage. Code and identifiers stay exact.