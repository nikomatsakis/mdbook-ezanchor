# Usage

There are two ways to reference anchors from your markdown: block anchors and inline anchors.

## Block anchors

A block anchor expands into a fenced code block with the snippet contents,
followed by a link to the source file on GitHub.

Write it like this in your markdown:

````
```{anchor}
anchor-name
```
````

For example, here is the CLI struct from `mdbook-ezanchor`'s own source:

```{anchor}
cli-struct
```

## Inline anchors

An inline anchor expands into a markdown link showing the file path and line range.
Use it when you want to point readers to a location without embedding the full code.

Write it like this:

```text
See {anchor}`anchor-name` for details.
```

For example, the dedent logic lives at {anchor}`dedent-fn`.

## How anchors are scanned

The preprocessor walks each directory listed in `scan-dirs`
and looks for files with these extensions: `.rs`, `.toml`, `.json`, `.yaml`, `.yml`, `.ts`, `.js`.

Here's the scanning logic itself:

```{anchor}
scan-loop
```

## Supported anchor comment syntax

Anchors are delimited by comments of the form:

```
// ANCHOR: name
...
// ANCHOR_END: name
```

Names must start with a word character and can contain word characters and hyphens (`\w[\w-]*`).
