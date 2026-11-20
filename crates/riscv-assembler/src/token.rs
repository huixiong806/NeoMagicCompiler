//! 词法分析：把一行汇编切成 token。
//!
//! 星之语的语法很规整，所以词法分析按"行"来做，逐行产出 [`Line`]。
//! 注释（`#` `;` `//`）在这里就被丢掉，字符串与字符字面量在这里被解好码。

/// 一行源码的分析结果。
#[derive(Debug, Clone)]
pub struct Line {
    /// 1-based 行号
    pub number: u32,
    /// 行首标签（`name:`）
    pub label: Option<String>,
    /// 助记符或指示符（指示符带前导 `.`）
    pub head: Option<String>,
    /// 操作数（已按 `,` 与空白切分）
    pub operands: Vec<String>,
    /// 原始文本（错误信息里回显用）
    pub raw: String,
}

/// 一行里出现的那种"看起来像字符串"的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringLit {
    /// 普通字符串 `"abc"`
    Str(String),
    /// 字符字面量 `'a'`，值已解好
    Char(i64),
}

/// 把源码切成行。
pub fn tokenize(source: &str) -> Vec<Line> {
    source
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .enumerate()
        .map(|(i, raw)| tokenize_line(raw, i as u32 + 1))
        .collect()
}

/// 解析单行。
pub fn tokenize_line(raw: &str, number: u32) -> Line {
    let stripped = strip_comment(raw);
    let text = stripped.trim();
    if text.is_empty() {
        return Line { number, label: None, head: None, operands: Vec::new(), raw: raw.to_string() };
    }

    // ── 行首标签 ──
    let (label, rest) = split_leading_label(text);
    let rest = rest.trim();
    if rest.is_empty() {
        return Line { number, label, head: None, operands: Vec::new(), raw: raw.to_string() };
    }

    // ── 助记符 / 指示符 ──
    let (head, operand_text) = match rest.find(|c: char| c.is_whitespace()) {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => (rest, ""),
    };

    Line {
        number,
        label,
        head: Some(head.to_string()),
        operands: split_operands(operand_text),
        raw: raw.to_string(),
    }
}

/// 去掉注释（`#` `;` `//`），但保留字符串里的内容。
fn strip_comment(line: &str) -> String {
    let bytes: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut in_str: Option<char> = None;
    while i < bytes.len() {
        let c = bytes[i];
        match in_str {
            Some(quote) => {
                out.push(c);
                if c == '\\' {
                    if i + 1 < bytes.len() {
                        out.push(bytes[i + 1]);
                        i += 2;
                        continue;
                    }
                } else if c == quote {
                    in_str = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_str = Some(c);
                    out.push(c);
                } else if c == '#' || c == ';' {
                    break;
                } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
                    break;
                } else {
                    out.push(c);
                }
            }
        }
        i += 1;
    }
    out
}

/// 取出行首的 `label:`。
fn split_leading_label(text: &str) -> (Option<String>, &str) {
    // 只认"行首是标识符 + 紧接着冒号"的形式
    let mut end = 0;
    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() || c == '_' || c == '.' || c == '$' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return (None, text);
    }
    let candidate = &text[..end];
    let after = &text[end..];
    if let Some(rest) = after.strip_prefix(':') {
        // `1:` 这种纯数字标签不合法
        if candidate.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
            return (None, text);
        }
        (Some(candidate.to_string()), rest)
    } else {
        (None, text)
    }
}

/// 按 `,` 与空白切分操作数，但保留字符串/字符字面量整体，并且**不拆散表达式**。
///
/// 规则：
/// 1. 先按顶层 `,` 分组（RISC-V 汇编的规范写法）；
/// 2. 组内再按空白切分。切出来的片段如果本身就是合法表达式，而"与下一段合并后也是
///    合法表达式"，就合并 —— 这样 `addi x1, x0, BASE + SIZE*2` 这类写法不会被拆散，
///    而 `add x1 x2 x3` 这种用空白分隔操作数的写法仍然正确。
fn split_operands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for group in split_top_level_commas(text) {
        let pieces = split_plain_whitespace(&group);
        if pieces.is_empty() {
            continue;
        }
        let mut i = 0;
        while i < pieces.len() {
            let mut current = pieces[i].clone();
            i += 1;
            // 尽量把后面的片段合并进当前操作数（只要合并后仍是合法表达式）
            while i < pieces.len() {
                let next = pieces[i].clone();
                // 纯运算符片段（如 `+`）本身不是操作数，直接并进来
                if is_bare_operator(&next) {
                    current = format!("{} {}", current, next);
                    i += 1;
                    continue;
                }
                let merged = format!("{} {}", current, next);
                // 合并条件：合并后是合法表达式；或者当前片段"还差一个右操作数"
                // （以二元运算符结尾，如 `BASE +`），此时也必须继续吃下一段
                let merge = crate::ast::parse_expr(&merged).is_ok()
                    || ends_with_binary_operator(&current);
                if merge {
                    current = merged;
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(current);
        }
    }
    out
}

/// 片段是否只由运算符组成（`+`、`-`、`*`、`<<` …）。
fn is_bare_operator(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| "+-*/%&|^~<>".contains(c))
}

/// 片段是否以二元运算符结尾（说明它还缺一个右操作数）。
fn ends_with_binary_operator(text: &str) -> bool {
    match text.chars().last() {
        Some(c) => "+-*/%&|^<>".contains(c),
        None => false,
    }
}

/// 按顶层逗号切分（忽略括号与引号里的逗号）。
fn split_top_level_commas(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                cur.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    cur.push(c);
                } else if c == '(' {
                    depth += 1;
                    cur.push(c);
                } else if c == ')' {
                    depth -= 1;
                    cur.push(c);
                } else if c == ',' && depth == 0 {
                    out.push(cur.trim().to_string());
                    cur.clear();
                } else {
                    cur.push(c);
                }
            }
        }
        i += 1;
    }
    out.push(cur.trim().to_string());
    out.retain(|s| !s.is_empty());
    out
}

/// 按空白切分，但保留引号内容与括号内的空白。
fn split_plain_whitespace(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match quote {
            Some(q) => {
                cur.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    cur.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    cur.push(c);
                } else if c == '(' {
                    depth += 1;
                    cur.push(c);
                } else if c == ')' {
                    depth -= 1;
                    cur.push(c);
                } else if c.is_whitespace() && depth == 0 {
                    if !cur.is_empty() {
                        out.push(cur.clone());
                        cur.clear();
                    }
                } else {
                    cur.push(c);
                }
            }
        }
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 解析字符串字面量，返回解码后的内容。
///
/// 支持 `\n \t \r \0 \\ \" \' \xNN`。
pub fn parse_string(text: &str) -> Result<String, String> {
    let inner = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| format!("`{}` 不是合法的字符串（要写成 \"...\" 的形式）", text))?;
    unescape(inner).map_err(|e| format!("字符串 `{}` 里的转义有问题：{}", text, e))
}

/// 解析字符字面量 `'a'`，返回它的整数值。
pub fn parse_char(text: &str) -> Result<i64, String> {
    let inner = text
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .ok_or_else(|| format!("`{}` 不是合法的字符字面量（要写成 'a' 的形式）", text))?;
    let decoded = unescape(inner)
        .map_err(|e| format!("字符 `{}` 里的转义有问题：{}", text, e))?;
    let mut it = decoded.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(c as i64),
        (None, _) => Err(format!("`{}` 里没有字符", text)),
        _ => Err(format!("`{}` 里有多个字符，字符字面量只能有一个", text)),
    }
}

fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('x') => {
                let h1 = chars.next().ok_or("\\x 后面要跟两位十六进制数")?;
                let h2 = chars.next().ok_or("\\x 后面要跟两位十六进制数")?;
                let hex: String = [h1, h2].iter().collect();
                let v = u8::from_str_radix(&hex, 16).map_err(|_| format!("`\\x{}` 不是十六进制", hex))?;
                out.push(v as char);
            }
            Some(other) => return Err(format!("不认识的转义 `\\{}`", other)),
            None => return Err("字符串以 `\\` 结束".to_string()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_comments_outside_strings() {
        let l = tokenize_line("add x1, x2, x3 # 加一下", 1);
        assert_eq!(l.head.as_deref(), Some("add"));
        assert_eq!(l.operands, vec!["x1", "x2", "x3"]);

        let l = tokenize_line(r#".asciz "a # b" # 注释"#, 2);
        assert_eq!(l.operands, vec![r#""a # b""#]);
    }

    #[test]
    fn splits_leading_label() {
        let l = tokenize_line("loop: addi x1, x1, 1", 3);
        assert_eq!(l.label.as_deref(), Some("loop"));
        assert_eq!(l.head.as_deref(), Some("addi"));

        let l = tokenize_line("done:", 4);
        assert_eq!(l.label.as_deref(), Some("done"));
        assert!(l.head.is_none());
    }

    #[test]
    fn parentheses_stay_glued() {
        let l = tokenize_line("lw x1, 4(x2)", 5);
        assert_eq!(l.operands, vec!["x1", "4(x2)"]);

        let l = tokenize_line("addi x1, x0, (2+3)*4", 6);
        assert_eq!(l.operands, vec!["x1", "x0", "(2+3)*4"]);
    }

    #[test]
    fn expressions_with_spaces_stay_one_operand() {
        let l = tokenize_line("li x1, BASE + SIZE*2", 7);
        assert_eq!(l.operands, vec!["x1", "BASE + SIZE*2"], "表达式不能被拆开");

        let l = tokenize_line("li x1, (1+2) * (3+4)", 8);
        assert_eq!(l.operands, vec!["x1", "(1+2) * (3+4)"]);
    }

    #[test]
    fn whitespace_separated_operands_are_not_merged() {
        let l = tokenize_line("add x1 x2 x3", 9);
        assert_eq!(l.operands, vec!["x1", "x2", "x3"], "空白分隔的操作数要保持分开");
    }

    #[test]
    fn string_and_char_literals() {
        assert_eq!(parse_string(r#""hello""#).unwrap(), "hello");
        assert_eq!(parse_string(r#""a\nb""#).unwrap(), "a\nb");
        assert_eq!(parse_string(r#""\x41""#).unwrap(), "A");
        assert_eq!(parse_char("'A'").unwrap(), 65);
        assert_eq!(parse_char(r"'\n'").unwrap(), 10);
        assert!(parse_char("'ab'").is_err());
        assert!(parse_string("nope").is_err());
    }
}
