set export
current_dir := `pwd`

RUSTFLAGS := "-D warnings"

# print help for Just targets
help:
    @just -l

# Build and run binary + args from any directory
[no-cd]
run *args:
    cargo run --manifest-path "${current_dir}/Cargo.toml" -- {{args}}

# Build
build *args:
    cargo build {{args}}

# Run Clippy to report and fix lints
clippy *args:
    cargo clippy {{args}} --color=always 2>&1 --tests | less -R

# Clean all artifacts
clean *args:
    cargo clean {{args}}
