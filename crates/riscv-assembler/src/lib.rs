//! 星之语汇编器
//!
//! 把汇编文本编译为 RV32IM 机器码。纯函数、无副作用，可独立测试。
//!
//! ## 两遍扫描
//!
//! - **第一遍**：按"条目长度表"推进地址计数器，收集标签地址与 `.equ` 常量，并记下**每个条目落在哪个地址**。
//!   每条指令的长度只取决于助记符与操作数**形式**（`li`/`la` 固定展开成 2 条），与标签的具体值无关，
//!   所以两遍的地址完全一致，不需要反复迭代。
//! - **第二遍**：真正编码，此时标签已全部就位。
//!
//! ## 支持范围
//!
//! - 指令：RV32IM 全量（见 [`riscv_simulator`]）
//! - 伪指令：`li la mv not neg nop j jr ret call beqz bnez blez bgez bltz bgtz bgt ble bgtu bleu seqz snez`
//! - 指示符：`.text .data .org .word .half .byte .ascii .asciz/.string .space .align .equ/.set .globl`
//! - 表达式：十进制 / `0x` 十六进制 / 字符字面量 / 标签 / `.equ` 常量，以及
//!   `+ - * / % & | ^ << >> ~` 与一元 `-`、括号
//! - 注释：`#` `;` `//` 到行尾
//!
//! 错误信息带**行号**与中文说明，可直接展示给玩家。

mod ast;
mod encode;
mod token;

#[cfg(test)]
mod tests;

pub use ast::Expr as AstExpr;
pub use ast::parse as parse_program;
pub use encode::{is_pseudo, pseudo_requires, OpKind, OP_TABLE};

use ast::{Directive, Item};
use riscv_simulator::{Simulator, SimulatorConfig};
use std::collections::HashMap;

/// 汇编产物。
#[derive(Debug, Clone, Default)]
pub struct Assembly {
    /// 代码段机器码（从 `base_addr` 起，`.org` 造成的空洞为 0）
    pub words: Vec<u32>,
    /// 代码段起始地址
    pub base_addr: u32,
    /// 数据段字节：(地址, 值)，稀疏
    pub data: Vec<(u32, u8)>,
    /// 数据段起始地址（程序用过 `.data` 时才有值）
    pub data_base: Option<u32>,
    /// 指令地址 → 源行号（1-based）
    pub source_map: Vec<(u32, u32)>,
    /// 标签地址表
    pub symbols: HashMap<String, u32>,
}

impl Assembly {
    /// 单条指令占的内存宽度（字节）。
    pub const WORD: u32 = 4;

    /// 指令地址 → 源行号。
    pub fn addr_to_line(&self, addr: u32) -> Option<u32> {
        self.source_map.iter().find(|(a, _)| *a == addr).map(|(_, l)| *l)
    }

    /// 源行号 → 该行首条指令的地址。
    pub fn line_to_addr(&self, line: u32) -> Option<u32> {
        self.source_map.iter().find(|(_, l)| *l == line).map(|(a, _)| *a)
    }

    /// 把代码段与数据段一起装进模拟器。
    ///
    /// 注意：`configure()` 会清空并重建机器状态，所以**先 configure，再 apply**。
    pub fn apply_to(&self, sim: &mut Simulator) -> Result<(), String> {
        for &(addr, byte) in &self.data {
            sim.set_mem(addr, &[byte]);
        }
        sim.load_binary(&self.words).map_err(|e| e.to_string())
    }

    /// 供 `SimulatorConfig.initial_memory` 使用的稀疏内存镜像。
    pub fn initial_memory(&self) -> std::collections::BTreeMap<u32, u8> {
        self.data.iter().map(|&(a, b)| (a, b)).collect()
    }

    /// 在给定配置上补齐"执行这段汇编"所需的信息（PC 与数据段初值）。
    pub fn base_config(&self, mut cfg: SimulatorConfig) -> SimulatorConfig {
        cfg.initial_pc = self.base_addr;
        for (a, b) in self.initial_memory() {
            cfg.initial_memory.insert(a, b);
        }
        cfg
    }
}

/// 汇编错误（带行号）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmError {
    /// 1-based 行号；0 表示与具体行无关
    pub line: u32,
    pub message: String,
}

impl AsmError {
    pub fn new(line: u32, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "第 {} 行：{}", self.line, self.message)
        }
    }
}

/// 汇编选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 代码段起始地址
    pub base_addr: u32,
    /// 数据段默认起始地址
    pub data_base: u32,
    /// 允许的指令助记符（空 = 全部允许）。按**助记符**检查。
    pub available_ops: Vec<String>,
    /// 禁止访问的寄存器（`x` 名或 ABI 名；空 = 全部允许）
    pub forbidden_regs: Vec<String>,
    /// 是否允许白名单之外的伪指令（只要它展开后用到的基础指令都在白名单里就放行）
    pub allow_pseudo: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_addr: 0,
            data_base: 0x1000,
            available_ops: Vec::new(),
            forbidden_regs: Vec::new(),
            allow_pseudo: true,
        }
    }
}

impl Options {
    /// 从模拟器配置推导汇编选项（保证两侧的安全字段一致）。
    pub fn from_sim_config(cfg: &SimulatorConfig) -> Self {
        Self {
            base_addr: cfg.initial_pc,
            available_ops: cfg.allowed_ops.clone(),
            forbidden_regs: cfg.forbidden_regs.clone(),
            ..Self::default()
        }
    }

    /// 该助记符是否被允许（伪指令按其展开后的基础指令判断）。
    pub fn op_allowed(&self, mnemonic: &str) -> bool {
        if self.available_ops.is_empty() {
            return true;
        }
        if self.available_ops.iter().any(|m| m == mnemonic) {
            return true;
        }
        if self.allow_pseudo && is_pseudo(mnemonic) {
            return pseudo_requires(mnemonic)
                .map(|reqs| reqs.iter().all(|r| self.available_ops.iter().any(|m| m == r)))
                .unwrap_or(false);
        }
        false
    }
}

/// 把汇编文本汇编为机器码。
pub fn assemble(source: &str) -> Result<Assembly, Vec<AsmError>> {
    assemble_with(source, &Options::default())
}

/// 带选项汇编。
pub fn assemble_with(source: &str, options: &Options) -> Result<Assembly, Vec<AsmError>> {
    let program = ast::parse(source)?;
    let mut errors: Vec<AsmError> = Vec::new();

    // ── `.equ` 常量表（按出现顺序求值，允许引用之前定义的常量） ──
    let mut consts: HashMap<String, i64> = HashMap::new();
    for item in &program {
        if let Item::Directive { directive: Directive::Equ { name, expr }, line } = item {
            match ast::eval(expr, &HashMap::new(), &consts) {
                Ok(v) => {
                    consts.insert(name.clone(), v);
                }
                Err(m) => errors.push(AsmError::new(*line, m)),
            }
        }
    }

    // ── 第一遍：定地址、收标签、记下每条目的落点 ──
    let layout = Layout::compute(&program, &consts, options, &mut errors);

    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        errors.dedup();
        return Err(errors);
    }

    // ── 第二遍：编码 ──
    let mut items: Vec<Emitted> = Vec::new();
    for (idx, item) in program.iter().enumerate() {
        let line = item.line();
        let slot = &layout.slots[idx];
        match item {
            Item::Label { .. } => {}
            Item::Instruction { mnemonic, operands, .. } => {
                if slot.size == 0 {
                    continue; // 白名单已过滤掉
                }
                match encode::encode(
                    mnemonic,
                    operands,
                    slot.addr,
                    &layout.labels,
                    &consts,
                    options,
                ) {
                    Ok(words) => {
                        for (i, w) in words.iter().enumerate() {
                            items.push(Emitted::Word {
                                addr: slot.addr + i as u32 * 4,
                                value: *w,
                                line,
                            });
                        }
                    }
                    Err(e) => errors.push(AsmError::new(line, e.message())),
                }
            }
            Item::Directive { directive, .. } => match directive {
                Directive::Globl { .. }
                | Directive::Equ { .. }
                | Directive::Text
                | Directive::Data
                | Directive::Org { .. }
                | Directive::Align { .. } => {}
                other => match data_bytes(other, &layout.labels, &consts) {
                    Ok(bytes) => {
                        for (i, b) in bytes.iter().enumerate() {
                            items.push(Emitted::Byte { addr: slot.addr + i as u32, value: *b });
                        }
                    }
                    Err(m) => errors.push(AsmError::new(line, m)),
                },
            },
        }
    }

    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        errors.dedup();
        return Err(errors);
    }

    // ── 汇总 ──
    let mut words: Vec<u32> = Vec::new();
    let mut source_map: Vec<(u32, u32)> = Vec::new();
    let mut data: Vec<(u32, u8)> = Vec::new();
    let mut cursor = options.base_addr;
    for it in &items {
        match it {
            Emitted::Word { addr, value, line } => {
                while cursor < *addr {
                    words.push(0);
                    cursor += 4;
                }
                words.push(*value);
                source_map.push((*addr, *line));
                cursor = addr + 4;
            }
            Emitted::Byte { addr, value } => data.push((*addr, *value)),
        }
    }

    if words.is_empty() {
        return Err(vec![AsmError::new(0, "这段代码里没有任何指令".to_string())]);
    }
    data.sort_by_key(|(a, _)| *a);
    let has_data = !data.is_empty();

    Ok(Assembly {
        words,
        base_addr: options.base_addr,
        data,
        data_base: if has_data { Some(layout.data_start) } else { None },
        source_map,
        symbols: layout.labels,
    })
}

/// 第一遍的布局结果。
struct Layout {
    /// 每个条目落在哪个地址、占多少字节
    slots: Vec<Slot>,
    labels: HashMap<String, u32>,
    data_start: u32,
}

struct Slot {
    addr: u32,
    size: u32,
}

impl Layout {
    fn compute(
        program: &[Item],
        consts: &HashMap<String, i64>,
        options: &Options,
        errors: &mut Vec<AsmError>,
    ) -> Self {
        let mut text = options.base_addr;
        let mut data = options.data_base;
        let mut in_data = false;
        let mut slots = Vec::with_capacity(program.len());
        let mut labels: HashMap<String, u32> = HashMap::new();

        for item in program {
            let line = item.line();
            match item {
                Item::Label { name, .. } => {
                    let addr = if in_data { data } else { text };
                    if labels.contains_key(name) {
                        errors.push(AsmError::new(line, format!("标签 `{}` 重复定义了", name)));
                    } else {
                        labels.insert(name.clone(), addr);
                    }
                    slots.push(Slot { addr, size: 0 });
                }
                Item::Instruction { mnemonic, operands, .. } => {
                    let addr = if in_data { data } else { text };
                    if in_data {
                        errors.push(AsmError::new(
                            line,
                            "数据段里不能写指令；请先写 `.text` 切回代码段",
                        ));
                        slots.push(Slot { addr, size: 0 });
                        continue;
                    }
                    if !options.op_allowed(mnemonic) {
                        errors.push(AsmError::new(
                            line,
                            format!("本关不允许使用指令 `{}`", mnemonic),
                        ));
                        slots.push(Slot { addr, size: 0 });
                        continue;
                    }
                    let size = encode::size_of(mnemonic, operands, line, errors);
                    slots.push(Slot { addr, size });
                    text += size * 4;
                }
                Item::Directive { directive, line } => {
                    let mut size = 0;
                    match directive {
                        Directive::Text => in_data = false,
                        Directive::Data => {
                            in_data = true;
                            if data < 0x1000 {
                                data = options.data_base;
                            }
                        }
                        Directive::Org { expr } => {
                            match ast::eval(expr, &labels, consts) {
                                Ok(v) if v >= 0 => {
                                    if in_data {
                                        data = v as u32;
                                    } else {
                                        text = v as u32;
                                    }
                                }
                                Ok(_) => errors.push(AsmError::new(*line, "地址不能是负数")),
                                Err(m) => errors.push(AsmError::new(*line, m)),
                            }
                        }
                        Directive::Align { n } => {
                            let mask = (*n).max(1) - 1;
                            if in_data {
                                data = (data + mask) & !mask;
                            } else {
                                text = (text + mask) & !mask;
                            }
                        }
                        Directive::Globl { .. } | Directive::Equ { .. } => {}
                        other => {
                            match data_bytes(other, &labels, consts) {
                                Ok(bytes) => size = bytes.len() as u32,
                                Err(m) => errors.push(AsmError::new(*line, m)),
                            }
                        }
                    }
                    let addr = if in_data { data } else { text };
                    slots.push(Slot { addr, size });
                    if in_data {
                        data += size;
                    } else {
                        // 允许在代码段就地放数据：占地址，不产生指令
                        text += size;
                    }
                }
            }
        }

        Self { slots, labels, data_start: options.data_base }
    }
}

/// 求值数据指示符的字节内容。
fn data_bytes(
    d: &Directive,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    match d {
        Directive::Word { values } => {
            for e in values {
                let v = ast::eval(e, labels, consts)?;
                out.extend_from_slice(&(v as i32).to_le_bytes());
            }
        }
        Directive::Half { values } => {
            for e in values {
                let v = ast::eval(e, labels, consts)?;
                out.extend_from_slice(&(v as i16).to_le_bytes());
            }
        }
        Directive::Byte { values } => {
            for e in values {
                let v = ast::eval(e, labels, consts)?;
                out.push(v as u8);
            }
        }
        Directive::Ascii { text } => out.extend_from_slice(text.as_bytes()),
        Directive::Asciz { text } => {
            out.extend_from_slice(text.as_bytes());
            out.push(0);
        }
        Directive::Space { n } => out.extend(std::iter::repeat(0u8).take(*n as usize)),
        _ => return Err("这个指示符不能用来放数据".to_string()),
    }
    Ok(out)
}

enum Emitted {
    Word { addr: u32, value: u32, line: u32 },
    Byte { addr: u32, value: u8 },
}
