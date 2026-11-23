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

## Write commits for release notes

Release-plz generates each crate's `CHANGELOG.md` in the release PR from the commits that land on `main`. You can skip changelog edits in a feature or bug-fix PR.

Write Conventional Commit titles in English using `type(scope): description`. Describe what changes for the user.

- **Squash merge.** Use this format for the PR title. Individual commits on your branch can use any title. The maintainer can edit the final squash commit title at merge time.
- **Merge or rebase that preserves individual commits.** Use this format for each commit that lands on `main`.

Choose the type that describes the change.

- Use `feat` for a new capability, for example `feat(table): add a command to split merged cells`.
- Use `fix` for a bug fix, for example `fix(bold): preserve the selection when toggling bold`.
- Use `docs` for documentation and `ci` for workflow changes. The scope in parentheses is optional.
- For a breaking change, add `!` before the colon and explain the migration in a `BREAKING CHANGE:` footer.

Review the proposed versions and changelog entries in the release PR. Add any migration steps that users need to the generated entry before merging.
