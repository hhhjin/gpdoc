---
name: gpdoc-commits
description: Apply gpdoc conventions when writing or reviewing commit messages and PR titles for squash merges.
---

# gpdoc commits

## Classify the change

- Use `feat` for new capabilities and `fix` for corrections to existing behavior. For other changes, choose the type that describes the work, such as `docs`, `ci`, `test`, `build`, or `refactor`.
- Add `!` before the colon on any type that breaks a supported contract. Check public Rust APIs, Cargo features, documented editor behavior, and saved document formats. In a `BREAKING CHANGE:` footer of the final commit, name the affected API or behavior and explain how to migrate.

## Write the title that lands on main

- Use an English Conventional Commit title with an optional component scope, such as `fix(bold): preserve the selection when toggling bold` or `feat(drag-handle)!: support dragging blocks across containers`. For a squash merge, use this format for the PR title and include any migration instructions in the squash commit's `BREAKING CHANGE:` footer.
- Describe what the change does in the title so it makes sense on its own.
- Keep the implementation, affected callers, and relevant tests together. When practical, give independent changes their own commits or squash PRs.
