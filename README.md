# glicol-cli

https://user-images.githubusercontent.com/35621141/225968990-2d1fea27-8560-4f41-ae0e-8e6edea15000.mp4

## What's this?

It's a command line interface that you can use for music live coding with [Glicol](https://glicol.org).

It watches a file changes and then update the music in real-time.

## How to use it?

First, you need to have `cargo` installed (see [here](https://doc.rust-lang.org/cargo/getting-started/installation.html)).

Then, in your Terminal:

```sh
cargo install --git https://github.com/glicol/glicol-cli.git
```

Create a new file called `1.glicol`, then run this command in your Terminal:
```sh
glicol-cli test.glicol
```

Edit `test.glicol` with your favourite editor:

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
