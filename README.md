# SPT

A simple **Sp**eed **T**est CLI.

## Build

```rust
cargo build --release
```

## Usage

Pass URLs

```bash
❯ spt https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240 
==> GET https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240
HTTP/1.1 200 OK 117.01561ms
  [00:00:00] [####################] 100% (18.88 MiB/s, 0s)

╭──────────────────────────────────────────────────────────────────┬─────────────╮
│ URL                                                              │ Speed       │
╞══════════════════════════════════════════════════════════════════╪═════════════╡
│ https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240 │ 18.90 MiB/s │
╰──────────────────────────────────────────────────────────────────┴─────────────╯
```

Or read URLs from file:

```plaintext
# in.txt
# Comments are allowed (line starts with // or #)
https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240
GET https://upos-sz-mirrorcos.bilivideo.com/_probe_/size_kbyte/10240
```

```bash
❯ spt -f ./in.txt 
==> GET https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240
HTTP/1.1 200 OK 51.049333ms
  [00:00:00] [####################] 100% (19.52 MiB/s, 0s)

==> GET https://upos-sz-mirrorcos.bilivideo.com/_probe_/size_kbyte/10240
HTTP/2.0 200 OK 342.579522ms
  [00:00:09] [####################] 100% (1.07 MiB/s, 0s)

╭──────────────────────────────────────────────────────────────────┬─────────────╮
│ URL                                                              │ Speed       │
╞══════════════════════════════════════════════════════════════════╪═════════════╡
│ https://upos-sz-mirrorali.bilivideo.com/_probe_/size_kbyte/10240 │ 19.53 MiB/s │
├──────────────────────────────────────────────────────────────────┼─────────────┤
│ https://upos-sz-mirrorcos.bilivideo.com/_probe_/size_kbyte/10240 │ 1.07 MiB/s  │
╰──────────────────────────────────────────────────────────────────┴─────────────╯
```
