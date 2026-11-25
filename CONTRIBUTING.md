# Contributing to gpdoc

Use English for documentation, issues, and pull requests. Keep multilingual test cases in their original language when they exercise text input or Unicode behavior.

Run the Rust checks from the workspace root with the current stable Rust toolchain. On macOS, the GPUI build also needs Xcode and its Metal toolchain.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
```

Run `cargo test --locked -p gpdoc-gpui-tests --features native-tests --test plugins` to test plugin interactions on the GPUI test platform. On macOS, omit `--test plugins` to also exercise Command shortcuts and text geometry in native windows.

To run the desktop example, use `cargo run -p gpdoc-editor-demo`.

For changes to editing behavior, add a regression case that shows what the user types or which command they run. To check Tiptap compatibility, run `bun install --frozen-lockfile` and `bun run test` in `tests/tiptap`.

In your pull request, describe the user-visible change and list the checks you ran.

## Choose a target branch

Open editor, plugin, and feature documentation PRs against `next`. Site styling changes can go directly to `main`.

## Write a PR title

Write the PR title in English using `type(scope): description`. The scope is optional. Describe what changes for the user, for example `fix(bold): preserve the selection when toggling bold`.
