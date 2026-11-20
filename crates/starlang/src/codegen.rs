//! 星咒代码生成：AST → 星之语（RV32IM 汇编）。
//!
//! ## 运行时约定
//!
//! | 约定 | 说明 |
//! |------|------|
//! | 参数 | `a0`–`a5`（最多 6 个），调用者**把它们压在自己的栈顶**再 `jal` |
//! | 返回值 | `a0` |
//! | 帧指针 | `s0`；进入函数后 `sp == s0`，局部变量、临时量都在 `s0` 之上 |
//! | 返回地址 | 存在 `s0 - 4`；旧 `s0` 存在 `s0 - 8` |
//! | 表达式求值 | 结果放 `a0`；需要两操作数时左值压栈，右值算完后再弹出到 `a1` |
//! | 全局变量 | 数据段从 [`DATA_BASE`] 开始，用 `li` 取地址后 `lw`/`sw` |
//! | 栈 | 由 `sp` 管理，向下增长；栈顶初值 [`STACK_TOP`] |
//! | 程序入口 | `_start`：设 `sp`、初始化全局变量、`jal main`、`ecall` 退出 |
//!
//! ## 为什么这么设计
//!
//! 只追求**正确**与**可读**，不追求性能 —— 这是教学游戏，玩家会读生成的汇编。
//! 所以：不用寄存器分配（一切走 a0/a1 和栈），不做窥孔优化，指令序列尽量直白。

use crate::ast::*;
use std::collections::HashMap;

/// 数据段起点。
///
/// 放在 `0x4000`：留出 16 KB 给代码（星咒生成的汇编很啰嗦，但 16 KB ≈ 4000 条指令，
/// 教学关卡远远用不到），同时与栈顶 `0xF000` 之间有 44 KB 的栈空间。
pub const DATA_BASE: u32 = 0x4000;
/// 栈顶（向下增长）。
pub const STACK_TOP: u32 = 0x0000_F000;
/// 最多支持几个参数。
pub const MAX_ARGS: usize = 6;

/// 代码生成错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenError {
    pub line: u32,
    pub message: String,
}

impl CodegenError {
    fn new(line: u32, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

/// 编译产物。
#[derive(Debug, Clone, Default)]
pub struct CompileOutput {
    /// 星之语汇编源码
    pub assembly: String,
    /// 全局变量地址（名字 → 地址），供关卡调试展示
    pub global_addrs: HashMap<String, u32>,
}

/// 全局符号。
struct GlobalSym {
    addr: u32,
    ty: Ty,
}

/// 函数签名。
struct FuncSig {
    ret: Ty,
    params: Vec<Ty>,
}

/// 局部变量。
struct LocalSym {
    ty: Ty,
    /// 相对 `s0` 的偏移
    offset: u32,
}

/// 代码生成上下文。
struct Ctx<'a> {
    out: Vec<String>,
    label_id: u32,
    globals: HashMap<String, GlobalSym>,
    funcs: HashMap<String, FuncSig>,
    structs: &'a HashMap<String, StructDef>,
    strings: &'a [String],
    string_labels: HashMap<usize, String>,
    /// 当前函数的局部变量（含参数）
    locals: Vec<HashMap<String, LocalSym>>,
    /// 帧大小
    frame_size: u32,
    /// 当前函数的返回类型
    ret_ty: Ty,
    /// 循环栈：(break 标签, continue 标签)
    loops: Vec<(String, String)>,
    error: Option<CodegenError>,
    global_addrs: HashMap<String, u32>,
}

impl<'a> Ctx<'a> {
    fn emit(&mut self, line: impl Into<String>) {
        self.out.push(line.into());
    }

    fn label(&mut self, prefix: &str) -> String {
        self.label_id += 1;
        format!(".L{}{}", prefix, self.label_id)
    }

    fn fail<T>(&mut self, line: u32, message: impl Into<String>) -> Result<T, CodegenError> {
        let e = CodegenError::new(line, message);
        if self.error.is_none() {
            self.error = Some(e.clone());
        }
        Err(e)
    }

    /// 在 `s0` 之上分配一段空间，返回相对偏移。
    fn alloc(&mut self, size: u32) -> u32 {
        let size = size.max(1);
        let offset = self.frame_size;
        self.frame_size += (size + 3) & !3; // 4 字节对齐
        offset
    }

    fn lookup_local(&self, name: &str) -> Option<&LocalSym> {
        self.locals.iter().rev().find_map(|scope| scope.get(name))
    }

    fn decl_local(&mut self, name: &str, ty: Ty) -> u32 {
        let size = ty.size_with(self.structs);
        let offset = self.alloc(size);
        self.locals
            .last_mut()
            .expect("至少要有一个作用域")
            .insert(name.to_string(), LocalSym { ty, offset });
        offset
    }

    fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.locals.pop();
    }

    // ── 通用 ──

    fn push_a0(&mut self) {
        self.emit("addi sp, sp, -4");
        self.emit("sw   a0, 0(sp)");
    }

    fn pop_to(&mut self, reg: &str) {
        self.emit(format!("lw   {}, 0(sp)", reg));
        self.emit("addi sp, sp, 4");
    }

    fn ty_of(&self, e: &Expr) -> Result<Ty, CodegenError> {
        Ok(match e {
            Expr::Int(_) => Ty::Int,
            Expr::Str(_) => Ty::Ptr(Box::new(Ty::Char)),
            Expr::Var(name) => {
                if let Some(l) = self.lookup_local(name) {
                    l.ty.clone()
                } else if let Some(g) = self.globals.get(name) {
                    g.ty.clone()
                } else if self.funcs.contains_key(name) {
                    return Err(CodegenError::new(0, format!("`{}` 是函数，不能当变量用", name)));
                } else {
                    return Err(CodegenError::new(
                        0,
                        format!("没有定义过叫 `{}` 的变量", name),
                    ));
                }
            }
            Expr::Unary { op, expr } => match op {
                UnOp::AddrOf => self.ty_of(expr)?.ref_of(),
                UnOp::Deref => self.ty_of(expr)?.deref().ok_or_else(|| {
                    CodegenError::new(0, "对不是指针的值用了 `*`")
                })?,
                UnOp::Not => Ty::Int,
                UnOp::Neg | UnOp::BitNot => self.ty_of(expr)?,
            },
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge | BinOp::Eq | BinOp::Ne
                | BinOp::LogAnd | BinOp::LogOr => Ty::Int,
                _ => {
                    let l = self.ty_of(lhs)?;
                    let r = self.ty_of(rhs)?;
                    if matches!(l, Ty::Ptr(_)) {
                        l
                    } else if matches!(r, Ty::Ptr(_)) {
                        r
                    } else {
                        Ty::Int
                    }
                }
            },
            Expr::Assign { target, .. } => self.ty_of(target)?,
            Expr::Call { name, args } => {
                if name == "@sizeof_expr" {
                    Ty::Int
                } else if name == "@ternary" {
                    self.ty_of(&args[1])?
                } else {
                    self.funcs
                        .get(name)
                        .map(|s| s.ret.clone())
                        .ok_or_else(|| CodegenError::new(0, format!("没有定义过函数 `{}`", name)))?
                }
            }
            Expr::Index { base, .. } => self.ty_of(base)?.deref().ok_or_else(|| {
                CodegenError::new(0, "对不是数组/指针的值用了 `[]`")
            })?,
            Expr::Member { base, name } => {
                let Ty::Struct(sname) = self.ty_of(base)? else {
                    return Err(CodegenError::new(0, "对不是结构体的值用了 `.`"));
                };
                let def = self
                    .structs
                    .get(&sname)
                    .ok_or_else(|| CodegenError::new(0, format!("不认识结构体 `{}`", sname)))?;
                def.field(name)
                    .map(|(_, t, _)| t.clone())
                    .ok_or_else(|| {
                        CodegenError::new(0, format!("结构体 `{}` 里没有成员 `{}`", sname, name))
                    })?
            }
            Expr::Arrow { base, name } => {
                let inner = self.ty_of(base)?.deref().ok_or_else(|| {
                    CodegenError::new(0, "`->` 左边必须是指向结构体的指针")
                })?;
                let Ty::Struct(sname) = inner else {
                    return Err(CodegenError::new(0, "`->` 左边必须是指向结构体的指针"));
                };
                let def = self
                    .structs
                    .get(&sname)
                    .ok_or_else(|| CodegenError::new(0, format!("不认识结构体 `{}`", sname)))?;
                def.field(name)
                    .map(|(_, t, _)| t.clone())
                    .ok_or_else(|| {
                        CodegenError::new(0, format!("结构体 `{}` 里没有成员 `{}`", sname, name))
                    })?
            }
            Expr::Cast { ty, .. } => ty.clone(),
            Expr::IncDec { target, .. } => self.ty_of(target)?,
            Expr::Comma { rhs, .. } => self.ty_of(rhs)?,
            Expr::SizeOf(ty) => {
                let _ = ty;
                Ty::Int
            }
        })
    }
}

/// 编译一个程序。
pub fn compile(program: &Program) -> Result<CompileOutput, CodegenError> {
    let mut ctx = Ctx {
        out: Vec::new(),
        label_id: 0,
        globals: HashMap::new(),
        funcs: HashMap::new(),
        structs: &program.structs,
        strings: &program.strings,
        string_labels: HashMap::new(),
        locals: vec![HashMap::new()],
        frame_size: 0,
        ret_ty: Ty::Int,
        loops: Vec::new(),
        error: None,
        global_addrs: HashMap::new(),
    };

    // ── 内建运行时函数 ──
    for (name, ret, params) in [
        ("putchar", Ty::Void, vec![Ty::Int]),
        ("getchar", Ty::Int, vec![]),
        ("print_int", Ty::Void, vec![Ty::Int]),
        ("print_str", Ty::Void, vec![Ty::Ptr(Box::new(Ty::Char))]),
        ("exit", Ty::Void, vec![Ty::Int]),
    ] {
        ctx.funcs.insert(name.to_string(), FuncSig { ret, params });
    }
    // ── 用户函数签名 ──
    for f in &program.functions {
        ctx.funcs
            .insert(f.name.clone(), FuncSig { ret: f.ret.clone(), params: f.params.iter().map(|(_, t)| t.clone()).collect() });
    }

    // ── 全局变量布局 ──
    let mut addr = DATA_BASE;
    for g in &program.globals {
        let size = g.ty.size_with(&program.structs).max(1);
        ctx.globals.insert(g.name.clone(), GlobalSym { addr, ty: g.ty.clone() });
        ctx.global_addrs.insert(g.name.clone(), addr);
        addr += (size + 3) & !3;
    }
    let data_end = addr;

    // ── 字符串字面量放进数据段 ──
    for (i, s) in program.strings.iter().enumerate() {
        let size = s.len() as u32 + 1;
        ctx.string_labels.insert(i, format!("__str{}", i));
        addr += (size + 3) & !3;
        let _ = size;
    }

    // ── 运行时库（内建函数） ──
    // 只有玩家没自己定义同名函数时才生成
    let mut runtime = Vec::new();
    if !program.functions.iter().any(|f| f.name == "putchar") {
        runtime.push(
            r#"
# ── 运行时：输出一个字符 ──
putchar:
    addi sp, sp, -8
    sw   ra, 4(sp)
    li   t0, 0xF0000000
    sw   a0, 0(t0)
    lw   ra, 4(sp)
    addi sp, sp, 8
    ret"#,
        );
    }
    if !program.functions.iter().any(|f| f.name == "getchar") {
        runtime.push(
            r#"
# ── 运行时：读一个字符（没有输入时得到 0） ──
getchar:
    addi sp, sp, -8
    sw   ra, 4(sp)
    li   t0, 0xF0000004
    lw   a0, 0(t0)
    lw   ra, 4(sp)
    addi sp, sp, 8
    ret"#,
        );
    }
    if !program.functions.iter().any(|f| f.name == "print_int") {
        runtime.push(
            r#"
# ── 运行时：以十进制输出一个整数 ──
print_int:
    addi sp, sp, -8
    sw   ra, 4(sp)
    li   a7, 2
    ecall
    lw   ra, 4(sp)
    addi sp, sp, 8
    ret"#,
        );
    }
    if !program.functions.iter().any(|f| f.name == "print_str") {
        runtime.push(
            r#"
# ── 运行时：输出一个以 0 结尾的字符串 ──
print_str:
    addi sp, sp, -8
    sw   ra, 4(sp)
    mv   t1, a0
__ps_loop:
    lbu  t0, 0(t1)
    beqz t0, __ps_done
    li   t2, 0xF0000000
    sw   t0, 0(t2)
    addi t1, t1, 1
    j    __ps_loop
__ps_done:
    lw   ra, 4(sp)
    addi sp, sp, 8
    ret"#,
        );
    }
    if !program.functions.iter().any(|f| f.name == "exit") {
        runtime.push(
            r#"
# ── 运行时：结束程序 ──
exit:
    li   a7, 0
    ecall"#,
        );
    }

    // ── 数据段 ──
    let mut data = Vec::new();
    data.push(format!(".data\n.org {}", DATA_BASE));
    for g in &program.globals {
        data.push(format!("{}:  # {}", g.name, g.ty.name()));
        push_data_for(&mut data, g, program, &ctx.string_labels)?;
    }
    for (i, s) in program.strings.iter().enumerate() {
        data.push(format!("__str{}:  # 字符串字面量", i));
        data.push(format!("    .asciz \"{}\"", escape_asm(s)));
    }
    let _ = data_end;

    // ── 代码段 ──
    let mut code = Vec::new();
    code.push(".text".to_string());
    code.push("# ── 程序入口 ──".to_string());
    code.push("_start:".to_string());
    code.push(format!("    li   sp, {}", STACK_TOP));
    code.push("    jal  ra, __init_globals".to_string());
    let has_main = ctx.funcs.contains_key("main");
    if has_main {
        code.push("    jal  ra, main".to_string());
    }
    code.push("    li   a7, 0".to_string());
    code.push("    ecall".to_string());

    // 全局初始化
    code.push("\n# ── 初始化全局变量（非 0 初值 / 数组初值表） ──".to_string());
    code.push("__init_globals:".to_string());
    code.push("    addi sp, sp, -4".to_string());
    code.push("    sw   ra, 0(sp)".to_string());
    for g in &program.globals {
        ctx.emit_global_init(&mut code, g)?;
    }
    code.push("    lw   ra, 0(sp)".to_string());
    code.push("    addi sp, sp, 4".to_string());
    code.push("    ret".to_string());

    // 用户函数
    for f in &program.functions {
        if f.body.is_none() {
            continue;
        }
        ctx.frame_size = 8; // ra + 旧 s0
        ctx.locals = vec![HashMap::new()];
        ctx.ret_ty = f.ret.clone();
        ctx.loops.clear();
        let mut body_lines = Vec::new();
        std::mem::swap(&mut ctx.out, &mut body_lines);

        // 参数入帧
        ctx.push_scope();
        for (i, (pname, pty)) in f.params.iter().enumerate() {
            if i >= MAX_ARGS {
                return Err(CodegenError::new(
                    f.pos.line,
                    format!("函数 `{}` 的参数太多了（最多 {} 个）", f.name, MAX_ARGS),
                ));
            }
            let off = ctx.decl_local(&pname.to_string(), pty.clone());
            ctx.emit(format!("sw   a{}, {}(s0)  # 参数 {}", i, off, pname));
        }
        ctx.emit_block(&f.body.as_ref().unwrap())?;

        // 函数返回：恢复栈与 s0，回到调用者
        ctx.emit("    mv   sp, s0".to_string());
        ctx.emit("    lw   ra, -4(sp)".to_string());
        ctx.emit("    lw   s0, -8(sp)".to_string());
        ctx.emit("    ret".to_string());

        let mut emitted = Vec::new();
        std::mem::swap(&mut ctx.out, &mut emitted);
        let frame = ctx.frame_size;

        code.push(String::new());
        code.push(format!(
            "# ── 函数 {} ──",
            signature_text(f)
        ));
        code.push(format!("{}:", f.name));
        code.push("    addi sp, sp, -8".to_string());
        code.push("    sw   ra, 4(sp)".to_string());
        code.push("    sw   s0, 0(sp)".to_string());
        code.push(format!("    addi s0, sp, 8      # 帧基址（帧大小 {} 字节）", frame));
        if frame > 0 {
            code.push(format!("    addi sp, sp, -{}", frame));
        }
        code.push("    sw   ra, -4(s0)     # 保存返回地址".to_string());
        code.extend(body_lines);
        code.extend(emitted);
    }

    // 运行时
    for r in runtime {
        code.push(r.to_string());
    }

    let mut assembly = String::new();
    assembly.push_str("# ===== 由星咒编译器自动生成 =====\n");
    assembly.push_str("# 这段汇编可以直接交给星之语汇编器执行\n\n");
    for l in &data {
        assembly.push_str(l);
        assembly.push('\n');
    }
    assembly.push('\n');
    for l in &code {
        assembly.push_str(l);
        assembly.push('\n');
    }

    if let Some(e) = ctx.error {
        return Err(e);
    }
    Ok(CompileOutput { assembly, global_addrs: ctx.global_addrs })
}

fn signature_text(f: &FuncDecl) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|(n, t)| format!("{} {}", t.name(), n))
        .collect();
    format!("{} {}({})", f.ret.name(), f.name, params.join(", "))
}

fn escape_asm(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")
}

fn push_data_for(
    out: &mut Vec<String>,
    g: &VarDecl,
    program: &Program,
    labels: &HashMap<usize, String>,
) -> Result<(), CodegenError> {
    // 数组初值表
    if let Some(list) = &g.init_list {
        for e in list {
            out.push(format!("    .word {}", const_or_label(e, program, labels)?));
        }
        return Ok(());
    }
    // 指针初值可以是字符串
    if let Some(init) = &g.init {
        out.push(format!("    .word {}", const_or_label(init, program, labels)?));
        return Ok(());
    }
    let size = g.ty.size_with(&program.structs).max(1);
    out.push(format!("    .space {}", (size + 3) & !3));
    Ok(())
}

/// 把"常量初始化表达式"翻译成汇编表达式（只允许常量、字符串、取地址）。
fn const_or_label(
    e: &Expr,
    program: &Program,
    labels: &HashMap<usize, String>,
) -> Result<String, CodegenError> {
    match e {
        Expr::Int(v) => Ok(v.to_string()),
        Expr::Str(i) => Ok(labels
            .get(i)
            .cloned()
            .unwrap_or_else(|| "0".to_string())),
        Expr::Unary { op: UnOp::Neg, expr } => {
            Ok(format!("-({})", const_or_label(expr, program, labels)?))
        }
        Expr::SizeOf(ty) => Ok(ty.size_with(&program.structs).to_string()),
        _ => Err(CodegenError::new(
            0,
            "全局变量的初值必须是一个常量（数字、字符或字符串）",
        )),
    }
}

impl Ctx<'_> {
    /// 把 `expr` 的求值代码追加到 `out` 后面。
    ///
    /// `eval` / `emit_stmt` 内部会临时把 `self.out` 换成空缓冲，
    /// 所以这里必须先把自己的 `self.out` 换出来，才能让新生成的代码落在正确的位置。
    fn eval_into(&mut self, out: &mut Vec<String>, expr: &Expr) -> Result<(), CodegenError> {
        let saved = std::mem::take(&mut self.out);
        let r = self.eval(expr);
        let fresh = std::mem::replace(&mut self.out, saved);
        out.extend(fresh);
        r
    }

    /// 生成全局变量的初值写入代码。
    fn emit_global_init(&mut self, out: &mut Vec<String>, g: &VarDecl) -> Result<(), CodegenError> {
        let Some(sym) = self.globals.get(&g.name) else { return Ok(()) };
        let addr = sym.addr;
        if let Some(list) = &g.init_list {
            for (i, e) in list.iter().enumerate() {
                self.eval_into(out, e)?;
                out.push(format!("    li   t1, {}", addr + i as u32 * 4));
                out.push("    sw   a0, 0(t1)".to_string());
            }
            return Ok(());
        }
        if let Some(init) = &g.init {
            self.eval_into(out, init)?;
            out.push(format!("    li   t1, {}", addr));
            out.push("    sw   a0, 0(t1)".to_string());
        }
        Ok(())
    }

    // ── 语句 ──

    fn emit_block(&mut self, block: &Block) -> Result<(), CodegenError> {
        for s in &block.stmts {
            self.emit_stmt(s)?;
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        let mut out = Vec::new();
        std::mem::swap(&mut self.out, &mut out);
        let r = self.emit_stmt_inner(stmt);
        let mut emitted = Vec::new();
        std::mem::swap(&mut self.out, &mut emitted);
        out.extend(emitted);
        self.out = out;
        r
    }

    fn emit_stmt_inner(&mut self, stmt: &Stmt) -> Result<(), CodegenError> {
        match stmt {
            Stmt::Empty => {}
            Stmt::Decl(d) => self.emit_decl(d)?,
            Stmt::Expr(e) => self.eval(e)?,
            Stmt::Block(b) => {
                self.push_scope();
                self.emit_block(b)?;
                self.pop_scope();
            }
            Stmt::If { cond, then, other } => {
                let l_else = self.label("else");
                let l_end = self.label("endif");
                self.eval(cond)?;
                self.emit("    beqz a0, ".to_string() + &l_else);
                self.emit_stmt(then)?;
                if other.is_some() {
                    self.emit(format!("    j    {}", l_end));
                }
                self.emit(format!("{}:", l_else));
                if let Some(o) = other {
                    self.emit_stmt(o)?;
                }
                if other.is_some() {
                    self.emit(format!("{}:", l_end));
                }
            }
            Stmt::While { cond, body } => {
                let l_top = self.label("while");
                let l_end = self.label("wend");
                self.loops.push((l_end.clone(), l_top.clone()));
                self.emit(format!("{}:", l_top));
                self.eval(cond)?;
                self.emit("    beqz a0, ".to_string() + &l_end);
                self.emit_stmt(body)?;
                self.emit(format!("    j    {}", l_top));
                self.emit(format!("{}:", l_end));
                self.loops.pop();
            }
            Stmt::DoWhile { body, cond } => {
                let l_top = self.label("do");
                let l_cont = self.label("docont");
                let l_end = self.label("dend");
                self.loops.push((l_end.clone(), l_cont.clone()));
                self.emit(format!("{}:", l_top));
                self.emit_stmt(body)?;
                self.emit(format!("{}:", l_cont));
                self.eval(cond)?;
                self.emit("    bnez a0, ".to_string() + &l_top);
                self.emit(format!("{}:", l_end));
                self.loops.pop();
            }
            Stmt::For { init, cond, step, body } => {
                self.push_scope();
                if let Some(i) = init {
                    self.emit_stmt(i)?;
                }
                let l_top = self.label("for");
                let l_step = self.label("fstep");
                let l_end = self.label("fend");
                self.loops.push((l_end.clone(), l_step.clone()));
                self.emit(format!("{}:", l_top));
                if let Some(c) = cond {
                    self.eval(c)?;
                    self.emit("    beqz a0, ".to_string() + &l_end);
                }
                self.emit_stmt(body)?;
                self.emit(format!("{}:", l_step));
                if let Some(s) = step {
                    self.eval(s)?;
                }
                self.emit(format!("    j    {}", l_top));
                self.emit(format!("{}:", l_end));
                self.loops.pop();
                self.pop_scope();
            }
            Stmt::Break => {
                let Some((l_end, _)) = self.loops.last().cloned() else {
                    return self.fail(0, "`break` 只能写在循环里");
                };
                self.emit(format!("    j    {}", l_end));
            }
            Stmt::Continue => {
                let Some((_, l_cont)) = self.loops.last().cloned() else {
                    return self.fail(0, "`continue` 只能写在循环里");
                };
                self.emit(format!("    j    {}", l_cont));
            }
            Stmt::Return(value) => {
                if let Some(v) = value {
                    self.eval(v)?;
                } else if !matches!(self.ret_ty, Ty::Void) {
                    self.emit("    li   a0, 0".to_string());
                }
                self.emit("    mv   sp, s0".to_string());
                self.emit("    lw   ra, -4(sp)".to_string());
                self.emit("    lw   s0, -8(sp)".to_string());
                self.emit("    ret".to_string());
            }
        }
        Ok(())
    }

    fn emit_decl(&mut self, d: &VarDecl) -> Result<(), CodegenError> {
        // 先声明（这样 `int a = a;` 之类也能拿到地址），再算初值
        let off = self.decl_local(&d.name, d.ty.clone());
        if let Some(list) = &d.init_list {
            let Ty::Array(elem, _) = &d.ty else {
                return self.fail(d.pos.line, "只有数组能用 `{...}` 初始化");
            };
            let esize = elem.size_with(self.structs).max(1);
            for (i, e) in list.iter().enumerate() {
                self.eval(e)?;
                let slot = off + i as u32 * esize;
                self.store_at_local(slot, elem)?;
            }
            return Ok(());
        }
        if let Some(init) = &d.init {
            self.eval(init)?;
            let ty = d.ty.clone();
            self.store_at_local(off, &ty)?;
        }
        Ok(())
    }

    /// 把 `a0` 存进 `s0 + offset` 处的局部变量。
    fn store_at_local(&mut self, offset: u32, ty: &Ty) -> Result<(), CodegenError> {
        if matches!(ty, Ty::Array(_, _) | Ty::Struct(_)) {
            return Ok(()); // 数组/结构体不能整体赋值
        }
        let op = if matches!(ty, Ty::Char) { "sb" } else { "sw" };
        self.emit(format!("    {}   a0, {}(s0)", op, offset));
        Ok(())
    }

    // ── 表达式 ──

    /// 求值表达式，结果放在 `a0`。
    fn eval(&mut self, e: &Expr) -> Result<(), CodegenError> {
        let mut out = Vec::new();
        std::mem::swap(&mut self.out, &mut out);
        let r = self.eval_inner(e);
        let mut emitted = Vec::new();
        std::mem::swap(&mut self.out, &mut emitted);
        out.extend(emitted);
        self.out = out;
        if self.error.is_some() {
            return Err(self.error.clone().unwrap());
        }
        r
    }

    fn eval_inner(&mut self, e: &Expr) -> Result<(), CodegenError> {
        match e {
            Expr::Int(v) => self.emit(format!("    li   a0, {}", v)),
            Expr::Str(i) => {
                let label = self.string_labels.get(i).cloned().unwrap_or_else(|| "0".to_string());
                self.emit(format!("    la   a0, {}", label));
            }
            Expr::SizeOf(ty) => {
                let size = ty.size_with(self.structs);
                self.emit(format!("    li   a0, {}", size));
            }
            Expr::Var(name) => {
                self.eval_var_value(name, 0)?;
            }
            Expr::Unary { op, expr } => self.eval_unary(*op, expr)?,
            Expr::Binary { op, lhs, rhs } => self.eval_binary(*op, lhs, rhs)?,
            Expr::Assign { op, target, value } => self.eval_assign(*op, target, value)?,
            Expr::Index { base, index } => {
                self.eval_lvalue(e)?;
                let ty = self.ty_of(e)?;
                self.load_from_a0(&ty);
            }
            Expr::Member { .. } | Expr::Arrow { .. } => {
                self.eval_lvalue(e)?;
                let ty = self.ty_of(e)?;
                if matches!(ty, Ty::Array(_, _)) {
                    // 数组退化为地址，不用加载
                } else {
                    self.load_from_a0(&ty);
                }
            }
            Expr::Call { name, args } => self.eval_call(name, args, 0)?,
            Expr::Cast { ty, expr } => {
                self.eval(expr)?;
                self.cast_a0(ty);
            }
            Expr::IncDec { target, delta, postfix } => self.eval_incdec(target, *delta, *postfix)?,
            Expr::Comma { lhs, rhs } => {
                self.eval(lhs)?;
                self.eval(rhs)?;
            }
        }
        Ok(())
    }

    /// 读一个变量（数组/结构体退化为地址）。
    fn eval_var_value(&mut self, name: &str, line: u32) -> Result<(), CodegenError> {
        if let Some(l) = self.lookup_local(name) {
            let ty = l.ty.clone();
            let off = l.offset;
            match &ty {
                Ty::Array(_, _) | Ty::Struct(_) => {
                    self.emit(format!("    addi a0, s0, {}   # &{}", off, name));
                }
                Ty::Char => self.emit(format!("    lbu  a0, {}(s0)   # {}", off, name)),
                _ => self.emit(format!("    lw   a0, {}(s0)   # {}", off, name)),
            }
            return Ok(());
        }
        if let Some(g) = self.globals.get(name) {
            let ty = g.ty.clone();
            let addr = g.addr;
            match &ty {
                Ty::Array(_, _) | Ty::Struct(_) => {
                    self.emit(format!("    li   a0, {}   # &{}", addr, name));
                }
                Ty::Char => {
                    self.emit(format!("    li   t1, {}", addr));
                    self.emit(format!("    lbu  a0, 0(t1)   # {}", name));
                }
                _ => {
                    self.emit(format!("    li   t1, {}", addr));
                    self.emit(format!("    lw   a0, 0(t1)   # {}", name));
                }
            }
            return Ok(());
        }
        if self.funcs.contains_key(name) {
            return self.fail(line, format!("`{}` 是一个函数，要调用它得写 `{}()`", name, name));
        }
        self.fail(line, format!("没有定义过叫 `{}` 的变量", name))
    }

    fn eval_unary(&mut self, op: UnOp, expr: &Expr) -> Result<(), CodegenError> {
        match op {
            UnOp::AddrOf => {
                self.eval_lvalue(expr)?;
            }
            UnOp::Deref => {
                self.eval(expr)?;
                let ty = self.ty_of(expr)?.deref().ok_or_else(|| {
                    CodegenError::new(0, "对不是指针的值用了 `*`")
                })?;
                if matches!(ty, Ty::Array(_, _) | Ty::Struct(_)) {
                    // 取地址即可
                } else {
                    self.load_from_a0(&ty);
                }
            }
            UnOp::Neg => {
                self.eval(expr)?;
                self.emit("    neg  a0, a0");
            }
            UnOp::BitNot => {
                self.eval(expr)?;
                self.emit("    not  a0, a0");
            }
            UnOp::Not => {
                self.eval(expr)?;
                self.emit("    seqz a0, a0");
            }
        }
        Ok(())
    }

    fn eval_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<(), CodegenError> {
        // 短路运算单独处理
        if op == BinOp::LogAnd || op == BinOp::LogOr {
            let l_short = self.label("sc");
            let l_end = self.label("scend");
            self.eval(lhs)?;
            self.emit(format!(
                "    {}  a0, {}",
                if op == BinOp::LogAnd { "beqz" } else { "bnez" },
                l_short
            ));
            self.eval(rhs)?;
            self.emit("    snez a0, a0");
            self.emit(format!("    j    {}", l_end));
            self.emit(format!("{}:", l_short));
            self.emit(format!("    li   a0, {}", if op == BinOp::LogAnd { 0 } else { 1 }));
            self.emit(format!("{}:", l_end));
            return Ok(());
        }

        let lty = self.ty_of(lhs)?;
        let rty = self.ty_of(rhs)?;
        self.eval(lhs)?;
        self.push_a0();
        self.eval(rhs)?;
        self.pop_to("a1"); // a1 = 左, a0 = 右

        // 指针算术：`ptr + int` / `ptr - int` 要按元素大小缩放
        let scaled = match op {
            BinOp::Add | BinOp::Sub => {
                if let Ty::Ptr(inner) = &lty {
                    let size = inner.size_with(self.structs).max(1);
                    if size != 1 {
                        self.emit(format!("    li   t0, {}", size));
                        self.emit("    mul  a0, a0, t0   # 指针偏移按元素大小缩放");
                    }
                    true
                } else if let Ty::Ptr(inner) = &rty {
                    let size = inner.size_with(self.structs).max(1);
                    if size != 1 {
                        self.emit(format!("    li   t0, {}", size));
                        self.emit("    mul  a1, a1, t0   # 指针偏移按元素大小缩放");
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        let _ = scaled;

        match op {
            BinOp::Add => self.emit("    add  a0, a1, a0"),
            BinOp::Sub => self.emit("    sub  a0, a1, a0"),
            BinOp::Mul => self.emit("    mul  a0, a1, a0"),
            BinOp::Div => self.emit("    div  a0, a1, a0"),
            BinOp::Rem => self.emit("    rem  a0, a1, a0"),
            BinOp::BitAnd => self.emit("    and  a0, a1, a0"),
            BinOp::BitOr => self.emit("    or   a0, a1, a0"),
            BinOp::BitXor => self.emit("    xor  a0, a1, a0"),
            BinOp::Shl => {
                // 移位量按 5 位取（和硬件一致）；`<<` 是逻辑左移
                self.emit("    andi a0, a0, 31");
                self.emit("    sll  a0, a1, a0");
            }
            BinOp::Shr => {
                // 右移用**逻辑右移**：星咒的 `int` 是 32 位比特容器，
                // 用算术右移会让负数永远移不到 0（`while (x) x >>= 1;` 这种写法会死循环）。
                self.emit("    andi a0, a0, 31");
                self.emit("    srl  a0, a1, a0");
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge | BinOp::Eq | BinOp::Ne => {
                let (reg_a, reg_b, kind) = match op {
                    BinOp::Lt => ("a1", "a0", "slt"),
                    BinOp::Gt => ("a0", "a1", "slt"),
                    BinOp::Le => ("a0", "a1", "sle"),
                    BinOp::Ge => ("a1", "a0", "sle"),
                    BinOp::Eq => ("a1", "a0", "seq"),
                    BinOp::Ne => ("a1", "a0", "sne"),
                    _ => unreachable!(),
                };
                match kind {
                    "slt" => self.emit(format!("    slt  a0, {}, {}", reg_a, reg_b)),
                    "sle" => {
                        self.emit(format!("    slt  a0, {}, {}", reg_a, reg_b));
                        self.emit("    xori a0, a0, 1");
                    }
                    "seq" => {
                        self.emit("    xor  a0, a1, a0");
                        self.emit("    seqz a0, a0");
                    }
                    "sne" => {
                        self.emit("    xor  a0, a1, a0");
                        self.emit("    snez a0, a0");
                    }
                    _ => unreachable!(),
                }
            }
            BinOp::LogAnd | BinOp::LogOr => unreachable!(),
        }
        Ok(())
    }

    fn eval_assign(
        &mut self,
        op: Option<BinOp>,
        target: &Expr,
        value: &Expr,
    ) -> Result<(), CodegenError> {
        match op {
            None => {
                self.eval_lvalue(target)?;
                self.push_a0();
                self.eval(value)?;
                self.pop_to("a1"); // a1 = 地址, a0 = 值
            }
            Some(binop) => {
                // `x += e` → `x = x + e`
                self.eval_lvalue(target)?;
                self.push_a0(); // 地址
                let ty = self.ty_of(target)?;
                self.load_from_stack_addr(&ty); // a0 = 旧值
                self.push_a0();
                self.eval(value)?;
                self.pop_to("a1"); // a1 = 旧值, a0 = 右值
                match binop {
                    BinOp::Add => self.emit("    add  a0, a1, a0"),
                    BinOp::Sub => self.emit("    sub  a0, a1, a0"),
                    BinOp::Mul => self.emit("    mul  a0, a1, a0"),
                    BinOp::Div => self.emit("    div  a0, a1, a0"),
                    BinOp::Rem => self.emit("    rem  a0, a1, a0"),
                    BinOp::BitAnd => self.emit("    and  a0, a1, a0"),
                    BinOp::BitOr => self.emit("    or   a0, a1, a0"),
                    BinOp::BitXor => self.emit("    xor  a0, a1, a0"),
                    BinOp::Shl => self.emit("    sll  a0, a1, a0"),
                    BinOp::Shr => self.emit("    sra  a0, a1, a0"),
                    other => {
                        return self.fail(0, format!("不支持 `{}` 这种复合赋值", other.symbol()))
                    }
                }
                // 复合赋值：先算新值，再取回地址写回
                // （注意顺序：算新值的过程中 a1 会被当作右操作数，所以只能最后再装地址）
                self.pop_to("a1"); // a1 = 地址
            }
        }
        let ty = self.ty_of(target)?;
        self.store_to_a1(&ty);
        Ok(())
    }

    /// 从栈顶的地址读一个值（不弹栈）。
    ///
    /// 用 `a1` 暂存地址：调用方随后必须**先**算完新值、**再**取地址写回，
    /// 否则 a1 里的旧值会被踩掉。
    fn load_from_stack_addr(&mut self, ty: &Ty) {
        self.emit("    lw   a1, 0(sp)");
        match ty {
            Ty::Char => self.emit("    lbu  a0, 0(a1)"),
            _ => self.emit("    lw   a0, 0(a1)"),
        }
    }

    fn store_to_a1(&mut self, ty: &Ty) {
        match ty {
            Ty::Char => self.emit("    sb   a0, 0(a1)"),
            _ => self.emit("    sw   a0, 0(a1)"),
        }
    }

    fn load_from_a0(&mut self, ty: &Ty) {
        match ty {
            Ty::Char => self.emit("    lbu  a0, 0(a0)"),
            Ty::Array(_, _) | Ty::Struct(_) | Ty::Void => {}
            _ => self.emit("    lw   a0, 0(a0)"),
        }
    }

    fn cast_a0(&mut self, ty: &Ty) {
        // 星咒里 int / char / 指针都是 32 位寄存器，只有 char 赋值时截断
        if matches!(ty, Ty::Char) {
            self.emit("    andi a0, a0, 255");
        }
    }

    fn eval_incdec(
        &mut self,
        target: &Expr,
        delta: i32,
        postfix: bool,
    ) -> Result<(), CodegenError> {
        let ty = self.ty_of(target)?;
        let step = match &ty {
            Ty::Ptr(inner) => inner.size_with(self.structs).max(1) as i32,
            _ => 1,
        } * delta;
        let (load, store) = match ty {
            Ty::Char => ("lbu", "sb"),
            _ => ("lw", "sw"),
        };

        self.eval_lvalue(target)?;
        self.push_a0(); // 栈顶：地址
        self.emit("    lw   t0, 0(sp)");
        self.emit(format!("    {}   a0, 0(t0)   # 旧值", load));
        if postfix {
            // 先把旧值留在栈上（它要作为整个表达式的值），再写回新值
            self.emit("    addi t1, a0, 0");
            self.emit("    addi sp, sp, -4");
            self.emit("    sw   t1, 0(sp)   # 留住旧值作为返回值");
            self.emit(format!("    addi t1, t1, {}", step));
            self.emit("    lw   t0, 4(sp)   # 地址");
            self.emit(format!("    {}   t1, 0(t0)", store));
            self.emit("    lw   a0, 0(sp)   # 返回值 = 旧值");
            self.emit("    addi sp, sp, 8");
        } else {
            self.emit(format!("    addi t1, a0, {}", step));
            self.emit("    lw   t0, 0(sp)   # 地址");
            self.emit(format!("    {}   t1, 0(t0)", store));
            self.emit("    addi a0, a0, ".to_string() + &step.to_string());
            self.emit("    addi sp, sp, 4");
        }
        Ok(())
    }

    /// 求值一个"可以当左值"的表达式的**地址**，结果放 `a0`。
    fn eval_lvalue(&mut self, e: &Expr) -> Result<(), CodegenError> {
        match e {
            Expr::Var(name) => {
                if let Some(l) = self.lookup_local(name) {
                    let off = l.offset;
                    self.emit(format!("    addi a0, s0, {}   # &{}", off, name));
                    return Ok(());
                }
                if let Some(g) = self.globals.get(name) {
                    let addr = g.addr;
                    self.emit(format!("    li   a0, {}   # &{}", addr, name));
                    return Ok(());
                }
                self.fail(0, format!("没有定义过叫 `{}` 的变量", name))
            }
            Expr::Unary { op: UnOp::Deref, expr } => {
                self.eval(expr)?;
                Ok(())
            }
            Expr::Index { base, index } => {
                let bty = self.ty_of(base)?;
                let elem = bty.deref().ok_or_else(|| {
                    CodegenError::new(0, "对不是数组/指针的值用了 `[]`")
                })?;
                // 数组：base 本身就是地址；指针：先把指针值取出来
                match &bty {
                    Ty::Array(_, _) => self.eval_lvalue(base)?,
                    _ => self.eval(base)?,
                }
                self.push_a0();
                self.eval(index)?;
                let esize = elem.size_with(self.structs).max(1);
                if esize != 1 {
                    self.emit(format!("    li   t0, {}", esize));
                    self.emit("    mul  a0, a0, t0");
                }
                self.pop_to("a1");
                self.emit("    add  a0, a1, a0");
                Ok(())
            }
            Expr::Member { base, name } => {
                let offset = self.member_offset(base, name)?;
                self.eval_lvalue(base)?;
                if offset != 0 {
                    self.emit(format!("    addi a0, a0, {}   # .{}", offset, name));
                }
                Ok(())
            }
            Expr::Arrow { base, name } => {
                let offset = self.member_offset_arrow(base, name)?;
                self.eval(base)?;
                if offset != 0 {
                    self.emit(format!("    addi a0, a0, {}   # ->{}", offset, name));
                }
                Ok(())
            }
            other => self.fail(0, format!("这个表达式不能作为赋值目标（{:?}）", variant_name(other))),
        }
    }

    fn member_offset(&mut self, base: &Expr, name: &str) -> Result<u32, CodegenError> {
        let Ty::Struct(sname) = self.ty_of(base)? else {
            return self.fail(0, "对不是结构体的值用了 `.`");
        };
        let def = self
            .structs
            .get(&sname)
            .ok_or_else(|| CodegenError::new(0, format!("不认识结构体 `{}`", sname)))?;
        def.field(name)
            .map(|(_, _, off)| *off)
            .ok_or_else(|| CodegenError::new(0, format!("结构体 `{}` 里没有成员 `{}`", sname, name)))
    }

    fn member_offset_arrow(&mut self, base: &Expr, name: &str) -> Result<u32, CodegenError> {
        let inner = self
            .ty_of(base)?
            .deref()
            .ok_or_else(|| CodegenError::new(0, "`->` 左边必须是指向结构体的指针"))?;
        let Ty::Struct(sname) = inner else {
            return self.fail(0, "`->` 左边必须是指向结构体的指针");
        };
        let def = self
            .structs
            .get(&sname)
            .ok_or_else(|| CodegenError::new(0, format!("不认识结构体 `{}`", sname)))?;
        def.field(name)
            .map(|(_, _, off)| *off)
            .ok_or_else(|| CodegenError::new(0, format!("结构体 `{}` 里没有成员 `{}`", sname, name)))
    }

    fn eval_call(&mut self, name: &str, args: &[Expr], line: u32) -> Result<(), CodegenError> {
        // 内建伪调用
        if name == "@sizeof_expr" {
            let ty = self.ty_of(&args[0])?;
            let size = ty.size_with(self.structs);
            self.emit(format!("    li   a0, {}", size));
            return Ok(());
        }
        if name == "@ternary" {
            let l_else = self.label("tern_else");
            let l_end = self.label("tern_end");
            self.eval(&args[0])?;
            self.emit(format!("    beqz a0, {}", l_else));
            self.eval(&args[1])?;
            self.emit(format!("    j    {}", l_end));
            self.emit(format!("{}:", l_else));
            self.eval(&args[2])?;
            self.emit(format!("{}:", l_end));
            return Ok(());
        }

        let Some(sig) = self.funcs.get(name) else {
            return self.fail(line, format!("没有定义过函数 `{}`", name));
        };
        let params = sig.params.clone();
        if params.len() != args.len() {
            return self.fail(
                line,
                format!(
                    "函数 `{}` 需要 {} 个参数，这里给了 {} 个",
                    name,
                    params.len(),
                    args.len()
                ),
            );
        }
        if args.len() > MAX_ARGS {
            return self.fail(line, format!("函数参数最多 {} 个", MAX_ARGS));
        }

        // 逆序求值并压栈 → 保证 a0 里是第 0 个参数
        for arg in args.iter().rev() {
            self.eval(arg)?;
            self.push_a0();
        }
        // 参数寄存器顺序：a0..a5（注意是 a0-a5，不是 a0-a7）
        let arg_regs = ["a0", "a1", "a2", "a3", "a4", "a5"];
        for (i, reg) in arg_regs.iter().enumerate().take(args.len()) {
            self.emit(format!("    lw   {}, {} * 4(sp)", reg, i));
        }
        self.emit(format!("    jal  ra, {}", name));
        if !args.is_empty() {
            self.emit(format!("    addi sp, sp, {}", args.len() * 4));
        }
        Ok(())
    }
}

fn variant_name(e: &Expr) -> &'static str {
    match e {
        Expr::Int(_) => "字面量",
        Expr::Str(_) => "字符串",
        Expr::Var(_) => "变量",
        Expr::Unary { .. } => "一元运算",
        Expr::Binary { .. } => "二元运算",
        Expr::Assign { .. } => "赋值",
        Expr::Call { .. } => "函数调用",
        Expr::Index { .. } => "下标",
        Expr::Member { .. } => "成员",
        Expr::Arrow { .. } => "指针成员",
        Expr::Cast { .. } => "强制转换",
        Expr::IncDec { .. } => "自增自减",
        Expr::Comma { .. } => "逗号表达式",
        Expr::SizeOf(_) => "sizeof",
    }
}
