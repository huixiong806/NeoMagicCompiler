//! 星咒语法分析：token 流 → AST（递归下降）。
//!
//! 表达式优先级（从低到高）：
//! `,` → 赋值 → `?:` → `||` → `&&` → `|` → `^` → `&` → `== !=` → `< <= > >=`
//! → `<< >>` → `+ -` → `* / %` → 一元 → 后缀（`[]` `.` `->` `()` `++` `--`）→ 基本项

use crate::ast::*;
use crate::lexer::{Pos, Tok};
use std::collections::HashMap;

/// 语法错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub pos: Pos,
    pub message: String,
}

impl ParseError {
    fn new(pos: Pos, message: impl Into<String>) -> Self {
        Self { pos, message: message.into() }
    }
}

type PResult<T> = Result<T, ParseError>;

/// 语法分析器。
pub struct Parser {
    toks: Vec<(Pos, Tok)>,
    pos: usize,
    pub structs: HashMap<String, StructDef>,
    pub strings: Vec<String>,
}

/// 把源码解析为 AST。
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let toks = crate::lexer::tokenize(source).map_err(|e| ParseError::new(e.pos, e.message))?;
    let mut p = Parser { toks, pos: 0, structs: HashMap::new(), strings: Vec::new() };
    p.parse_program()
}

/// 把一段源码解析为一个函数体（供"只写一个函数"的关卡使用）。
pub fn parse_function(source: &str) -> Result<FuncDecl, ParseError> {
    let program = parse(source)?;
    program
        .functions
        .into_iter()
        .next()
        .ok_or_else(|| ParseError::new(Pos { line: 1, col: 1 }, "这段代码里没有函数"))
}

impl Parser {
    // ── token 访问 ──

    fn peek(&self) -> &Tok {
        self.toks.get(self.pos).map(|(_, t)| t).unwrap_or(&Tok::Eof)
    }

    fn peek_at(&self, off: usize) -> &Tok {
        self.toks.get(self.pos + off).map(|(_, t)| t).unwrap_or(&Tok::Eof)
    }

    fn pos(&self) -> Pos {
        self.toks.get(self.pos).map(|(p, _)| *p).unwrap_or_default()
    }

    fn bump(&mut self) -> (Pos, Tok) {
        let t = self.toks.get(self.pos).cloned().unwrap_or((Pos::default(), Tok::Eof));
        if self.pos < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Tok::Punct(x) if *x == p) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_kw(&mut self, k: &str) -> bool {
        if matches!(self.peek(), Tok::Kw(x) if *x == k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, p: &str) -> PResult<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            Err(ParseError::new(
                self.pos(),
                format!("这里应该是一个 `{}`，但看到的是 {}", p, self.peek().describe()),
            ))
        }
    }

    fn expect_ident(&mut self) -> PResult<(Pos, String)> {
        match self.bump() {
            (pos, Tok::Ident(name)) => Ok((pos, name)),
            (pos, other) => Err(ParseError::new(
                pos,
                format!("这里应该是一个名字，但看到的是 {}", other.describe()),
            )),
        }
    }

    /// 读"指针星号之后的那个名字"，支持 `int* p` / `int * p` / `int** p`。
    fn name_after_stars(&mut self) -> PResult<(Pos, String, usize)> {
        let mut stars = 0usize;
        while self.eat_punct("*") {
            stars += 1;
        }
        let (pos, name) = self.expect_ident()?;
        Ok((pos, name, stars))
    }

    /// 把类型包 `stars` 层指针。
    fn apply_stars(&self, mut ty: Ty, stars: usize) -> Ty {
        for _ in 0..stars {
            ty = ty.ref_of();
        }
        ty
    }

    /// 吃掉并返回当前位置连续 `*` 的个数。
    fn take_stars(&mut self) -> usize {
        let mut n = 0;
        while self.eat_punct("*") {
            n += 1;
        }
        n
    }

    /// 下一个 token 是不是类型名（用于区分"声明"与"表达式"）。
    fn at_type_start(&self) -> bool {
        matches!(
            self.peek(),
            Tok::Kw("int") | Tok::Kw("char") | Tok::Kw("void") | Tok::Kw("struct")
        )
    }

    // ── 顶层 ──

    fn parse_program(&mut self) -> PResult<Program> {
        let mut program = Program::default();
        while !matches!(self.peek(), Tok::Eof) {
            if self.eat_punct(";") {
                continue;
            }
            // 结构体定义：`struct 名字 { ... };`
            if matches!(self.peek(), Tok::Kw("struct"))
                && matches!(self.peek_at(1), Tok::Ident(_))
                && matches!(self.peek_at(2), Tok::Punct("{"))
            {
                self.bump();
                let (pos, name) = self.expect_ident()?;
                self.expect_punct("{")?;
                let def = self.parse_struct_body(pos, name)?;
                self.expect_punct(";")?;
                // 同时记进 self.structs：后面的声明要能按名字引用它（并计算大小）
                program.structs.insert(def.name.clone(), def.clone());
                self.structs.insert(def.name.clone(), def);
                continue;
            }

            // 全局变量或函数
            let base = self.parse_base_type()?;
            let (pos, name, stars) = self.name_after_stars()?;
            let ty = self.apply_stars(base, stars);

            if self.eat_punct("(") {
                let func = self.parse_function_rest(pos, name, ty)?;
                program.functions.push(func);
            } else {
                let (ty, _) = self.parse_array_suffix(ty)?;
                let (init, init_list) = if self.eat_punct("=") {
                    if self.eat_punct("{") {
                        (None, Some(self.parse_init_list()?))
                    } else {
                        (Some(self.parse_assign()?), None)
                    }
                } else {
                    (None, None)
                };
                self.expect_punct(";")?;
                program.globals.push(VarDecl { name, ty, init, init_list, pos });
            }
        }
        program.structs = self.structs.clone();
        program.strings = self.strings.clone();
        Ok(program)
    }

    fn parse_struct_body(&mut self, pos: Pos, name: String) -> PResult<StructDef> {
        let mut fields = Vec::new();
        let mut offset = 0u32;
        while !self.eat_punct("}") {
            if matches!(self.peek(), Tok::Eof) {
                return Err(ParseError::new(pos, format!("结构体 `{}` 少了结尾的 `}}`", name)));
            }
            let base = self.parse_base_type()?;
            loop {
                let (fpos, fname, stars) = self.name_after_stars()?;
                let fty = self.apply_stars(base.clone(), stars);
                let (fty, _count) = self.parse_array_suffix(fty)?;
                if matches!(fty, Ty::Void) {
                    return Err(ParseError::new(fpos, "结构体成员不能是 void"));
                }
                let size = fty.size_with(&self.structs);
                fields.push((fname, fty, offset));
                offset += size.max(1);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(";")?;
        }
        Ok(StructDef { name, fields, size: offset })
    }

    fn parse_function_rest(&mut self, pos: Pos, name: String, ret: Ty) -> PResult<FuncDecl> {
        let mut params = Vec::new();
        if !self.eat_punct(")") {
            if matches!(self.peek(), Tok::Kw("void")) && matches!(self.peek_at(1), Tok::Punct(")")) {
                self.bump();
                self.expect_punct(")")?;
            } else {
                loop {
                    let base = self.parse_base_type()?;
                    let (ppos, pname, stars) = self.name_after_stars()?;
                    let pty = self.apply_stars(base, stars);
                    let (pty, _) = self.parse_array_suffix(pty)?;
                    let pty = match pty {
                        // 数组参数退化为指针
                        Ty::Array(elem, _) => Ty::Ptr(elem),
                        other => other,
                    };
                    if matches!(pty, Ty::Void) {
                        return Err(ParseError::new(ppos, format!("参数 `{}` 不能是 void", pname)));
                    }
                    params.push((pname, pty));
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                self.expect_punct(")")?;
            }
        }
        if self.eat_punct(";") {
            return Ok(FuncDecl { name, ret, params, body: None, pos });
        }
        let body = self.parse_block()?;
        Ok(FuncDecl { name, ret, params, body: Some(body), pos })
    }

    /// 基础类型（`int` / `char` / `void` / `struct 名字`）。
    fn parse_base_type(&mut self) -> PResult<Ty> {
        let pos = self.pos();
        match self.bump() {
            (_, Tok::Kw("int")) => Ok(Ty::Int),
            (_, Tok::Kw("char")) => Ok(Ty::Char),
            (_, Tok::Kw("void")) => Ok(Ty::Void),
            (_, Tok::Kw("struct")) => {
                let (_, name) = self.expect_ident()?;
                if !self.structs.contains_key(&name) {
                    return Err(ParseError::new(
                        pos,
                        format!(
                            "不认识结构体 `{}`（要先写 `struct {} {{ ... }};` 定义它）",
                            name, name
                        ),
                    ));
                }
                Ok(Ty::Struct(name))
            }
            (_, other) => Err(ParseError::new(
                pos,
                format!(
                    "这里应该是一个类型（int / char / void / struct 名字），但看到的是 {}",
                    other.describe()
                ),
            )),
        }
    }

    /// 解析 `[N]` 后缀（只支持一维）。
    fn parse_array_suffix(&mut self, base: Ty) -> PResult<(Ty, Option<usize>)> {
        if !self.eat_punct("[") {
            return Ok((base, None));
        }
        if self.eat_punct("]") {
            // `[]` 只有参数里允许，长度交给调用者处理
            return Ok((Ty::Array(Box::new(base), 0), None));
        }
        let pos = self.pos();
        let (_, tok) = self.bump();
        let Tok::Int(n) = tok else {
            return Err(ParseError::new(pos, "数组长度要写一个正整数"));
        };
        if n <= 0 {
            return Err(ParseError::new(pos, "数组长度要大于 0"));
        }
        self.expect_punct("]")?;
        Ok((Ty::Array(Box::new(base), n as usize), Some(n as usize)))
    }

    fn parse_init_list(&mut self) -> PResult<Vec<Expr>> {
        let mut out = Vec::new();
        if self.eat_punct("}") {
            return Ok(out);
        }
        loop {
            out.push(self.parse_assign()?);
            if self.eat_punct(",") {
                if self.eat_punct("}") {
                    break;
                }
                continue;
            }
            self.expect_punct("}")?;
            break;
        }
        Ok(out)
    }

    // ── 语句 ──

    fn parse_block(&mut self) -> PResult<Block> {
        self.expect_punct("{")?;
        let mut stmts = Vec::new();
        while !self.eat_punct("}") {
            if matches!(self.peek(), Tok::Eof) {
                return Err(ParseError::new(self.pos(), "少了结尾的 `}`"));
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(Block { stmts })
    }

    /// 块里的"声明语句"（返回 None 表示下一个 token 不是声明）。
    fn try_parse_decl(&mut self) -> PResult<Option<Stmt>> {
        if !self.at_type_start() {
            return Ok(None);
        }
        let base = self.parse_base_type()?;
        let (pos, name, stars) = self.name_after_stars()?;
        let ty = self.apply_stars(base, stars);
        let (ty, _) = self.parse_array_suffix(ty)?;
        if matches!(ty, Ty::Void) {
            return Err(ParseError::new(pos, format!("变量 `{}` 不能是 void", name)));
        }
        let (init, init_list) = if self.eat_punct("=") {
            if self.eat_punct("{") {
                (None, Some(self.parse_init_list()?))
            } else {
                (Some(self.parse_assign()?), None)
            }
        } else {
            (None, None)
        };
        self.expect_punct(";")?;
        Ok(Some(Stmt::Decl(VarDecl { name, ty, init, init_list, pos })))
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        if let Some(decl) = self.try_parse_decl()? {
            return Ok(decl);
        }
        let pos = self.pos();
        match self.peek().clone() {
            Tok::Punct("{") => Ok(Stmt::Block(self.parse_block()?)),
            Tok::Punct(";") => {
                self.bump();
                Ok(Stmt::Empty)
            }
            Tok::Kw("if") => {
                self.bump();
                self.expect_punct("(")?;
                let cond = self.parse_expr()?;
                self.expect_punct(")")?;
                let then = Box::new(self.parse_stmt()?);
                let other = if self.eat_kw("else") { Some(Box::new(self.parse_stmt()?)) } else { None };
                Ok(Stmt::If { cond, then, other })
            }
            Tok::Kw("while") => {
                self.bump();
                self.expect_punct("(")?;
                let cond = self.parse_expr()?;
                self.expect_punct(")")?;
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::While { cond, body })
            }
            Tok::Kw("do") => {
                self.bump();
                let body = Box::new(self.parse_stmt()?);
                if !self.eat_kw("while") {
                    return Err(ParseError::new(pos, "`do` 后面要跟 `while (...)`"));
                }
                self.expect_punct("(")?;
                let cond = self.parse_expr()?;
                self.expect_punct(")")?;
                self.expect_punct(";")?;
                Ok(Stmt::DoWhile { body, cond })
            }
            Tok::Kw("for") => {
                self.bump();
                self.expect_punct("(")?;
                let init = if self.eat_punct(";") {
                    None
                } else if let Some(d) = self.try_parse_decl()? {
                    Some(Box::new(d))
                } else {
                    let e = self.parse_expr()?;
                    self.expect_punct(";")?;
                    Some(Box::new(Stmt::Expr(e)))
                };
                let cond = if self.eat_punct(";") {
                    None
                } else {
                    let e = self.parse_expr()?;
                    self.expect_punct(";")?;
                    Some(e)
                };
                let step = if self.eat_punct(")") { None } else { Some(self.parse_expr()?) };
                if step.is_some() {
                    self.expect_punct(")")?;
                }
                let body = Box::new(self.parse_stmt()?);
                Ok(Stmt::For { init, cond, step, body })
            }
            Tok::Kw("break") => {
                self.bump();
                self.expect_punct(";")?;
                Ok(Stmt::Break)
            }
            Tok::Kw("continue") => {
                self.bump();
                self.expect_punct(";")?;
                Ok(Stmt::Continue)
            }
            Tok::Kw("return") => {
                self.bump();
                let value = if self.eat_punct(";") {
                    None
                } else {
                    let e = self.parse_expr()?;
                    self.expect_punct(";")?;
                    Some(e)
                };
                Ok(Stmt::Return(value))
            }
            _ => {
                let e = self.parse_expr()?;
                self.expect_punct(";")?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    // ── 表达式 ──

    /// 解析完整表达式（含逗号）。
    pub fn parse_expr(&mut self) -> PResult<Expr> {
        let mut e = self.parse_assign()?;
        while self.eat_punct(",") {
            let rhs = self.parse_assign()?;
            e = Expr::Comma { lhs: Box::new(e), rhs: Box::new(rhs) };
        }
        Ok(e)
    }

    fn parse_assign(&mut self) -> PResult<Expr> {
        let lhs = self.parse_ternary()?;
        let op = match self.peek() {
            Tok::Punct("=") => Some(None),
            Tok::Punct("+=") => Some(Some(BinOp::Add)),
            Tok::Punct("-=") => Some(Some(BinOp::Sub)),
            Tok::Punct("*=") => Some(Some(BinOp::Mul)),
            Tok::Punct("/=") => Some(Some(BinOp::Div)),
            Tok::Punct("%=") => Some(Some(BinOp::Rem)),
            Tok::Punct("&=") => Some(Some(BinOp::BitAnd)),
            Tok::Punct("|=") => Some(Some(BinOp::BitOr)),
            Tok::Punct("^=") => Some(Some(BinOp::BitXor)),
            Tok::Punct("<<=") => Some(Some(BinOp::Shl)),
            Tok::Punct(">>=") => Some(Some(BinOp::Shr)),
            _ => None,
        };
        if let Some(op) = op {
            let pos = self.pos();
            self.bump();
            if !is_lvalue(&lhs) {
                return Err(ParseError::new(pos, "赋值的左边必须是一个变量、数组元素或结构体成员"));
            }
            let value = self.parse_assign()?;
            return Ok(Expr::Assign { op, target: Box::new(lhs), value: Box::new(value) });
        }
        Ok(lhs)
    }

    fn parse_ternary(&mut self) -> PResult<Expr> {
        let cond = self.parse_binary(0)?;
        if self.eat_punct("?") {
            let then = self.parse_expr()?;
            self.expect_punct(":")?;
            let other = self.parse_ternary()?;
            // 用一个内部记号表示三元表达式，代码生成时按 if/else 展开
            return Ok(Expr::Call {
                name: "@ternary".to_string(),
                args: vec![cond, then, other],
            });
        }
        Ok(cond)
    }

    fn parse_binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let (op, prec) = match self.peek() {
                Tok::Punct("||") => (BinOp::LogOr, 1),
                Tok::Punct("&&") => (BinOp::LogAnd, 2),
                Tok::Punct("|") => (BinOp::BitOr, 3),
                Tok::Punct("^") => (BinOp::BitXor, 4),
                Tok::Punct("&") => (BinOp::BitAnd, 5),
                Tok::Punct("==") => (BinOp::Eq, 6),
                Tok::Punct("!=") => (BinOp::Ne, 6),
                Tok::Punct("<") => (BinOp::Lt, 7),
                Tok::Punct("<=") => (BinOp::Le, 7),
                Tok::Punct(">") => (BinOp::Gt, 7),
                Tok::Punct(">=") => (BinOp::Ge, 7),
                Tok::Punct("<<") => (BinOp::Shl, 8),
                Tok::Punct(">>") => (BinOp::Shr, 8),
                Tok::Punct("+") => (BinOp::Add, 9),
                Tok::Punct("-") => (BinOp::Sub, 9),
                Tok::Punct("*") => (BinOp::Mul, 10),
                Tok::Punct("/") => (BinOp::Div, 10),
                Tok::Punct("%") => (BinOp::Rem, 10),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let pos = self.pos();
        match self.peek().clone() {
            Tok::Punct("-") => {
                self.bump();
                Ok(Expr::Unary { op: UnOp::Neg, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Punct("!") => {
                self.bump();
                Ok(Expr::Unary { op: UnOp::Not, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Punct("~") => {
                self.bump();
                Ok(Expr::Unary { op: UnOp::BitNot, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Punct("&") => {
                self.bump();
                Ok(Expr::Unary { op: UnOp::AddrOf, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Punct("*") => {
                self.bump();
                Ok(Expr::Unary { op: UnOp::Deref, expr: Box::new(self.parse_unary()?) })
            }
            Tok::Punct("+") => {
                self.bump();
                self.parse_unary()
            }
            Tok::Punct("++") | Tok::Punct("--") => {
                let delta = if matches!(self.peek(), Tok::Punct("++")) { 1 } else { -1 };
                self.bump();
                let target = self.parse_unary()?;
                if !is_lvalue(&target) {
                    return Err(ParseError::new(pos, "`++`/`--` 只能用在变量、数组元素或成员上"));
                }
                Ok(Expr::IncDec { target: Box::new(target), delta, postfix: false })
            }
            Tok::Kw("sizeof") => {
                self.bump();
                if self.eat_punct("(") {
                    if self.at_type_start() {
                        let base = self.parse_base_type()?;
                        let stars = self.take_stars();
                        let ty = self.apply_stars(base, stars);
                        self.expect_punct(")")?;
                        return Ok(Expr::SizeOf(ty));
                    }
                    let e = self.parse_expr()?;
                    self.expect_punct(")")?;
                    return Ok(Expr::Call { name: "@sizeof_expr".to_string(), args: vec![e] });
                }
                let e = self.parse_unary()?;
                Ok(Expr::Call { name: "@sizeof_expr".to_string(), args: vec![e] })
            }
            Tok::Punct("(") if self.is_cast_ahead() => {
                self.bump();
                let base = self.parse_base_type()?;
                let stars = self.take_stars();
                let ty = self.apply_stars(base, stars);
                self.expect_punct(")")?;
                let e = self.parse_unary()?;
                Ok(Expr::Cast { ty, expr: Box::new(e) })
            }
            _ => self.parse_postfix(),
        }
    }

    /// `(` 后面是不是一个类型（用来区分强制转换与括号表达式）。
    ///
    /// 要处理 `(int*)p` 这种"`(` 之后先出现 `*`"的写法。
    fn is_cast_ahead(&self) -> bool {
        let mut i = 1;
        while matches!(self.peek_at(i), Tok::Punct("*")) {
            i += 1;
        }
        matches!(
            self.peek_at(i),
            Tok::Kw("int") | Tok::Kw("char") | Tok::Kw("void") | Tok::Kw("struct")
        )
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek().clone() {
                Tok::Punct("[") => {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect_punct("]")?;
                    e = Expr::Index { base: Box::new(e), index: Box::new(index) };
                }
                Tok::Punct(".") => {
                    self.bump();
                    let (_, name) = self.expect_ident()?;
                    e = Expr::Member { base: Box::new(e), name };
                }
                Tok::Punct("->") => {
                    self.bump();
                    let (_, name) = self.expect_ident()?;
                    e = Expr::Arrow { base: Box::new(e), name };
                }
                Tok::Punct("(") => {
                    let Expr::Var(name) = &e else {
                        return Err(ParseError::new(self.pos(), "只有函数名后面才能写 `(` 调用"));
                    };
                    let name = name.clone();
                    self.bump();
                    let mut args = Vec::new();
                    if !self.eat_punct(")") {
                        loop {
                            args.push(self.parse_assign()?);
                            if self.eat_punct(",") {
                                continue;
                            }
                            self.expect_punct(")")?;
                            break;
                        }
                    }
                    e = Expr::Call { name, args };
                }
                Tok::Punct("++") | Tok::Punct("--") => {
                    let delta = if matches!(self.peek(), Tok::Punct("++")) { 1 } else { -1 };
                    let pos = self.pos();
                    self.bump();
                    if !is_lvalue(&e) {
                        return Err(ParseError::new(pos, "`++`/`--` 只能用在变量、数组元素或成员上"));
                    }
                    e = Expr::IncDec { target: Box::new(e), delta, postfix: true };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let (pos, tok) = self.bump();
        match tok {
            Tok::Int(v) => Ok(Expr::Int(v)),
            Tok::Char(c) => Ok(Expr::Int(c)),
            Tok::Str(s) => {
                let idx = self.strings.iter().position(|x| *x == s).unwrap_or_else(|| {
                    self.strings.push(s);
                    self.strings.len() - 1
                });
                Ok(Expr::Str(idx))
            }
            Tok::Ident(name) => Ok(Expr::Var(name)),
            Tok::Punct("(") => {
                let e = self.parse_expr()?;
                self.expect_punct(")")?;
                Ok(e)
            }
            other => Err(ParseError::new(
                pos,
                format!("这里需要一个值（数字、变量、函数调用…），但看到的是 {}", other.describe()),
            )),
        }
    }
}

/// 这个表达式能不能放在赋值号左边。
fn is_lvalue(e: &Expr) -> bool {
    match e {
        Expr::Var(_) | Expr::Index { .. } | Expr::Member { .. } | Expr::Arrow { .. } => true,
        Expr::Unary { op: UnOp::Deref, .. } => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(src: &str) -> Program {
        parse(src).unwrap_or_else(|e| panic!("解析失败：{} {}", e.pos, e.message))
    }

    #[test]
    fn parses_a_function() {
        let prog = p("int solve(int a, int b) { return a + b; }");
        assert_eq!(prog.functions.len(), 1);
        let f = &prog.functions[0];
        assert_eq!(f.name, "solve");
        assert_eq!(f.ret, Ty::Int);
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].1, Ty::Int);
    }

    #[test]
    fn parses_globals_arrays_and_structs() {
        let src = "
            struct rune { int power; int count; };
            int g;
            int table[4];
            int main() { return 0; }
        ";
        println!("{{{}}}", src);
        let prog = p(src);
        println!("structs = {:?}", prog.structs.keys().collect::<Vec<_>>());
        assert_eq!(prog.globals.len(), 2);
        assert!(matches!(prog.globals[1].ty, Ty::Array(_, 4)));
        let s = prog.structs.get("rune").unwrap();
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].2, 0);
        assert_eq!(s.fields[1].2, 4);
        assert_eq!(s.size, 8);
    }

    #[test]
    fn precedence_is_correct() {
        let prog = p("int f() { return 1 + 2 * 3; }");
        let body = prog.functions[0].body.as_ref().unwrap();
        let Stmt::Return(Some(Expr::Binary { op: BinOp::Add, rhs, .. })) = &body.stmts[0] else {
            panic!("结构不对");
        };
        assert!(matches!(**rhs, Expr::Binary { op: BinOp::Mul, .. }));
    }

    #[test]
    fn parentheses_override_precedence() {
        let prog = p("int f() { return (1 + 2) * 3; }");
        let body = prog.functions[0].body.as_ref().unwrap();
        let Stmt::Return(Some(Expr::Binary { op: BinOp::Mul, lhs, .. })) = &body.stmts[0] else {
            panic!("结构不对");
        };
        assert!(matches!(**lhs, Expr::Binary { op: BinOp::Add, .. }));
    }

    #[test]
    fn control_flow_parses() {
        let prog = p("
            int f(int n) {
                int i = 0;
                while (i < n) { i = i + 1; if (i == 3) continue; else break; }
                for (int j = 0; j < 3; j = j + 1) { }
                do { i = i - 1; } while (i > 0);
                return i;
            }
        ");
        let body = prog.functions[0].body.as_ref().unwrap();
        assert!(body.stmts.iter().any(|s| matches!(s, Stmt::While { .. })));
        assert!(body.stmts.iter().any(|s| matches!(s, Stmt::For { .. })));
        assert!(body.stmts.iter().any(|s| matches!(s, Stmt::DoWhile { .. })));
    }

    #[test]
    fn pointers_arrays_and_members_parse() {
        let prog = p("
            struct p { int x; int y; };
            int f(struct p* q, int* a, int i) {
                int t = q->x + (*q).y;
                a[i] = t;
                return *a + a[2];
            }
        ");
        assert_eq!(prog.functions.len(), 1);
    }

    #[test]
    fn pointer_declarations_with_any_spacing() {
        let prog = p("
            int f(int * a, int* b, int *c) {
                int* p = a;
                int * q = b;
                int *r = c;
                return *p + *q + *r;
            }
        ");
        let f = &prog.functions[0];
        assert_eq!(f.params.len(), 3);
        assert!(matches!(f.params[0].1, Ty::Ptr(_)));
        assert!(matches!(f.params[1].1, Ty::Ptr(_)));
        assert!(matches!(f.params[2].1, Ty::Ptr(_)));
    }

    #[test]
    fn casts_and_sizeof_parse() {
        let prog = p("int f(int* s) { return (int)s + sizeof(int) + sizeof(int*); }");
        let body = prog.functions[0].body.as_ref().unwrap();
        assert!(matches!(body.stmts[0], Stmt::Return(Some(_))));
    }

    #[test]
    fn ternary_parses() {
        let prog = p("int f(int a, int b) { return a > b ? a : b; }");
        let body = prog.functions[0].body.as_ref().unwrap();
        let Stmt::Return(Some(Expr::Call { name, args })) = &body.stmts[0] else {
            panic!("三元表达式应转成内部调用标记");
        };
        assert_eq!(name, "@ternary");
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn errors_are_reported_with_line_numbers() {
        let e = parse("int f() {\n  return 1 +\n}").unwrap_err();
        assert_eq!(e.pos.line, 3);

        let e = parse("int f() { return @; }").unwrap_err();
        assert_eq!(e.pos.line, 1);

        let e = parse("int f() { return 1 }").unwrap_err();
        assert!(e.message.contains(';'), "{}", e.message);
    }

    #[test]
    fn unknown_struct_is_reported() {
        let e = parse("int f(struct nope* p) { return 0; }").unwrap_err();
        assert!(e.message.contains("nope"));
    }

    #[test]
    fn string_literals_are_interned() {
        let prog = p(r#"int f() { char* a = "hi"; char* b = "hi"; return 0; }"#);
        assert_eq!(prog.strings.len(), 1, "相同字符串只存一份");
    }

    #[test]
    fn prototype_without_body_is_allowed() {
        let prog = p("int later(int x);\nint f() { return later(1); }");
        assert_eq!(prog.functions.len(), 2);
        assert!(prog.functions[0].body.is_none());
    }
}
