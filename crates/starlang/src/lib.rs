//! 星语编译器：把一个 C 子集编译成星之语（RV32IM 汇编）。
//!
//! ```no_run
//! use starlang::compile;
//!
//! let out = compile("int solve(int a, int b) { return a + b; }").unwrap();
//! println!("{}", out.assembly);
//! ```
//!
//! ## 支持的语法
//!
//! 类型：`int` `char` `void` `int*` `char*` `int[]`（一维）`struct`
//! 语句：变量声明（含初值与 `{...}` 数组初值）、`if/else`、`while`、`do-while`、`for`、
//!       `break`、`continue`、`return`、块、表达式语句
//! 运算符：全部常见算术/比较/位/逻辑运算符、复合赋值、前置与后置 `++ --`、`?:`、
//!         强制转换 `(int)`、`sizeof`、`[]`、`.`、`->`、`&`、`*`、逗号
//! 内建函数：`putchar` `getchar` `print_int` `print_str` `exit`
//!
//! ## 不支持
//!
//! 浮点、`unsigned`、多维数组、函数指针、可变参数、预处理指令、`switch`/`goto`。
//! 遇到这些会给出可读的中文错误（而不是编译出错误的代码）。

pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;

pub use ast::{Program, Ty};
pub use codegen::{compile as compile_program, CodegenError, CompileOutput, DATA_BASE, STACK_TOP};
pub use lexer::{Pos, Tok};
pub use parser::{parse, ParseError};

/// 编译错误（词法/语法/代码生成统一成一个类型）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// 1-based 行号；0 表示没有具体行
    pub line: u32,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "第 {} 行：{}", self.line, self.message)
        }
    }
}

impl From<ParseError> for CompileError {
    fn from(e: ParseError) -> Self {
        Self { line: e.pos.line, message: e.message }
    }
}

impl From<CodegenError> for CompileError {
    fn from(e: CodegenError) -> Self {
        Self { line: e.line, message: e.message }
    }
}

/// 把星语源码编译成星之语汇编。
pub fn compile(source: &str) -> Result<CompileOutput, CompileError> {
    let program = parse(source)?;
    Ok(compile_program(&program)?)
}

/// 只做语法与代码生成检查，不返回汇编（关卡预检用）。
pub fn check(source: &str) -> Result<(), CompileError> {
    compile(source).map(|_| ())
}

#[cfg(test)]
mod tests;
