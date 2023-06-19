# multichecks

Run multiple commands in parallel. If all of them succeed, `multichecks` succeeds; otherwise, it fails, and prints stderr and stdout for each command that failed.

For example:

    cargo test: ⠋
    cargo check: OK
    cargo fmt --check: FAILED
    │ Diff in .../multichecks/src/main.rs at line 3:
    │ -use std::process::{    Command, Stdio};
    │ +use std::process::{Command, Stdio};

In this example, `cargo test` is still executing (it shows an animated spinner), `cargo check` completed successfully, and `cargo fmt --check` failed.

## Installing

    cargo install --git https://github.com/yshavit/multichecks

## Using

To use multichecks, simply pipe in a series of commands, one per line. Multichecks does very simple parsing (for now): it always splits on spaces, and does not do any shell expansion.

I suggest using HEREDOCs:

    multichecks <<EOF
      cargo check
      cargo test
      cargo fmt --check
    EOF
