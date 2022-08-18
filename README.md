# pre-commit-rust

[pre-commit](https://pre-commit.com/) hooks for running `cargo fmt`, `cargo check`, and `cargo clippy`.

Unlike other existing hooks, these are designed to also work in a repo that contains multiple disconnected and arbitrarily nested rust projects that don't belong to the same workspace.
