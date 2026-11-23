# Project overview

gpdoc is a headless rich-text editor framework for Rust with GPUI support.

# Repository layout

- `crates/gpdoc/examples/editor/` contains the runnable GPUI editor demo.
- `crates/` contains the core editing engine, shared editing support, and the GPUI editor library.
- `plugins/` contains Rust crates for individual editor features such as tables, mentions, and Markdown.
- `web/` contains the Astro documentation site and the browser playground.

# Pull requests and release notes

- Write titles in English as `type(scope): description`, for example `fix(bold): preserve the selection when toggling bold`. The scope is optional. Use `feat` for features, `fix` for bug fixes, `docs` for documentation, and `ci` for workflow changes.
- For a squash merge, use this format for the PR title. Branch commits can use any title. When a merge or rebase preserves individual commits, apply the format to each commit that lands on `main`.
- For a breaking change, add `!` before the colon and explain how to migrate in a `BREAKING CHANGE:` footer of the final commit message.
- In the PR description, summarize the user-visible change and list the checks you ran.
- Release-plz generates crate changelogs in the release PR. Feature and bug-fix PRs can skip changelog edits. When reviewing a release PR, check the proposed versions and add any migration steps users need.
