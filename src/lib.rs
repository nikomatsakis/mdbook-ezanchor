use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use anyhow::Result;
use clap::{Parser, Subcommand};
use mdbook_preprocessor::book::{Book, BookItem};
use mdbook_preprocessor::{Preprocessor, PreprocessorContext, parse_input};
use regex::Regex;
use walkdir::WalkDir;

// ANCHOR: cli-struct
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Supports { renderer: String },
}
// ANCHOR_END: cli-struct

pub fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Supports { renderer }) => {
            if renderer == "html" || renderer == "markdown" {
                process::exit(0);
            } else {
                process::exit(1);
            }
        }
        None => {
            let (ctx, book) = parse_input(io::stdin())?;
            let preprocessor = AnchorPreprocessor;
            let processed = preprocessor.run(&ctx, book)?;
            serde_json::to_writer(io::stdout(), &processed)?;
            Ok(())
        }
    }
}

struct AnchorPreprocessor;

impl Preprocessor for AnchorPreprocessor {
    fn name(&self) -> &str {
        "anchor"
    }

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let config = Config::from_context(ctx)?;
        let anchors = scan_anchors(&config.root, &config.scan_dirs)?;

        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                chapter.content = expand_anchors(&chapter.content, &anchors, &config);
            }
        });

        Ok(book)
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PreprocessorConfig {
    #[serde(default)]
    scan_dirs: Vec<String>,
    #[serde(default)]
    github_repo: Option<String>,
    #[serde(default)]
    github_branch: Option<String>,
}

#[derive(Debug)]
struct Config {
    root: PathBuf,
    scan_dirs: Vec<PathBuf>,
    github_repo: String,
    github_ref: String,
}

impl Config {
    fn from_context(ctx: &PreprocessorContext) -> Result<Self> {
        let root = ctx.root.clone();

        let pp_config: PreprocessorConfig =
            ctx.config
                .get("preprocessor.anchor")?
                .unwrap_or(PreprocessorConfig {
                    scan_dirs: vec![],
                    github_repo: None,
                    github_branch: None,
                });

        let scan_dirs = if pp_config.scan_dirs.is_empty() {
            vec![root.join("src")]
        } else {
            pp_config.scan_dirs.iter().map(|s| root.join(s)).collect()
        };

        let (repo, git_ref) = detect_git_context(&root, &pp_config);

        Ok(Config {
            root,
            scan_dirs,
            github_repo: repo,
            github_ref: git_ref,
        })
    }
}

fn detect_git_context(root: &Path, fallback: &PreprocessorConfig) -> (String, String) {
    let tracking = tracking_remote_and_branch(root);

    let (remote_url, branch) = match &tracking {
        Some((url, branch)) => (Some(url.clone()), Some(branch.clone())),
        None => (origin_remote_url(root), None),
    };

    let repo = remote_url
        .and_then(|url| parse_github_repo(&url))
        .or_else(|| fallback.github_repo.clone())
        .unwrap_or_else(|| "UNKNOWN/REPO".to_string());

    let git_ref = branch
        .or_else(|| fallback.github_branch.clone())
        .unwrap_or_else(|| "main".to_string());

    (repo, git_ref)
}

fn tracking_remote_and_branch(root: &Path) -> Option<(String, String)> {
    let upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())?;

    let (remote, branch) = upstream.split_once('/')?;
    let url = remote_url_by_name(root, remote)?;
    Some((url, branch.to_string()))
}

fn origin_remote_url(root: &Path) -> Option<String> {
    remote_url_by_name(root, "origin")
}

fn remote_url_by_name(root: &Path, name: &str) -> Option<String> {
    std::process::Command::new("git")
        .args(["remote", "get-url", name])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn parse_github_repo(url: &str) -> Option<String> {
    let path = if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else if url.contains("github.com/") {
        url.split("github.com/").nth(1)?
    } else {
        return None;
    };
    let path = path.strip_suffix(".git").unwrap_or(path);
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() >= 2 {
        Some(format!("{}/{}", parts[0], parts[1]))
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct Anchor {
    file: PathBuf,
    line_start: usize,
    line_end: usize,
    content: String,
}

impl Anchor {
    fn relative_path(&self, root: &Path) -> String {
        self.file
            .strip_prefix(root)
            .unwrap_or(&self.file)
            .to_string_lossy()
            .into_owned()
    }

    fn github_url(&self, config: &Config) -> String {
        let rel = self.relative_path(&config.root);
        format!(
            "https://github.com/{}/blob/{}/{}#L{}-L{}",
            config.github_repo, config.github_ref, rel, self.line_start, self.line_end,
        )
    }

    fn file_extension(&self) -> &str {
        self.file.extension().and_then(|e| e.to_str()).unwrap_or("")
    }
}

fn scan_anchors(root: &Path, scan_dirs: &[PathBuf]) -> Result<HashMap<String, Anchor>> {
    let mut anchors = HashMap::new();
    let anchor_start = Regex::new(r"//\s*ANCHOR:\s*(\w[\w-]*)").unwrap();
    let anchor_end = Regex::new(r"//\s*ANCHOR_END:\s*(\w[\w-]*)").unwrap();

    // ANCHOR: scan-loop
    for dir in scan_dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "rs" | "toml" | "json" | "yaml" | "yml" | "ts" | "js") {
                continue;
            }
            // ANCHOR_END: scan-loop

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut open_anchors: HashMap<String, (usize, Vec<String>)> = HashMap::new();

            for (line_num, line) in content.lines().enumerate() {
                let line_1based = line_num + 1;

                if let Some(caps) = anchor_start.captures(line) {
                    let name = caps[1].to_string();
                    open_anchors.insert(name, (line_1based, Vec::new()));
                    continue;
                }

                if let Some(caps) = anchor_end.captures(line) {
                    let name = caps[1].to_string();
                    if let Some((start_line, lines)) = open_anchors.remove(&name) {
                        let anchor = Anchor {
                            file: path.to_path_buf(),
                            line_start: start_line + 1,
                            line_end: line_1based - 1,
                            content: dedent(&lines),
                        };
                        if anchors.contains_key(&name) {
                            eprintln!(
                                "warning: duplicate anchor `{}` in {}",
                                name,
                                path.strip_prefix(root).unwrap_or(path).display()
                            );
                        }
                        anchors.insert(name, anchor);
                    }
                    continue;
                }

                for (_, lines) in open_anchors.values_mut() {
                    lines.push(line.to_string());
                }
            }

            for (name, _) in open_anchors {
                eprintln!(
                    "warning: unclosed anchor `{}` in {}",
                    name,
                    path.strip_prefix(root).unwrap_or(path).display()
                );
            }
        }
    }

    Ok(anchors)
}

// ANCHOR: dedent-fn
fn dedent(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
// ANCHOR_END: dedent-fn

fn expand_anchors(content: &str, anchors: &HashMap<String, Anchor>, config: &Config) -> String {
    let mut result = String::new();
    let mut lines = content.lines();
    let mut pending_nonfenced = String::new();

    #[allow(clippy::while_let_on_iterator)]
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let backtick_count = trimmed.len() - trimmed.trim_start_matches('`').len();

        if backtick_count >= 3 && !trimmed[backtick_count..].starts_with('`') {
            let fence = &trimmed[..backtick_count];
            let info = trimmed[backtick_count..].trim();

            let mut fence_body = String::new();
            fence_body.push_str(line);
            fence_body.push('\n');

            let mut closed = false;
            for inner in lines.by_ref() {
                fence_body.push_str(inner);
                fence_body.push('\n');
                let inner_trimmed = inner.trim_start();
                if inner_trimmed.starts_with(fence)
                    && inner_trimmed[fence.len()..].trim().is_empty()
                {
                    closed = true;
                    break;
                }
            }

            result.push_str(&expand_nonfenced(&pending_nonfenced, anchors, config));
            pending_nonfenced.clear();

            if backtick_count == 3 && info.starts_with("{anchor}") && closed {
                result.push_str(&expand_single_block_anchor(&fence_body, anchors, config));
            } else {
                result.push_str(&fence_body);
            }
        } else {
            pending_nonfenced.push_str(line);
            pending_nonfenced.push('\n');
        }
    }

    if !content.ends_with('\n') && pending_nonfenced.ends_with('\n') {
        pending_nonfenced.pop();
    }

    result.push_str(&expand_nonfenced(&pending_nonfenced, anchors, config));
    result
}

fn expand_nonfenced(content: &str, anchors: &HashMap<String, Anchor>, config: &Config) -> String {
    let re = Regex::new(r"\{anchor\}`([^`]+)`").unwrap();
    re.replace_all(content, |caps: &regex::Captures| {
        let name = &caps[1];
        match anchors.get(name) {
            Some(anchor) => {
                let rel = anchor.relative_path(&config.root);
                let url = anchor.github_url(config);
                format!(
                    "[`{}:{}-{}`]({})",
                    rel, anchor.line_start, anchor.line_end, url,
                )
            }
            None => {
                eprintln!("warning: unknown anchor `{name}`");
                format!("**⚠️ unknown anchor `{name}`**")
            }
        }
    })
    .into_owned()
}

fn expand_single_block_anchor(
    fence_text: &str,
    anchors: &HashMap<String, Anchor>,
    config: &Config,
) -> String {
    let body = fence_text.strip_prefix("```{anchor}").unwrap_or("").trim();
    let body = body.strip_suffix("```").unwrap_or(body).trim();
    let name = body.lines().next().unwrap_or("").trim();
    match anchors.get(name) {
        Some(anchor) => {
            let rel = anchor.relative_path(&config.root);
            let url = anchor.github_url(config);
            let lang = anchor.file_extension();
            format!(
                "```{lang}\n{content}\n```\n\n*[`{rel}:{start}-{end}`]({url})*",
                content = anchor.content,
                start = anchor.line_start,
                end = anchor.line_end,
            )
        }
        None => {
            eprintln!("warning: unknown anchor `{name}`");
            format!("**⚠️ unknown anchor `{name}`**")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            root: PathBuf::from("/project"),
            scan_dirs: vec![],
            github_repo: "user/repo".to_string(),
            github_ref: "main".to_string(),
        }
    }

    fn test_anchors() -> HashMap<String, Anchor> {
        let mut m = HashMap::new();
        m.insert(
            "foo".to_string(),
            Anchor {
                file: PathBuf::from("/project/src/lib.rs"),
                line_start: 10,
                line_end: 15,
                content: "fn foo() {\n    println!(\"hello\");\n}".to_string(),
            },
        );
        m
    }

    #[test]
    fn inline_expansion() {
        let anchors = test_anchors();
        let config = test_config();
        let input = "See {anchor}`foo` for details.";
        let output = expand_anchors(input, &anchors, &config);
        assert!(output.contains("[`src/lib.rs:10-15`]"));
        assert!(output.contains("https://github.com/user/repo/blob/main/src/lib.rs#L10-L15"));
    }

    #[test]
    fn block_expansion() {
        let anchors = test_anchors();
        let config = test_config();
        let input = "```{anchor}\nfoo\n```";
        let output = expand_anchors(input, &anchors, &config);
        assert!(output.contains("```rs"));
        assert!(output.contains("fn foo()"));
        assert!(output.contains("src/lib.rs:10-15"));
    }

    #[test]
    fn unknown_anchor_inline() {
        let anchors = HashMap::new();
        let config = test_config();
        let input = "{anchor}`missing`";
        let output = expand_anchors(input, &anchors, &config);
        assert!(output.contains("unknown anchor"));
    }

    #[test]
    fn fenced_code_not_expanded() {
        let anchors = test_anchors();
        let config = test_config();
        let input = "````\n```{anchor}\nfoo\n```\n````\n";
        let output = expand_anchors(input, &anchors, &config);
        assert!(output.contains("```{anchor}"));
        assert!(!output.contains("fn foo()"));
    }

    #[test]
    fn dedent_removes_common_whitespace() {
        let lines = vec![
            "        fn bar() {".to_string(),
            "            42".to_string(),
            "        }".to_string(),
        ];
        let result = dedent(&lines);
        assert_eq!(result, "fn bar() {\n    42\n}");
    }

    #[test]
    fn parse_github_repo_ssh() {
        assert_eq!(
            parse_github_repo("git@github.com:sparkle-ai-space/jamsession.git"),
            Some("sparkle-ai-space/jamsession".to_string())
        );
    }

    #[test]
    fn parse_github_repo_https() {
        assert_eq!(
            parse_github_repo("https://github.com/nikomatsakis/jamsession.git"),
            Some("nikomatsakis/jamsession".to_string())
        );
    }

    #[test]
    fn parse_github_repo_no_suffix() {
        assert_eq!(
            parse_github_repo("https://github.com/user/repo"),
            Some("user/repo".to_string())
        );
    }
}
