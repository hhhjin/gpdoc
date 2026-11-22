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

## Document a user-visible change

Add an English bullet under `## Unreleased` in each affected crate's `CHANGELOG.md`. Describe what changes for someone using that crate. For breaking changes, explain how to migrate. For example, a fix to the Bold plugin belongs in [its changelog](plugins/bold/CHANGELOG.md).

Use Conventional Commit titles such as `fix(bold): preserve the selection` or `feat(core)!: change the command signature` for commits that land on `main`. With squash merges, use the PR title as the commit title. Release-plz uses commit history and supported public API checks to propose versions for the maintainer to review.
