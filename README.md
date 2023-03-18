# glicol-cli

https://user-images.githubusercontent.com/35621141/226138600-199aed46-4f16-4ae2-9951-716181782f59.mp4

## What's this?

It's a command line interface that you can use for music live coding with [Glicol](https://glicol.org).

It watches a file changes and then update the music in real-time.

## How to use it?

### Step 1

You need to have `cargo` installed (see [here](https://doc.rust-lang.org/cargo/getting-started/installation.html)).

### Step 2

In your Terminal:

```sh
cargo install --git https://github.com/glicol/glicol-cli.git
```

### Step 3

Create a new file called `test.glicol`, then run this command in your Terminal:

```sh
glicol-cli test.glicol
```

For more `OPTIONS`, call `--help` in your terminal:

```
~ glicol-cli --help                                    

Glicol cli tool. This tool will watch the changes in a .glicol file

Usage: glicol-cli [OPTIONS] <FILE>

Arguments:
  <FILE>  path to the .glicol file

Options:
  -b, --bpm <BPM>        Set beats per minute (BPM) [default: 120]
  -d, --device <DEVICE>  The audio device to use [default: default]
  -h, --help             Print help
  -V, --version          Print version
```

### Step 4

Start live coding. Edit `test.glicol` with your favourite editor:

```
// test.glicol
~t1: speed 4.0 >> seq 60 >> bd 0.2 >> mul 0.6
    
~t2: seq 33_33_ _33 33__33 _33
>> sawsynth 0.01 0.1
>> mul 0.5 >> lpf 1000.0 1.0

out: mix ~t.. >> plate 0.1
```

## Todos

- [x] Remove unnecessary print
- [ ] Support sample adding
- [ ] Error handling (so far when there is an error, the previous music continues but no report)
- [ ] Linux/Windows test; PR welcomed!
- [ ] Improve TUI
