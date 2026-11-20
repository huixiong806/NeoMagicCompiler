//! 星咒词法分析。
//!
//! 把源码切成 token 流，同时记录每个 token 的**行号**（错误信息要指给玩家看）。

use std::fmt;

/// 一个 token 的位置（行、列，均从 1 开始）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "第 {} 行", self.line)
    }
}

/// 星咒的 token。
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// 标识符（变量名 / 函数名 / 结构体名）
    Ident(String),
    /// 整数常量
    Int(i32),
    /// 字符常量（值是它的编码）
    Char(i32),
    /// 字符串常量
    Str(String),
    /// 关键字（`int` `if` `while` …）
    Kw(&'static str),
    /// 运算符或标点
    Punct(&'static str),
    /// 结束
    Eof,
}

impl Tok {
    /// 面向玩家的描述（错误信息里用）。
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(s) => format!("标识符 `{}`", s),
            Tok::Int(v) => format!("整数 `{}`", v),
            Tok::Char(c) => format!("字符 `{}`", *c as u8 as char),
            Tok::Str(s) => format!("字符串 \"{}\"", s),
            Tok::Kw(k) => format!("关键字 `{}`", k),
            Tok::Punct(p) => format!("`{}`", p),
            Tok::Eof => "代码末尾".to_string(),
        }
    }
}

/// 词法错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub pos: Pos,
    pub message: String,
}

/// 关键字表。
const KEYWORDS: &[&str] = &[
    "int", "char", "void", "struct", "if", "else", "while", "for", "do", "break", "continue",
    "return", "sizeof",
];

/// 运算符表（按长度从长到短匹配）。
const PUNCTS: &[&str] = &[
    "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=", "%=",
    "&=", "|=", "^=", "<<", ">>", "->", "+", "-", "*", "/", "%", "=", "<", ">", "!", "&", "|", "^",
    "~", "?", ":", ";", ",", "(", ")", "{", "}", "[", "]", ".",
];

/// 词法分析器。
pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { src: source.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    fn here(&self) -> Pos {
        Pos { line: self.line, col: self.col }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// 跳过空白与注释（`//` 与 `/* */`）。
    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.bump();
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let start = self.here();
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            Some(b'*') if self.peek_at(1) == Some(b'/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            Some(_) => {
                                self.bump();
                            }
                            None => {
                                return Err(LexError {
                                    pos: start,
                                    message: "这个 `/*` 注释没有对应的 `*/`".to_string(),
                                })
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// 取下一个 token。
    pub fn next_token(&mut self) -> Result<(Pos, Tok), LexError> {
        self.skip_trivia()?;
        let pos = self.here();
        let Some(c) = self.peek() else {
            return Ok((pos, Tok::Eof));
        };

        // 标识符 / 关键字
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.pos;
            while let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'_' {
                    self.bump();
                } else {
                    break;
                }
            }
            let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap_or("").to_string();
            let tok = KEYWORDS
                .iter()
                .find(|k| **k == text)
                .map(|k| Tok::Kw(k))
                .unwrap_or(Tok::Ident(text));
            return Ok((pos, tok));
        }

        // 预处理指令：星咒没有预处理器，给出明确提示而不是"看不懂的字符 `#`"
        if c == b'#' {
            while let Some(c) = self.peek() {
                if c == b'\n' {
                    break;
                }
                self.bump();
            }
            return Err(LexError {
                pos,
                message: "星咒没有预处理指令，`#include` / `#define` 这类写法要去掉".to_string(),
            });
        }

        // 数字（十进制 / 0x 十六进制）
        if c.is_ascii_digit() {
            let mut value: i64 = 0;
            if c == b'0' && matches!(self.peek_at(1), Some(b'x') | Some(b'X')) {
                self.bump();
                self.bump();
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_ascii_hexdigit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
                if self.pos == start {
                    return Err(LexError { pos, message: "`0x` 后面要跟十六进制数字".to_string() });
                }
                let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
                value = i64::from_str_radix(text, 16)
                    .map_err(|_| LexError { pos, message: "这个十六进制数太大了".to_string() })?;
            } else {
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        value = value * 10 + (c - b'0') as i64;
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
            if !(-(1 << 31)..(1 << 31)).contains(&value) {
                return Err(LexError { pos, message: "这个数超出了 32 位整数的范围".to_string() });
            }
            return Ok((pos, Tok::Int(value as i32)));
        }

        // 字符常量
        if c == b'\'' {
            self.bump();
            let value = self.read_char_or_escape(pos)?;
            if self.peek() != Some(b'\'') {
                return Err(LexError { pos, message: "字符常量要用单引号包起来，例如 'a'".to_string() });
            }
            self.bump();
            return Ok((pos, Tok::Char(value)));
        }

        // 字符串常量
        if c == b'"' {
            self.bump();
            let mut out = String::new();
            loop {
                match self.peek() {
                    Some(b'"') => {
                        self.bump();
                        break;
                    }
                    Some(b'\\') => {
                        self.bump();
                        let v = self.read_escape(pos)?;
                        out.push(v as u8 as char);
                    }
                    Some(ch) => {
                        self.bump();
                        out.push(ch as char);
                    }
                    None => {
                        return Err(LexError {
                            pos,
                            message: "这个字符串没有结尾的双引号".to_string(),
                        })
                    }
                }
            }
            return Ok((pos, Tok::Str(out)));
        }

        // 运算符 / 标点
        for p in PUNCTS {
            if self.src[self.pos..].starts_with(p.as_bytes()) {
                for _ in 0..p.len() {
                    self.bump();
                }
                return Ok((pos, Tok::Punct(p)));
            }
        }

        Err(LexError {
            pos,
            message: format!("看不懂的字符 `{}`", c as char),
        })
    }

    /// 读一个字符常量里的字符（可能是 `\n` 这样的转义）。
    fn read_char_or_escape(&mut self, pos: Pos) -> Result<i32, LexError> {
        if self.peek() == Some(b'\\') {
            self.bump();
            return self.read_escape(pos);
        }
        match self.bump() {
            Some(c) => Ok(c as i32),
            None => Err(LexError { pos, message: "字符常量里没有字符".to_string() }),
        }
    }

    /// 读一个转义序列（反斜杠已经被吃掉）。
    fn read_escape(&mut self, pos: Pos) -> Result<i32, LexError> {
        let Some(c) = self.bump() else {
            return Err(LexError { pos, message: "转义没写完".to_string() });
        };
        Ok(match c {
            b'n' => b'\n' as i32,
            b't' => b'\t' as i32,
            b'r' => b'\r' as i32,
            b'0' => 0,
            b'\\' => b'\\' as i32,
            b'\'' => b'\'' as i32,
            b'"' => b'"' as i32,
            b'x' => {
                let mut v = 0i32;
                let mut n = 0;
                while n < 2 {
                    match self.peek() {
                        Some(h) if h.is_ascii_hexdigit() => {
                            v = v * 16 + (h as char).to_digit(16).unwrap() as i32;
                            self.bump();
                            n += 1;
                        }
                        _ => break,
                    }
                }
                if n == 0 {
                    return Err(LexError { pos, message: "`\\x` 后面要跟两位十六进制数".to_string() });
                }
                v
            }
            other => {
                return Err(LexError {
                    pos,
                    message: format!("不认识的转义 `\\{}`", other as char),
                })
            }
        })
    }

    /// 一次性取完所有 token。
    pub fn tokenize(mut self) -> Result<Vec<(Pos, Tok)>, LexError> {
        let mut out = Vec::new();
        loop {
            let (pos, tok) = self.next_token()?;
            let eof = tok == Tok::Eof;
            out.push((pos, tok));
            if eof {
                return Ok(out);
            }
        }
    }
}

/// 把源码切成 token 流。
pub fn tokenize(source: &str) -> Result<Vec<(Pos, Tok)>, LexError> {
    Lexer::new(source).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        tokenize(src).unwrap().into_iter().map(|(_, t)| t).collect()
    }

    #[test]
    fn tokenizes_a_small_function() {
        let t = toks("int solve(int a) { return a + 1; }");
        assert_eq!(
            t,
            vec![
                Tok::Kw("int"),
                Tok::Ident("solve".into()),
                Tok::Punct("("),
                Tok::Kw("int"),
                Tok::Ident("a".into()),
                Tok::Punct(")"),
                Tok::Punct("{"),
                Tok::Kw("return"),
                Tok::Ident("a".into()),
                Tok::Punct("+"),
                Tok::Int(1),
                Tok::Punct(";"),
                Tok::Punct("}"),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn numbers_and_escapes() {
        assert_eq!(toks("0x1F")[0], Tok::Int(31));
        assert_eq!(toks("'A'")[0], Tok::Char(65));
        assert_eq!(toks(r"'\n'")[0], Tok::Char(10));
        assert_eq!(toks(r#""a\tb""#)[0], Tok::Str("a\tb".to_string()));
    }

    #[test]
    fn multi_char_operators_win() {
        let t = toks("a <<= 1; b != c; d && e; x->y;");
        assert!(t.contains(&Tok::Punct("<<=")));
        assert!(t.contains(&Tok::Punct("!=")));
        assert!(t.contains(&Tok::Punct("&&")));
        assert!(t.contains(&Tok::Punct("->")));
    }

    #[test]
    fn comments_are_skipped() {
        let t = toks("a // 注释\n /* 多行\n 注释 */ b");
        assert_eq!(t, vec![Tok::Ident("a".into()), Tok::Ident("b".into()), Tok::Eof]);
    }

    #[test]
    fn positions_are_tracked() {
        let toks = tokenize("a\n  b").unwrap();
        assert_eq!(toks[0].0.line, 1);
        assert_eq!(toks[1].0.line, 2);
        assert_eq!(toks[1].0.col, 3);
    }

    #[test]
    fn errors_are_reported_with_position() {
        let e = tokenize("int a = @;").unwrap_err();
        assert_eq!(e.pos.line, 1);
        assert!(e.message.contains('@'));

        let e = tokenize("/* 没关").unwrap_err();
        assert!(e.message.contains("*/"));
    }
}
