# mdbook-ezanchor

`mdbook-ezanchor` is an mdbook preprocessor that lets you reference
anchored regions of source code from your markdown.
Instead of copy-pasting code snippets that go stale,
you mark regions in your source with `ANCHOR` comments
and reference them from your book.
The preprocessor expands them into code blocks
with links back to the source on GitHub.

## Features

- **Block anchors** — embed a full code snippet inline
- **Inline anchors** — link to a source location by name
- **Auto-detection** — determines the GitHub repo and branch from your git remote
- **Dedenting** — strips common leading whitespace so indented code looks natural
