//! Discourse markdown 到纯文本的简易降级渲染。
//!
//! 用途：`shuiyuan topic --render plain` 给 AI Agent 读取语义时剥掉格式符。
//! 保留：行内文字、`[text](url)` 的 text 部分、换行结构。
//! 移除：标题 `#`、引用 `>`、粗斜 `*_~`、行内 code 反引号、`![alt](img)` 整段。

/// 把 Discourse markdown 降级为纯文本。
pub fn to_plain(md: &str) -> String {
    md.lines()
        .map(|line| {
            let l = line
                .trim_start()
                .trim_start_matches('#')
                .trim_start()
                .trim_start_matches('>')
                .trim_start();
            strip_inline(l)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '~' | '`' => continue,
            '!' if chars.peek() == Some(&'[') => {
                // 图片 ![alt](url) —— 整段跳过
                chars.next();
                for nc in chars.by_ref() {
                    if nc == ']' {
                        break;
                    }
                }
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for nc in chars.by_ref() {
                        if nc == ')' {
                            break;
                        }
                    }
                }
            }
            '[' => {
                let mut text = String::new();
                for nc in chars.by_ref() {
                    if nc == ']' {
                        break;
                    }
                    text.push(nc);
                }
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for nc in chars.by_ref() {
                        if nc == ')' {
                            break;
                        }
                    }
                }
                out.push_str(&text);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_plain;

    #[test]
    fn strip_bold_italic_code() {
        assert_eq!(
            to_plain("**hello** _world_ ~~bye~~ `x`"),
            "hello world bye x"
        );
    }

    #[test]
    fn strip_headers_and_quotes() {
        assert_eq!(to_plain("# Title\n> quote"), "Title\nquote");
    }

    #[test]
    fn fold_links_keep_text() {
        assert_eq!(to_plain("see [docs](https://example.com)"), "see docs");
    }

    #[test]
    fn drop_images_entirely() {
        assert_eq!(to_plain("![alt](https://x/y.png) end"), " end");
    }
}
