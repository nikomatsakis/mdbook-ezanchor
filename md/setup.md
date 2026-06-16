# Setup

## Install

```bash
cargo install mdbook-ezanchor
```

## Configure your book

Add the preprocessor to your `book.toml`:

```toml
[preprocessor.anchor]
command = "mdbook-ezanchor"
scan-dirs = ["src", "examples"]
```

The `scan-dirs` field lists directories (relative to the book root) to scan for anchor markers.
If omitted, it defaults to `["src"]`.

### Optional fields

You can explicitly set the GitHub repo and branch if auto-detection doesn't work for your setup:

```toml
[preprocessor.anchor]
command = "mdbook-ezanchor"
scan-dirs = ["src"]
github-repo = "yourname/yourrepo"
github-branch = "main"
```

In most cases you don't need these — the preprocessor reads the git remote URL
and tracking branch automatically.

## Mark anchors in source code

In any source file within your scan directories, surround regions with `ANCHOR` comments:

```rs
// ANCHOR: my-example
fn hello() {
    println!("Hello from an anchor!");
}
// ANCHOR_END: my-example
```

The anchor name (`my-example` above) is what you'll reference from your markdown.
