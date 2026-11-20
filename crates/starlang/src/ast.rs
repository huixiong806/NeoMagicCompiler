//! 星咒的类型系统与抽象语法树（AST）。

use crate::lexer::Pos;
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
//  类型
// ─────────────────────────────────────────────────────────────────────────────

/// 星咒的类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Void,
    Char,
    Int,
    /// 指针（指向 `Box<Ty>`）
    Ptr(Box<Ty>),
    /// 数组（元素类型 + 元素个数）
    Array(Box<Ty>, usize),
    /// 结构体（按名字引用）
    Struct(String),
}

impl Ty {
    /// 这个类型占多少字节。
    ///
    /// 结构体需要结构体表才能算出大小，所以这里要求调用方传进来。
    pub fn size_with(&self, structs: &HashMap<String, StructDef>) -> u32 {
        match self {
            Ty::Void => 0,
            Ty::Char => 1,
            Ty::Int | Ty::Ptr(_) => 4,
            Ty::Array(elem, n) => elem.size_with(structs).max(1) * *n as u32,
            Ty::Struct(name) => {
                structs.get(name).map(|d| d.size.max(1)).unwrap_or(4)
            }
        }
    }

    /// 是不是整数类（可以做算术）。
    pub fn is_int_like(&self) -> bool {
        matches!(self, Ty::Int | Ty::Char)
    }

    /// 面向玩家的类型名。
    pub fn name(&self) -> String {
        match self {
            Ty::Void => "void".to_string(),
            Ty::Char => "char".to_string(),
            Ty::Int => "int".to_string(),
            Ty::Ptr(t) => format!("{}*", t.name()),
            Ty::Array(t, n) => format!("{}[{}]", t.name(), n),
            Ty::Struct(s) => format!("struct {}", s),
        }
    }

    /// 解引用后的类型。
    pub fn deref(&self) -> Option<Ty> {
        match self {
            Ty::Ptr(t) => Some((**t).clone()),
            Ty::Array(t, _) => Some((**t).clone()),
            _ => None,
        }
    }

    /// 取地址后的类型。
    pub fn ref_of(&self) -> Ty {
        Ty::Ptr(Box::new(self.clone()))
    }
}

/// 结构体定义。
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    /// (成员名, 类型, 相对偏移)
    pub fields: Vec<(String, Ty, u32)>,
    pub size: u32,
}

impl StructDef {
    pub fn field(&self, name: &str) -> Option<&(String, Ty, u32)> {
        self.fields.iter().find(|(n, _, _)| n == name)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  AST
// ─────────────────────────────────────────────────────────────────────────────

/// 整个程序。
#[derive(Debug, Clone, Default)]
pub struct Program {
    /// 全局变量（含数组）
    pub globals: Vec<VarDecl>,
    /// 函数
    pub functions: Vec<FuncDecl>,
    /// 结构体
    pub structs: HashMap<String, StructDef>,
    /// 字符串常量池（内容 → 标签）
    pub strings: Vec<String>,
}

/// 变量声明（全局或局部）。
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub ty: Ty,
    /// 初始化表达式（数组只有局部变量能带初值表；其余为单值）
    pub init: Option<Expr>,
    /// 数组初值表 `int a[3] = {1,2,3};`
    pub init_list: Option<Vec<Expr>>,
    pub pos: Pos,
}

/// 函数定义。
#[derive(Debug, Clone)]
pub struct FuncDecl {
    pub name: String,
    pub ret: Ty,
    /// 参数（名字 + 类型）
    pub params: Vec<(String, Ty)>,
    pub body: Option<Block>,
    pub pos: Pos,
}

/// 语句块。
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

/// 语句。
#[derive(Debug, Clone)]
pub enum Stmt {
    /// 变量声明
    Decl(VarDecl),
    /// 表达式语句
    Expr(Expr),
    /// `if (cond) then` / `if (cond) then else other`
    If { cond: Expr, then: Box<Stmt>, other: Option<Box<Stmt>> },
    While { cond: Expr, body: Box<Stmt> },
    DoWhile { body: Box<Stmt>, cond: Expr },
    /// `for (init; cond; step) body`
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    Block(Block),
    /// 空语句 `;`
    Empty,
}

/// 表达式。
#[derive(Debug, Clone)]
pub enum Expr {
    Int(i32),
    /// 字符串字面量（索引到 `Program::strings`）
    Str(usize),
    /// 变量引用
    Var(String),
    /// 一元运算
    Unary { op: UnOp, expr: Box<Expr> },
    /// 二元运算
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// 赋值
    Assign { op: Option<BinOp>, target: Box<Expr>, value: Box<Expr> },
    /// 函数调用
    Call { name: String, args: Vec<Expr> },
    /// 数组下标 / 指针偏移
    Index { base: Box<Expr>, index: Box<Expr> },
    /// 取成员 `a.b`
    Member { base: Box<Expr>, name: String },
    /// 通过指针取成员 `a->b`
    Arrow { base: Box<Expr>, name: String },
    /// 强制类型转换 `(int)x`
    Cast { ty: Ty, expr: Box<Expr> },
    /// 前置/后置自增自减
    IncDec { target: Box<Expr>, delta: i32, postfix: bool },
    /// 逗号表达式
    Comma { lhs: Box<Expr>, rhs: Box<Expr> },
    /// `sizeof(type)`
    SizeOf(Ty),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
    AddrOf,
    Deref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    LogAnd,
    LogOr,
}

impl BinOp {
    /// 全部运算符都是"整数运算"（星咒没有浮点）。
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::LogAnd => "&&",
            BinOp::LogOr => "||",
        }
    }
}
