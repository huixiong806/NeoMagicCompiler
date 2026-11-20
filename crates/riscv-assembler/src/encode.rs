//! 指令编码：助记符 + 操作数 → 32 位机器码。
//!
//! 编码表用 `(助记符, 编码种类)` 的静态数组描述，查表用线性扫描
//! （指令种类不到 60 个，线性扫描比哈希更快也更好读）。
//!
//! 分支与 `jal` 需要**当前指令地址**才能算偏移，所以编码入口统一带上 `addr`。

use crate::ast::{eval, Expr};
use crate::{AsmError, Options};
use riscv_simulator::{reg_abi_name, reg_index, reg_name};
use std::collections::HashMap;

/// 指令的编码种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// `rd, rs1, rs2`（add/sub/.../mul...）
    R { funct3: u32, funct7: u32 },
    /// `rd, rs1, shamt`（slli/srli/srai）
    Shift { funct3: u32, funct7: u32 },
    /// `rd, rs1, imm12`
    IArith { funct3: u32 },
    /// `rd, imm12(rs1)`
    Load { funct3: u32 },
    /// `rs2, imm12(rs1)`
    Store { funct3: u32 },
    /// `rs1, rs2, label`
    Branch { funct3: u32 },
    /// `rd, label`
    Jal,
    /// `rd, imm12(rs1)`
    Jalr,
    /// `rd, imm20`
    Upper,
    /// 无操作数
    System { funct12: u32 },
    /// `fence`
    Fence,
}

/// 编码表。
pub const OP_TABLE: &[(&str, OpKind)] = &[
    // ── R 型运算 ──
    ("add", OpKind::R { funct3: 0x0, funct7: 0x00 }),
    ("sub", OpKind::R { funct3: 0x0, funct7: 0x20 }),
    ("sll", OpKind::R { funct3: 0x1, funct7: 0x00 }),
    ("slt", OpKind::R { funct3: 0x2, funct7: 0x00 }),
    ("sltu", OpKind::R { funct3: 0x3, funct7: 0x00 }),
    ("xor", OpKind::R { funct3: 0x4, funct7: 0x00 }),
    ("srl", OpKind::R { funct3: 0x5, funct7: 0x00 }),
    ("sra", OpKind::R { funct3: 0x5, funct7: 0x20 }),
    ("or", OpKind::R { funct3: 0x6, funct7: 0x00 }),
    ("and", OpKind::R { funct3: 0x7, funct7: 0x00 }),
    // ── M 扩展 ──
    ("mul", OpKind::R { funct3: 0x0, funct7: 0x01 }),
    ("div", OpKind::R { funct3: 0x4, funct7: 0x01 }),
    ("divu", OpKind::R { funct3: 0x5, funct7: 0x01 }),
    ("rem", OpKind::R { funct3: 0x6, funct7: 0x01 }),
    ("remu", OpKind::R { funct3: 0x7, funct7: 0x01 }),
    // ── 立即数运算 ──
    ("addi", OpKind::IArith { funct3: 0x0 }),
    ("slti", OpKind::IArith { funct3: 0x2 }),
    ("sltiu", OpKind::IArith { funct3: 0x3 }),
    ("xori", OpKind::IArith { funct3: 0x4 }),
    ("ori", OpKind::IArith { funct3: 0x6 }),
    ("andi", OpKind::IArith { funct3: 0x7 }),
    ("slli", OpKind::Shift { funct3: 0x1, funct7: 0x00 }),
    ("srli", OpKind::Shift { funct3: 0x5, funct7: 0x00 }),
    ("srai", OpKind::Shift { funct3: 0x5, funct7: 0x20 }),
    // ── 加载 ──
    ("lb", OpKind::Load { funct3: 0x0 }),
    ("lh", OpKind::Load { funct3: 0x1 }),
    ("lw", OpKind::Load { funct3: 0x2 }),
    ("lbu", OpKind::Load { funct3: 0x4 }),
    ("lhu", OpKind::Load { funct3: 0x5 }),
    // ── 存储 ──
    ("sb", OpKind::Store { funct3: 0x0 }),
    ("sh", OpKind::Store { funct3: 0x1 }),
    ("sw", OpKind::Store { funct3: 0x2 }),
    // ── 分支 ──
    ("beq", OpKind::Branch { funct3: 0x0 }),
    ("bne", OpKind::Branch { funct3: 0x1 }),
    ("blt", OpKind::Branch { funct3: 0x4 }),
    ("bge", OpKind::Branch { funct3: 0x5 }),
    ("bltu", OpKind::Branch { funct3: 0x6 }),
    ("bgeu", OpKind::Branch { funct3: 0x7 }),
    // ── 跳转与高位立即数 ──
    ("jal", OpKind::Jal),
    ("jalr", OpKind::Jalr),
    ("lui", OpKind::Upper),
    ("auipc", OpKind::Upper),
    // ── 其他 ──
    ("fence", OpKind::Fence),
    ("ebreak", OpKind::System { funct12: 0x001 }),
    ("ecall", OpKind::System { funct12: 0x000 }),
];

/// 查表。
pub fn lookup(mnemonic: &str) -> Option<OpKind> {
    OP_TABLE.iter().find(|(m, _)| *m == mnemonic).map(|(_, k)| *k)
}

// ─────────────────────────────────────────────────────────────────────────────
//  伪指令
// ─────────────────────────────────────────────────────────────────────────────

/// 一条伪指令展开后需要哪些基础指令（用于白名单判定）。
pub fn pseudo_requires(mnemonic: &str) -> Option<&'static [&'static str]> {
    Some(match mnemonic {
        "li" => &["lui", "addi"],
        "la" => &["lui", "addi"],
        "mv" => &["addi"],
        "not" => &["xori"],
        "neg" => &["sub"],
        "nop" => &["addi"],
        "j" => &["jal"],
        "jr" => &["jalr"],
        "ret" => &["jalr"],
        "call" => &["jal"],
        "seqz" => &["sltiu"],
        "snez" => &["sltu"],
        "beqz" => &["beq"],
        "bnez" => &["bne"],
        "blez" => &["bge"],
        "bgez" => &["bge"],
        "bltz" => &["blt"],
        "bgtz" => &["blt"],
        "bgt" => &["blt"],
        "ble" => &["bge"],
        "bgtu" => &["bltu"],
        "bleu" => &["bgeu"],
        "slp" => &["slp"],
        _ => return None,
    })
}

/// 是否为伪指令。
pub fn is_pseudo(mnemonic: &str) -> bool {
    pseudo_requires(mnemonic).is_some()
}

/// 一条指令展开后占几条机器指令。
///
/// 只依赖助记符与操作数的**形式**，与标签的具体值无关 —— 这是"两遍扫描地址不漂移"的前提。
pub fn size_of(mnemonic: &str, operands: &[String], line: u32, errors: &mut Vec<AsmError>) -> u32 {
    if mnemonic == "li" || mnemonic == "la" {
        if operands.len() != 2 {
            errors.push(AsmError::new(
                line,
                format!("`{}` 要写成 `{} 寄存器, 数值`", mnemonic, mnemonic),
            ));
        }
        return 2;
    }
    if is_pseudo(mnemonic) || lookup(mnemonic).is_some() {
        return 1;
    }
    errors.push(AsmError::new(line, format!("不认识的指令 `{}`", mnemonic)));
    1
}

// ─────────────────────────────────────────────────────────────────────────────
//  编码入口
// ─────────────────────────────────────────────────────────────────────────────

/// 编码错误（只带消息，行号由上层补）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingError(pub String);

impl EncodingError {
    pub fn message(&self) -> String {
        self.0.clone()
    }
}

impl std::fmt::Display for EncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

type EResult<T> = Result<T, EncodingError>;

fn err<T>(msg: impl Into<String>) -> EResult<T> {
    Err(EncodingError(msg.into()))
}

/// 把一条指令编码成机器码（伪指令会展开成多条）。
///
/// `addr` 是这条指令的地址（分支/跳转算偏移用）。
pub fn encode(
    mnemonic: &str,
    operands: &[String],
    addr: u32,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
    options: &Options,
) -> EResult<Vec<u32>> {
    if is_pseudo(mnemonic) {
        return encode_pseudo(mnemonic, operands, addr, labels, consts, options);
    }
    let Some(kind) = lookup(mnemonic) else {
        return err(format!("不认识的指令 `{}`", mnemonic));
    };
    Ok(vec![encode_kind(mnemonic, kind, operands, addr, labels, consts, options)?])
}

fn encode_pseudo(
    mnemonic: &str,
    operands: &[String],
    addr: u32,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
    options: &Options,
) -> EResult<Vec<u32>> {
    match mnemonic {
        "li" => {
            let rd = expect_reg(operands, 0, "li", "目标寄存器")?;
            check_usage(&[rd], options)?;
            let value = eval_operand(operands, 1, labels, consts)?;
            Ok(expanded_li(rd, value))
        }
        "la" => {
            let rd = expect_reg(operands, 0, "la", "目标寄存器")?;
            check_usage(&[rd], options)?;
            let Some(name) = operands.get(1) else {
                return err("`la` 要写成 `la 寄存器, 标签`");
            };
            let value = eval_symbol(name, labels, consts).map_err(EncodingError)?;
            Ok(expanded_li(rd, value))
        }
        "mv" => {
            let rd = expect_reg(operands, 0, "mv", "目标寄存器")?;
            let rs = expect_reg(operands, 1, "mv", "源寄存器")?;
            check_usage(&[rd, rs], options)?;
            Ok(vec![i_type(0, rs, 0x0, rd, 0x13)])
        }
        "not" => {
            let rd = expect_reg(operands, 0, "not", "目标寄存器")?;
            let rs = expect_reg(operands, 1, "not", "源寄存器")?;
            check_usage(&[rd, rs], options)?;
            Ok(vec![i_type(-1, rs, 0x4, rd, 0x13)])
        }
        "neg" => {
            let rd = expect_reg(operands, 0, "neg", "目标寄存器")?;
            let rs = expect_reg(operands, 1, "neg", "源寄存器")?;
            check_usage(&[rd, rs], options)?;
            Ok(vec![r_type(0x20, rs, 0, 0x0, rd, 0x33)])
        }
        "nop" => Ok(vec![i_type(0, 0, 0x0, 0, 0x13)]),
        "j" => {
            let target = eval_operand(operands, 0, labels, consts)?;
            let off = target - addr as i64;
            check_jal_offset("j", off)?;
            Ok(vec![j_type(off as i32, 0, 0x6F)])
        }
        "jr" => {
            let rs = expect_reg(operands, 0, "jr", "寄存器")?;
            check_usage(&[rs], options)?;
            Ok(vec![i_type(0, rs, 0x0, 0, 0x67)])
        }
        "ret" => Ok(vec![i_type(0, 1, 0x0, 0, 0x67)]),
        "call" => {
            let target = eval_operand(operands, 0, labels, consts)?;
            let off = target - addr as i64;
            check_jal_offset("call", off)?;
            Ok(vec![j_type(off as i32, 1, 0x6F)])
        }
        "seqz" => {
            let rd = expect_reg(operands, 0, "seqz", "目标寄存器")?;
            let rs = expect_reg(operands, 1, "seqz", "源寄存器")?;
            check_usage(&[rd, rs], options)?;
            Ok(vec![i_type(1, rs, 0x3, rd, 0x13)])
        }
        "snez" => {
            let rd = expect_reg(operands, 0, "snez", "目标寄存器")?;
            let rs = expect_reg(operands, 1, "snez", "源寄存器")?;
            check_usage(&[rd, rs], options)?;
            Ok(vec![r_type(0, rs, 0, 0x3, rd, 0x33)])
        }
        // 分支别名
        "beqz" | "bnez" | "blez" | "bgez" | "bltz" | "bgtz" => {
            let rs = expect_reg(operands, 0, mnemonic, "要比较的寄存器")?;
            let target = eval_operand(operands, 1, labels, consts)?;
            let (a, b, funct3) = match mnemonic {
                "beqz" => (rs, 0, 0x0),
                "bnez" => (rs, 0, 0x1),
                "blez" => (0, rs, 0x5), // bge x0, rs
                "bgez" => (rs, 0, 0x5), // bge rs, x0
                "bltz" => (rs, 0, 0x4), // blt rs, x0
                "bgtz" => (0, rs, 0x4), // blt x0, rs
                _ => unreachable!(),
            };
            check_usage(&[a, b], options)?;
            let off = target - addr as i64;
            check_branch_offset(mnemonic, off)?;
            Ok(vec![b_type(off as i32, b, a, funct3, 0x63)])
        }
        "bgt" | "ble" | "bgtu" | "bleu" => {
            let rs = expect_reg(operands, 0, mnemonic, "第一个寄存器")?;
            let rt = expect_reg(operands, 1, mnemonic, "第二个寄存器")?;
            let target = eval_operand(operands, 2, labels, consts)?;
            // 都是"交换操作数 + 取反"得到的基础分支。
            //
            // ⚠ 这里**很容易写反**：`b_type` 的参数是 `(imm, rs2, rs1, ...)` —— 先 rs2 再 rs1。
            // `ble rs, rt`（rs <= rt）等价于"取反的分支" `bge(rs, rt)` 不成立时跳过去，
            // 而 `bge` 的编码是 funct3=0x5、rs1=rs、rs2=rt，所以元组要写 `(rt, rs, 0x5)`。
            // 早先写成 `(rs, rt, 0x5)`，结果 `ble` 被编成了 `bge rs, rt`（判断恒假），
            // 一个用它做数字范围判断的编译器关卡因此静默产出错误代码。
            let (a, b, funct3) = match mnemonic {
                "bgt" => (rt, rs, 0x4),  // blt rt, rs
                "ble" => (rt, rs, 0x5),  // bge rs, rt
                "bgtu" => (rt, rs, 0x6), // bltu rt, rs
                "bleu" => (rt, rs, 0x7), // bgeu rs, rt
                _ => unreachable!(),
            };
            check_usage(&[a, b], options)?;
            let off = target - addr as i64;
            check_branch_offset(mnemonic, off)?;
            Ok(vec![b_type(off as i32, b, a, funct3, 0x63)])
        }
        "slp" => {
            let cycles = eval_operand(operands, 0, labels, consts)?;
            if !(0..=(0xFFF)).contains(&cycles) {
                return err("`slp` 的等待周期要在 0..4095 之间");
            }
            Ok(vec![((cycles as u32) << 20) | 0x73])
        }
        other => err(format!("不认识的伪指令 `{}`", other)),
    }
}

/// `li` / `la` 的展开：`lui` + `addi`（固定两条，长度可预测）。
fn expanded_li(rd: u8, value: i64) -> Vec<u32> {
    let v = value as i32 as i64;
    let lo = ((v << 52) >> 52) as i32; // 低 12 位按有符号解释
    let hi = ((v - lo as i64) >> 12) as i32;
    let hi20 = (hi as u32) & 0x000F_FFFF;
    vec![
        (hi20 << 12) | ((rd as u32) << 7) | 0x37, // lui rd, hi20
        i_type(lo, rd, 0x0, rd, 0x13),            // addi rd, rd, lo
    ]
}

fn encode_kind(
    mnemonic: &str,
    kind: OpKind,
    operands: &[String],
    addr: u32,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
    options: &Options,
) -> EResult<u32> {
    match kind {
        OpKind::R { funct3, funct7 } => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            let rs1 = expect_reg(operands, 1, mnemonic, "第一个源寄存器")?;
            let rs2 = expect_reg(operands, 2, mnemonic, "第二个源寄存器")?;
            check_usage(&[rd, rs1, rs2], options)?;
            Ok(r_type(funct7, rs2, rs1, funct3, rd, 0x33))
        }
        OpKind::Shift { funct3, funct7 } => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            let rs1 = expect_reg(operands, 1, mnemonic, "源寄存器")?;
            check_usage(&[rd, rs1], options)?;
            let amount = eval_operand(operands, 2, labels, consts)?;
            if !(0..=31).contains(&amount) {
                return err(format!("`{}` 的移位量必须在 0..31 之间（写的是 {}）", mnemonic, amount));
            }
            Ok(i_type(((funct7 << 5) | amount as u32) as i32, rs1, funct3, rd, 0x13))
        }
        OpKind::IArith { funct3 } => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            let rs1 = expect_reg(operands, 1, mnemonic, "源寄存器")?;
            check_usage(&[rd, rs1], options)?;
            let imm = eval_operand(operands, 2, labels, consts)?;
            let imm = check_imm12(mnemonic, imm)?;
            Ok(i_type(imm, rs1, funct3, rd, 0x13))
        }
        OpKind::Load { funct3 } => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            let (imm, rs1) = parse_mem_operand(operands, 1, mnemonic, labels, consts)?;
            check_usage(&[rd, rs1], options)?;
            Ok(i_type(imm, rs1, funct3, rd, 0x03))
        }
        OpKind::Store { funct3 } => {
            let rs2 = expect_reg(operands, 0, mnemonic, "要写入的寄存器")?;
            let (imm, rs1) = parse_mem_operand(operands, 1, mnemonic, labels, consts)?;
            check_usage(&[rs1, rs2], options)?;
            Ok(s_type(imm, rs2, rs1, funct3, 0x23))
        }
        OpKind::Branch { funct3 } => {
            let rs1 = expect_reg(operands, 0, mnemonic, "第一个寄存器")?;
            let rs2 = expect_reg(operands, 1, mnemonic, "第二个寄存器")?;
            check_usage(&[rs1, rs2], options)?;
            let target = eval_operand(operands, 2, labels, consts)?;
            let off = target - addr as i64;
            check_branch_offset(mnemonic, off)?;
            Ok(b_type(off as i32, rs2, rs1, funct3, 0x63))
        }
        OpKind::Jal => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            check_usage(&[rd], options)?;
            let target = eval_operand(operands, 1, labels, consts)?;
            let off = target - addr as i64;
            check_jal_offset(mnemonic, off)?;
            Ok(j_type(off as i32, rd, 0x6F))
        }
        OpKind::Jalr => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            // 两种写法都接受：`jalr rd, 偏移(寄存器)` 与 `jalr rd, 寄存器, 偏移`
            let (imm, rs1) = if operands.len() >= 3 && !operands[1].contains('(') {
                let rs1 = expect_reg(operands, 1, mnemonic, "基址寄存器")?;
                let imm = eval_operand(operands, 2, labels, consts)?;
                (check_imm12(mnemonic, imm)?, rs1)
            } else {
                parse_mem_operand(operands, 1, mnemonic, labels, consts)?
            };
            check_usage(&[rd, rs1], options)?;
            Ok(i_type(imm, rs1, 0x0, rd, 0x67))
        }
        OpKind::Upper => {
            let rd = expect_reg(operands, 0, mnemonic, "目标寄存器")?;
            check_usage(&[rd], options)?;
            let imm = eval_operand(operands, 1, labels, consts)?;
            if !(-(1 << 19)..(1 << 20)).contains(&imm) {
                return err(format!("`{}` 的立即数要在 20 位范围内（写的是 {}）", mnemonic, imm));
            }
            let opcode = if mnemonic == "lui" { 0x37 } else { 0x17 };
            Ok(((imm as u32 & 0x000F_FFFF) << 12) | ((rd as u32) << 7) | opcode)
        }
        OpKind::System { funct12 } => {
            if !operands.is_empty() {
                return err(format!("`{}` 不需要操作数", mnemonic));
            }
            Ok((funct12 << 20) | 0x73)
        }
        OpKind::Fence => Ok(0x0F),
    }
}

fn check_branch_offset(mnemonic: &str, off: i64) -> EResult<()> {
    if off % 2 != 0 {
        return err(format!("`{}` 的跳转目标没有按 2 字节对齐", mnemonic));
    }
    if !(-4096..=4094).contains(&off) {
        return err(format!(
            "`{}` 跳得太远了（超出 ±4KB）；中间插一条 `j` 中转一下",
            mnemonic
        ));
    }
    Ok(())
}

fn check_jal_offset(mnemonic: &str, off: i64) -> EResult<()> {
    if off % 2 != 0 {
        return err(format!("`{}` 的跳转目标没有按 2 字节对齐", mnemonic));
    }
    if !(-(1 << 20)..(1 << 20)).contains(&off) {
        return err(format!("`{}` 跳得太远（超出 ±1MB）", mnemonic));
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  操作数解析
// ─────────────────────────────────────────────────────────────────────────────

fn expect_reg(operands: &[String], index: usize, mnemonic: &str, what: &str) -> EResult<u8> {
    let Some(text) = operands.get(index) else {
        return err(format!("`{}` 缺少{}（第 {} 个操作数）", mnemonic, what, index + 1));
    };
    match reg_index(text) {
        Some(i) => Ok(i as u8),
        None => err(format!(
            "`{}` 不是寄存器名（寄存器叫 x0..x31，也可以写 sp、ra、a0 这些别名）",
            text
        )),
    }
}

/// 取第 `index` 个操作数并求值。
fn eval_operand(
    operands: &[String],
    index: usize,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> EResult<i64> {
    let Some(text) = operands.get(index) else {
        return err(format!("缺少第 {} 个操作数", index + 1));
    };
    eval_symbol(text, labels, consts).map_err(EncodingError)
}

fn eval_symbol(
    text: &str,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> Result<i64, String> {
    if let Some(addr) = labels.get(text) {
        return Ok(*addr as i64);
    }
    if let Some(v) = consts.get(text) {
        return Ok(*v);
    }
    let expr: Expr = crate::ast::parse_expr(text)?;
    eval(&expr, labels, consts)
}

/// 解析 `imm(rs1)` 形式的内存操作数。
fn parse_mem_operand(
    operands: &[String],
    index: usize,
    mnemonic: &str,
    labels: &HashMap<String, u32>,
    consts: &HashMap<String, i64>,
) -> EResult<(i32, u8)> {
    let Some(text) = operands.get(index) else {
        return err(format!("`{}` 缺少内存操作数，应该写成 `偏移(基址寄存器)`", mnemonic));
    };
    if let Some(open) = text.find('(') {
        if !text.ends_with(')') {
            return err(format!("`{}` 的括号没有配对（应写成 `偏移(基址寄存器)`）", text));
        }
        let offset_text = text[..open].trim();
        let reg_text = text[open + 1..text.len() - 1].trim();
        let offset = if offset_text.is_empty() {
            0
        } else {
            eval_symbol(offset_text, labels, consts).map_err(EncodingError)? as i32
        };
        let base = match reg_index(reg_text) {
            Some(i) => i as u8,
            None => return err(format!("`{}` 不是寄存器名", reg_text)),
        };
        let offset = check_imm12(mnemonic, offset as i64)?;
        return Ok((offset, base));
    }
    err(format!("`{}` 应该写成 `偏移(基址寄存器)` 的形式，比如 `0(x1)`、`8(sp)`", text))
}

fn check_imm12(mnemonic: &str, imm: i64) -> EResult<i32> {
    if !(-2048..=2047).contains(&imm) {
        return err(format!(
            "`{}` 的立即数要在 -2048..2047 之间（写的是 {}）；更大的数请先用 `li` 放进寄存器",
            mnemonic, imm
        ));
    }
    Ok(imm as i32)
}

fn check_usage(regs: &[u8], options: &Options) -> EResult<()> {
    if options.forbidden_regs.is_empty() {
        return Ok(());
    }
    for &r in regs {
        let name = reg_name(r as usize);
        let abi = reg_abi_name(r as usize);
        if options.forbidden_regs.iter().any(|f| f == &name || f == abi) {
            return err(format!("本关禁止使用寄存器 {}（{}）", name, abi));
        }
    }
    Ok(())
}

// ── 位级编码（RISC-V 标准格式） ──

fn r_type(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    (funct7 << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((rd as u32) << 7)
        | opcode
}

fn i_type(imm: i32, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    (((imm as u32) & 0xFFF) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((rd as u32) << 7)
        | opcode
}

fn s_type(imm: i32, rs2: u8, rs1: u8, funct3: u32, opcode: u32) -> u32 {
    let imm = (imm as u32) & 0xFFF;
    ((imm >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((imm & 0x1F) << 7)
        | opcode
}

fn b_type(imm: i32, rs2: u8, rs1: u8, funct3: u32, opcode: u32) -> u32 {
    let imm = imm as u32;
    (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3F) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | (((imm >> 1) & 0xF) << 8)
        | (((imm >> 11) & 1) << 7)
        | opcode
}

fn j_type(imm: i32, rd: u8, opcode: u32) -> u32 {
    let imm = imm as u32;
    (((imm >> 20) & 1) << 31)
        | (((imm >> 1) & 0x3FF) << 21)
        | (((imm >> 11) & 1) << 20)
        | (((imm >> 12) & 0xFF) << 12)
        | ((rd as u32) << 7)
        | opcode
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(text_line: &str) -> u32 {
        enc_at(text_line, 0)
    }

    fn enc_at(text_line: &str, addr: u32) -> u32 {
        let line = crate::token::tokenize_line(text_line, 1);
        let mnemonic = line.head.clone().unwrap();
        let labels = HashMap::new();
        let consts = HashMap::new();
        match encode(&mnemonic, &line.operands, addr, &labels, &consts, &Options::default()) {
            Ok(w) => w[0],
            Err(e) => panic!("`{}` 编码失败：{}（操作数 {:?}）", text_line, e.message(), line.operands),
        }
    }

    #[test]
    fn reference_encodings_match_the_riscv_spec() {
        // 这些值是 RISC-V 规范里的标准编码
        assert_eq!(enc("add x3, x1, x2"), 0x0020_81B3);
        assert_eq!(enc("addi x2, x1, 7"), 0x0070_8113);
        assert_eq!(enc("sub x3, x1, x2"), 0x4020_81B3);
        assert_eq!(enc("mul x10, x1, x1"), 0x0210_8533);
        assert_eq!(enc("lw x5, 16(x2)"), 0x0101_2283);
        assert_eq!(enc("sw x5, 16(x2)"), 0x0051_2823);
        assert_eq!(enc("lui x3, 0x12345"), 0x1234_51B7);
        assert_eq!(enc("jalr x0, x1, 0"), 0x0000_8067);
        assert_eq!(enc("ecall"), 0x0000_0073);
        assert_eq!(enc("ebreak"), 0x0010_0073);
        assert_eq!(enc("slli x1, x2, 3"), 0x0031_1093);
        assert_eq!(enc("srai x1, x2, 3"), 0x4031_5093);
    }

    #[test]
    fn branches_and_jumps_use_absolute_targets() {
        let labels: HashMap<String, u32> = [("t".to_string(), 8u32)].into_iter().collect();
        let consts = HashMap::new();
        let ops = vec!["x1".to_string(), "x1".to_string(), "t".to_string()];
        // 地址 0 处 beq x1, x1, t(=+8)
        assert_eq!(
            encode("beq", &ops, 0, &labels, &consts, &Options::default()).unwrap()[0],
            0x0010_8463
        );
        // 反向跳转：地址 16 处跳到 8
        let w = encode("beq", &ops, 16, &labels, &consts, &Options::default()).unwrap()[0];
        assert_eq!(w & 0x7000, 0x0000);
        // jal ra, +12
        let jops = vec!["ra".to_string(), "12".to_string()];
        assert_eq!(
            encode("jal", &jops, 0, &labels, &consts, &Options::default()).unwrap()[0],
            0x00C0_00EF
        );
    }

    #[test]
    fn li_expands_to_two_instructions_with_fixed_length() {
        let labels = HashMap::new();
        let consts = HashMap::new();
        let opts = Options::default();
        let big = vec!["x1".to_string(), "0x12345678".to_string()];
        let w = encode("li", &big, 0, &labels, &consts, &opts).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0], 0x1234_50B7, "lui x1, 0x12345");
        assert_eq!(w[1], 0x6780_8093, "addi x1, x1, 0x678");

        let small = vec!["x1".to_string(), "5".to_string()];
        let w = encode("li", &small, 0, &labels, &consts, &opts).unwrap();
        assert_eq!(w.len(), 2, "长度固定，两遍扫描地址才不会漂移");
        assert_eq!(w[1], enc("addi x1, x1, 5"));
    }

    #[test]
    fn aliases_encode_like_their_base_instructions() {
        assert_eq!(enc("mv x1, x2"), enc("addi x1, x2, 0"));
        assert_eq!(enc("neg x1, x2"), enc("sub x1, x0, x2"));
        assert_eq!(enc("not x1, x2"), enc("xori x1, x2, -1"));
        assert_eq!(enc("nop"), enc("addi x0, x0, 0"));
        assert_eq!(enc("ret"), enc("jalr x0, ra, 0"));
        assert_eq!(enc("seqz x1, x2"), enc("sltiu x1, x2, 1"));
        assert_eq!(enc("snez x1, x2"), enc("sltu x1, x0, x2"));
    }

    #[test]
    fn slp_encodes_cycles_in_funct12() {
        assert_eq!(enc("slp 1000"), (1000u32 << 20) | 0x73);
    }

    #[test]
    fn bad_operands_give_readable_errors() {
        let labels = HashMap::new();
        let consts = HashMap::new();
        let e = |line: &str| {
            let l = crate::token::tokenize_line(line, 1);
            encode(
                &l.head.clone().unwrap(),
                &l.operands,
                0,
                &labels,
                &consts,
                &Options::default(),
            )
            .unwrap_err()
            .message()
        };
        assert!(e("add x1, x2").contains("缺少"));
        assert!(e("addi x1, x2, 99999").contains("立即数"));
        assert!(e("lw x1, x2").contains("偏移(基址寄存器)"));
        assert!(e("add x1, x2, x99").contains("不是寄存器名"));
        assert!(e("slli x1, x2, 40").contains("移位量"));
        assert!(e("bogus x1, x2").contains("不认识"));
    }

    #[test]
    fn whitelist_accepts_pseudo_when_expansion_is_allowed() {
        let opts = Options { available_ops: vec!["lui".into(), "addi".into()], ..Options::default() };
        assert!(opts.op_allowed("li"), "li 展开成 lui+addi，两者都在白名单里");
        assert!(!opts.op_allowed("mul"));
        assert!(!opts.op_allowed("j"), "j 需要 jal，不在白名单里");
    }

    #[test]
    fn forbidden_registers_are_rejected() {
        let opts = Options { forbidden_regs: vec!["x9".into()], ..Options::default() };
        let ops = vec!["x9".to_string(), "x1".to_string(), "x2".to_string()];
        let labels = HashMap::new();
        let consts = HashMap::new();
        let e = encode("add", &ops, 0, &labels, &consts, &opts).unwrap_err();
        assert!(e.message().contains("禁止"));
    }
}
