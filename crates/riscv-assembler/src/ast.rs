//! 语法分析：把 token 行组装成"条目"序列，并提供表达式求值。

use crate::token::{self, Line};
use crate::AsmError;
use std::collections::HashMap;

/// 一个可求值的算术表达式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i64),
    /// 符号引用（标签或 `.equ` 常量）
    Sym(String),
    Unary {
        op: char,
        rhs: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

/// 一个汇编条目。
#[derive(Debug, Clone)]
pub enum Item {
    Label { name: String, line: u32 },
    Instruction { mnemonic: String, operands: Vec<String>, line: u32 },
    Directive { directive: Directive, line: u32 },
}

impl Item {
    pub fn line(&self) -> u32 {
        match self {
            Item::Label { line, .. } | Item::Instruction { line, .. } | Item::Directive { line, .. } => {
                *line
            }
        }
    }
}

/// 数据与布局指示符。
#[derive(Debug, Clone)]
pub enum Directive {
    /// `.text`：切回代码段
    Text,
    /// `.data`：切到数据段
    Data,
    /// `.org <addr>`：把当前段的地址计数器设为 addr
    Org { expr: Expr },
    /// `.align <n>`：地址对齐到 n 字节
    Align { n: u32 },
    /// `.globl <name>`：声明为全局符号（本项目里只用于可读性，不做实际处理）
    Globl { names: Vec<String> },
    /// `.equ <name>, <expr>`
    Equ { name: String, expr: Expr },
    Word { values: Vec<Expr> },
    Half { values: Vec<Expr> },
    Byte { values: Vec<Expr> },
    Ascii { text: String },
    Asciz { text: String },
    Space { n: u32 },
}

/// 解析整段源码。
pub fn parse(source: &str) -> Result<Vec<Item>, Vec<AsmError>> {
    let lines = token::tokenize(source);
    let mut items = Vec::new();
    let mut errors = Vec::new();

    for line in &lines {
        match parse_line(line) {
            Ok(mut produced) => items.append(&mut produced),
            Err(e) => errors.push(e),
        }
    }

    if errors.is_empty() {
        Ok(items)
    } else {
        errors.sort_by_key(|e| e.line);
        Err(errors)
    }
}

fn parse_line(line: &Line) -> Result<Vec<Item>, AsmError> {
    let mut out = Vec::new();
    if let Some(name) = &line.label {
        out.push(Item::Label { name: name.clone(), line: line.number });
    }

    let Some(head) = &line.head else {
        return Ok(out);
    };

    if head.starts_with('.') {
        out.push(Item::Directive {
            directive: parse_directive(head, &line.operands, line)?,
            line: line.number,
        });
        return Ok(out);
    }

    // 助记符一律小写（玩家写大写也能跑）
    out.push(Item::Instruction {
        mnemonic: head.to_ascii_lowercase(),
        operands: line.operands.clone(),
        line: line.number,
    });
    Ok(out)
}

fn parse_directive(head: &str, operands: &[String], line: &Line) -> Result<Directive, AsmError> {
    let n = line.number;
    let lower = head.to_ascii_lowercase();
    let d = match lower.as_str() {
        ".text" => Directive::Text,
        ".data" => Directive::Data,
        ".org" => {
            let e = expect_one(&lower, operands, n)?;
            Directive::Org { expr: parse_expr(e).map_err(|m| AsmError::new(n, m))? }
        }
        ".align" | ".balign" | ".p2align" => {
            let e = expect_one(&lower, operands, n)?;
            let v = eval_simple(e).map_err(|m| AsmError::new(n, m))?;
            if v <= 0 || (v & (v - 1)) != 0 {
                return Err(AsmError::new(n, "`.align` 后面必须是 2 的幂（如 2、4、8）"));
            }
            Directive::Align { n: v as u32 }
        }
        ".globl" | ".global" => Directive::Globl { names: operands.to_vec() },
        ".equ" | ".set" => {
            if operands.len() != 2 {
                return Err(AsmError::new(n, "`.equ` 要写成 `.equ 名字, 值`"));
            }
            let name = operands[0].clone();
            let expr = parse_expr(&operands[1]).map_err(|m| AsmError::new(n, m))?;
            Directive::Equ { name, expr }
        }
        ".word" | ".long" | ".int" => Directive::Word { values: parse_expr_list(operands, n)? },
        ".half" | ".short" => Directive::Half { values: parse_expr_list(operands, n)? },
        ".byte" => Directive::Byte { values: parse_expr_list(operands, n)? },
        ".ascii" => Directive::Ascii { text: expect_string(operands, n, &lower)? },
        ".asciz" | ".string" => Directive::Asciz { text: expect_string(operands, n, &lower)? },
        ".space" | ".skip" | ".zero" => {
            let e = expect_one(&lower, operands, n)?;
            let v = eval_simple(e).map_err(|m| AsmError::new(n, m))?;
            if v < 0 {
                return Err(AsmError::new(n, "预留的空间大小不能是负数"));
            }
            Directive::Space { n: v as u32 }
        }
        other => {
            return Err(AsmError::new(
                n,
                format!("不认识的指示符 `{}`（常用的是 .text .data .word .byte .ascii .asciz .space）", other),
            ))
        }
    };
    Ok(d)
}

fn expect_one<'a>(name: &str, operands: &'a [String], line: u32) -> Result<&'a str, AsmError> {
    if operands.len() != 1 {
        return Err(AsmError::new(line, format!("`{}` 后面要跟一个参数", name)));
    }
    Ok(&operands[0])
}

fn expect_string(operands: &[String], line: u32, name: &str) -> Result<String, AsmError> {
    let raw = expect_one(name, operands, line)?;
    token::parse_string(raw).map_err(|m| AsmError::new(line, m))
}

fn parse_expr_list(operands: &[String], line: u32) -> Result<Vec<Expr>, AsmError> {
    if operands.is_empty() {
        return Err(AsmError::new(line, "这里至少要写一个数值"));
    }
    operands.iter().map(|o| parse_expr(o).map_err(|m| AsmError::new(line, m))).collect()
}

/// 只含常量（不含标签）的简易求值，用于 `.align` / `.space` 这类参数。
fn eval_simple(text: &str) -> Result<i64, String> {
    let e = parse_expr(text)?;
    eval(&e, &HashMap::new(), &HashMap::new())
}

// ─────────────────────────────────────────────────────────────────────────────
//  表达式求值
// ─────────────────────────────────────────────────────────────────────────────

/// 求值表达式。`labels` 是标签地址表，`consts` 是 `.equ` 常量表。
pub fn eval(
    expr: &Expr,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> Result<i64, String> {
    Ok(match expr {
        Expr::Num(v) => *v,
        Expr::Sym(name) => {
            if let Some(v) = consts.get(name) {
                *v
            } else if let Some(addr) = labels.get(name) {
                *addr as i64
            } else {
                return Err(format!("不认识的符号 `{}`（标签要先定义，常量要用 `.equ` 声明）", name));
            }
        }
        Expr::Unary { op, rhs } => {
            let v = eval(rhs, labels, consts)?;
            match op {
                '-' => -v,
                '~' => !v,
                '+' => v,
                _ => return Err(format!("不支持的一元运算符 `{}`", op)),
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let a = eval(lhs, labels, consts)?;
            let b = eval(rhs, labels, consts)?;
            match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => {
                    if b == 0 {
                        return Err("表达式里除以了 0".to_string());
                    }
                    a / b
                }
                BinOp::Rem => {
                    if b == 0 {
                        return Err("表达式里对 0 取了余数".to_string());
                    }
                    a % b
                }
                BinOp::And => a & b,
                BinOp::Or => a | b,
                BinOp::Xor => a ^ b,
                BinOp::Shl => a << b,
                BinOp::Shr => a >> b,
            }
        }
    })
}

    /// 解析一个表达式。
    ///
    /// **必须把整段文本用完**，否则报错 —— 这样 `token` 模块才能用
    /// "能不能解析成功"来判断空白分隔的片段是否属于同一个表达式。
    pub fn parse_expr(text: &str) -> Result<Expr, String> {
        let tokens = lex_expr(text)?;
        if tokens.is_empty() {
            return Err(format!("`{}` 不是一个表达式", text));
        }
        let mut p = ExprParser { tokens, pos: 0, text };
        let e = p.parse_binary(0)?;
        if p.pos != p.tokens.len() {
            return Err(format!("表达式 `{}` 里有看不懂的部分", text));
        }
        Ok(e)
    }

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Num(i64),
    Sym(String),
    Op(String),
}

fn lex_expr(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i == start + 2 {
                    return Err(format!("`{}` 不是合法的十六进制数", text));
                }
            } else {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
            }
            let raw: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            let v = if raw.starts_with("0x") || raw.starts_with("0X") {
                i64::from_str_radix(&raw[2..], 16)
            } else {
                raw.parse::<i64>()
            }
            .map_err(|_| format!("`{}` 超出范围了", raw))?;
            out.push(Tok::Num(v));
            continue;
        }
        if c == '\'' {
            // 字符字面量
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= chars.len() {
                return Err("字符字面量少了结尾的单引号".to_string());
            }
            i += 1;
            let raw: String = chars[start..i].iter().collect();
            let v = token::parse_char(&raw)?;
            out.push(Tok::Num(v));
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '.' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.' || chars[i] == '$')
            {
                i += 1;
            }
            out.push(Tok::Sym(chars[start..i].iter().collect()));
            continue;
        }
        // 运算符（先长后短）
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if ["<<", ">>"].contains(&two.as_str()) {
            out.push(Tok::Op(two));
            i += 2;
            continue;
        }
        if "+-*/%&|^~()".contains(c) {
            out.push(Tok::Op(c.to_string()));
            i += 1;
            continue;
        }
        return Err(format!("表达式 `{}` 里有不认识的字符 `{}`", text, c));
    }
    Ok(out)
}

struct ExprParser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    text: &'a str,
}

impl ExprParser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            let (prec, binop) = match op.as_str() {
                "|" => (1, BinOp::Or),
                "^" => (2, BinOp::Xor),
                "&" => (3, BinOp::And),
                "<<" => (4, BinOp::Shl),
                ">>" => (4, BinOp::Shr),
                "+" => (5, BinOp::Add),
                "-" => (5, BinOp::Sub),
                "*" => (6, BinOp::Mul),
                "/" => (6, BinOp::Div),
                "%" => (6, BinOp::Rem),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Binary { op: binop, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Op(op)) if op == "-" || op == "~" || op == "+" => {
                self.bump();
                let rhs = self.parse_unary()?;
                Ok(Expr::Unary { op: op.chars().next().unwrap(), rhs: Box::new(rhs) })
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(Tok::Num(v)) => Ok(Expr::Num(v)),
            Some(Tok::Sym(s)) => Ok(Expr::Sym(s)),
            Some(Tok::Op(op)) if op == "(" => {
                let e = self.parse_binary(0)?;
                match self.bump() {
                    Some(Tok::Op(close)) if close == ")" => Ok(e),
                    _ => Err(format!("表达式 `{}` 的括号没有配对", self.text)),
                }
            }
            other => Err(match other {
                Some(t) => format!("表达式 `{}` 里 `{:?}` 出现的位置不对", self.text, t),
                None => format!("表达式 `{}` 不完整", self.text),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(text: &str) -> i64 {
        eval(&parse_expr(text).unwrap(), &HashMap::new(), &HashMap::new()).unwrap()
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(ev("1+2*3"), 7);
        assert_eq!(ev("(1+2)*3"), 9);
        assert_eq!(ev("10-2-3"), 5);
        assert_eq!(ev("2*(3+4)"), 14);
        assert_eq!(ev("-5+8"), 3);
        assert_eq!(ev("~0"), -1);
        assert_eq!(ev("1<<4"), 16);
        assert_eq!(ev("0xF0|0x0F"), 255);
        assert_eq!(ev("7%3"), 1);
        assert_eq!(ev("'A'"), 65);
    }

    #[test]
    fn symbols_resolve() {
        let mut labels = HashMap::new();
        labels.insert("target".to_string(), 0x40u32);
        let mut consts = HashMap::new();
        consts.insert("SIZE".to_string(), 16i64);
        let e = parse_expr("target + SIZE*2").unwrap();
        assert_eq!(eval(&e, &labels, &consts).unwrap(), 0x60);
    }

    #[test]
    fn unknown_symbol_is_reported() {
        let e = parse_expr("nope+1").unwrap();
        let err = eval(&e, &HashMap::new(), &HashMap::new()).unwrap_err();
        assert!(err.contains("nope"));
    }

    #[test]
    fn parse_basic_program() {
        let src = "
            # 注释
            .text
        start:  addi x1, x0, 5
                .word 1, 2
        .equ    FIVE, 5
        ";
        let items = parse(src).unwrap();
        assert!(matches!(items[0], Item::Directive { directive: Directive::Text, .. }));
        assert!(matches!(items[1], Item::Label { .. }));
        assert!(matches!(items[2], Item::Instruction { .. }));
        assert!(matches!(items[3], Item::Directive { directive: Directive::Word { .. }, .. }));
        assert!(matches!(items[4], Item::Directive { directive: Directive::Equ { .. }, .. }));
    }
}
