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
  -H, --headless         Disable the TUI
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

## Load your own samples

Run the line in your terminal first:

`export GLICOL_CLI_SAMPLES_PATH=/path/to/your/samples`

For example:

`export GLICOL_CLI_SAMPLES_PATH=~/Downloads/samples`

## Development

If you are developing the glicol-cli source code itself, you can setup
a convenient self-recompiling debug binary alias:

 * Install
[Just](https://github.com/casey/just?tab=readme-ov-file#readme)

```sh
cargo install just
```

 * Put the alias into your shell init (`~/.bashrc`):

```
# Point this to the Justfile found in your git clone:
alias glicol-cli='just -f ~/git/vendor/glicol/glicol-cli/Justfile run'
```

This special `glicol-cli` alias can be run from any directory, and the
program will automatically recompile itself before running the
program. You can still provide `glicol-cli` command line arguments as
normal.
