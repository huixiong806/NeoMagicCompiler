//! RV32IM 指令解码。
//!
//! 只做"机器码 → 指令"的翻译，不产生副作用，便于单独测试。

/// 一条解码后的 RV32IM 指令。
///
/// 加载/存储指令同时携带地址寄存器与偏移，执行阶段负责算有效地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    // ── 寄存器-寄存器运算 ──
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    Sll { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    Xor { rd: u8, rs1: u8, rs2: u8 },
    Srl { rd: u8, rs1: u8, rs2: u8 },
    Sra { rd: u8, rs1: u8, rs2: u8 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },

    // ── M 扩展 ──
    Mul { rd: u8, rs1: u8, rs2: u8 },
    Div { rd: u8, rs1: u8, rs2: u8 },
    Divu { rd: u8, rs1: u8, rs2: u8 },
    Rem { rd: u8, rs1: u8, rs2: u8 },
    Remu { rd: u8, rs1: u8, rs2: u8 },

    // ── 立即数运算 ──
    Addi { rd: u8, rs1: u8, imm: i32 },
    Slti { rd: u8, rs1: u8, imm: i32 },
    Sltiu { rd: u8, rs1: u8, imm: i32 },
    Xori { rd: u8, rs1: u8, imm: i32 },
    Ori { rd: u8, rs1: u8, imm: i32 },
    Andi { rd: u8, rs1: u8, imm: i32 },
    Slli { rd: u8, rs1: u8, shamt: u32 },
    Srli { rd: u8, rs1: u8, shamt: u32 },
    Srai { rd: u8, rs1: u8, shamt: u32 },

    // ── 加载 ──
    Lb { rd: u8, rs1: u8, imm: i32 },
    Lh { rd: u8, rs1: u8, imm: i32 },
    Lw { rd: u8, rs1: u8, imm: i32 },
    Lbu { rd: u8, rs1: u8, imm: i32 },
    Lhu { rd: u8, rs1: u8, imm: i32 },

    // ── 存储 ──
    Sb { rs2: u8, rs1: u8, imm: i32 },
    Sh { rs2: u8, rs1: u8, imm: i32 },
    Sw { rs2: u8, rs1: u8, imm: i32 },

    // ── 分支 ──
    Beq { rs1: u8, rs2: u8, imm: i32 },
    Bne { rs1: u8, rs2: u8, imm: i32 },
    Blt { rs1: u8, rs2: u8, imm: i32 },
    Bge { rs1: u8, rs2: u8, imm: i32 },
    Bltu { rs1: u8, rs2: u8, imm: i32 },
    Bgeu { rs1: u8, rs2: u8, imm: i32 },

    // ── 跳转与高位立即数 ──
    Jal { rd: u8, imm: i32 },
    Jalr { rd: u8, rs1: u8, imm: i32 },
    Lui { rd: u8, imm: i32 },
    Auipc { rd: u8, imm: i32 },

    // ── 其他 ──
    Fence,
    Ebreak,
    Ecall,
    /// 原创：等待指令。`cycles` 为等待消耗的周期数。
    Slp { cycles: u32 },
}

impl Instruction {
    /// 指令助记符（用于白名单校验与错误信息）。
    pub fn mnemonic(&self) -> &'static str {
        use Instruction::*;
        match self {
            Add { .. } => "add",
            Sub { .. } => "sub",
            Sll { .. } => "sll",
            Slt { .. } => "slt",
            Sltu { .. } => "sltu",
            Xor { .. } => "xor",
            Srl { .. } => "srl",
            Sra { .. } => "sra",
            Or { .. } => "or",
            And { .. } => "and",
            Mul { .. } => "mul",
            Div { .. } => "div",
            Divu { .. } => "divu",
            Rem { .. } => "rem",
            Remu { .. } => "remu",
            Addi { .. } => "addi",
            Slti { .. } => "slti",
            Sltiu { .. } => "sltiu",
            Xori { .. } => "xori",
            Ori { .. } => "ori",
            Andi { .. } => "andi",
            Slli { .. } => "slli",
            Srli { .. } => "srli",
            Srai { .. } => "srai",
            Lb { .. } => "lb",
            Lh { .. } => "lh",
            Lw { .. } => "lw",
            Lbu { .. } => "lbu",
            Lhu { .. } => "lhu",
            Sb { .. } => "sb",
            Sh { .. } => "sh",
            Sw { .. } => "sw",
            Beq { .. } => "beq",
            Bne { .. } => "bne",
            Blt { .. } => "blt",
            Bge { .. } => "bge",
            Bltu { .. } => "bltu",
            Bgeu { .. } => "bgeu",
            Jal { .. } => "jal",
            Jalr { .. } => "jalr",
            Lui { .. } => "lui",
            Auipc { .. } => "auipc",
            Fence => "fence",
            Ebreak => "ebreak",
            Ecall => "ecall",
            Slp { .. } => "slp",
        }
    }

    /// 该指令读取的寄存器（用于禁止寄存器检查）。`x0` 不计入。
    pub fn reads(&self) -> Vec<u8> {
        use Instruction::*;
        let mut v = Vec::new();
        let mut push = |r: u8| {
            if r != 0 && !v.contains(&r) {
                v.push(r);
            }
        };
        match *self {
            Add { rs1, rs2, .. }
            | Sub { rs1, rs2, .. }
            | Sll { rs1, rs2, .. }
            | Slt { rs1, rs2, .. }
            | Sltu { rs1, rs2, .. }
            | Xor { rs1, rs2, .. }
            | Srl { rs1, rs2, .. }
            | Sra { rs1, rs2, .. }
            | Or { rs1, rs2, .. }
            | And { rs1, rs2, .. }
            | Mul { rs1, rs2, .. }
            | Div { rs1, rs2, .. }
            | Divu { rs1, rs2, .. }
            | Rem { rs1, rs2, .. }
            | Remu { rs1, rs2, .. } => {
                push(rs1);
                push(rs2);
            }
            Addi { rs1, .. }
            | Slti { rs1, .. }
            | Sltiu { rs1, .. }
            | Xori { rs1, .. }
            | Ori { rs1, .. }
            | Andi { rs1, .. }
            | Slli { rs1, .. }
            | Srli { rs1, .. }
            | Srai { rs1, .. }
            | Lb { rs1, .. }
            | Lh { rs1, .. }
            | Lw { rs1, .. }
            | Lbu { rs1, .. }
            | Lhu { rs1, .. }
            | Jalr { rs1, .. } => push(rs1),
            Sb { rs1, rs2, .. } | Sh { rs1, rs2, .. } | Sw { rs1, rs2, .. } => {
                push(rs1);
                push(rs2);
            }
            Beq { rs1, rs2, .. }
            | Bne { rs1, rs2, .. }
            | Blt { rs1, rs2, .. }
            | Bge { rs1, rs2, .. }
            | Bltu { rs1, rs2, .. }
            | Bgeu { rs1, rs2, .. } => {
                push(rs1);
                push(rs2);
            }
            _ => {}
        }
        v
    }

    /// 该指令写入的寄存器（`x0` 写入被忽略，不计入）。
    pub fn writes(&self) -> Vec<u8> {
        use Instruction::*;
        let rd = match *self {
            Add { rd, .. }
            | Sub { rd, .. }
            | Sll { rd, .. }
            | Slt { rd, .. }
            | Sltu { rd, .. }
            | Xor { rd, .. }
            | Srl { rd, .. }
            | Sra { rd, .. }
            | Or { rd, .. }
            | And { rd, .. }
            | Mul { rd, .. }
            | Div { rd, .. }
            | Divu { rd, .. }
            | Rem { rd, .. }
            | Remu { rd, .. }
            | Addi { rd, .. }
            | Slti { rd, .. }
            | Sltiu { rd, .. }
            | Xori { rd, .. }
            | Ori { rd, .. }
            | Andi { rd, .. }
            | Slli { rd, .. }
            | Srli { rd, .. }
            | Srai { rd, .. }
            | Lb { rd, .. }
            | Lh { rd, .. }
            | Lw { rd, .. }
            | Lbu { rd, .. }
            | Lhu { rd, .. }
            | Jal { rd, .. }
            | Jalr { rd, .. }
            | Lui { rd, .. }
            | Auipc { rd, .. } => rd,
            _ => 0,
        };
        if rd == 0 { Vec::new() } else { vec![rd] }
    }
}

/// 解码错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("未知指令（机器码 0x{0:08X}）")]
    Unknown(u32),
    #[error("保留的移位指令（funct7=0x{0:02X} 非法）")]
    BadShift(u32),
}

/// 把 32 位机器码解码为指令。
pub fn decode(word: u32) -> Result<Instruction, DecodeError> {
    let opcode = word & 0x7F;
    let rd = ((word >> 7) & 0x1F) as u8;
    let funct3 = (word >> 12) & 0x7;
    let rs1 = ((word >> 15) & 0x1F) as u8;
    let rs2 = ((word >> 20) & 0x1F) as u8;
    let funct7 = (word >> 25) & 0x7F;

    /// I 型立即数（算术/加载）
    fn imm_i(word: u32) -> i32 {
        ((word as i32) >> 20) as i32
    }
    /// S 型立即数（存储）
    fn imm_s(word: u32) -> i32 {
        let lo = (word >> 7) & 0x1F;
        let hi = (word >> 25) & 0x7F;
        let v = (hi << 5) | lo;
        ((v << 20) as i32) >> 20
    }
    /// B 型立即数（分支）
    fn imm_b(word: u32) -> i32 {
        let b11 = (word >> 7) & 0x1;
        let b4_1 = (word >> 8) & 0xF;
        let b10_5 = (word >> 25) & 0x3F;
        let b12 = (word >> 31) & 0x1;
        let v = (b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1);
        ((v << 19) as i32) >> 19
    }
    /// J 型立即数（jal）
    fn imm_j(word: u32) -> i32 {
        let b20 = (word >> 31) & 0x1;
        let b10_1 = (word >> 21) & 0x3FF;
        let b11 = (word >> 20) & 0x1;
        let b19_12 = (word >> 12) & 0xFF;
        let v = (b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1);
        ((v << 11) as i32) >> 11
    }

    use Instruction::*;
    Ok(match opcode {
        // ── OP：寄存器运算 ──
        0x33 => match (funct3, funct7) {
            (0x0, 0x00) => Add { rd, rs1, rs2 },
            (0x0, 0x20) => Sub { rd, rs1, rs2 },
            (0x1, 0x00) => Sll { rd, rs1, rs2 },
            (0x2, 0x00) => Slt { rd, rs1, rs2 },
            (0x3, 0x00) => Sltu { rd, rs1, rs2 },
            (0x4, 0x00) => Xor { rd, rs1, rs2 },
            (0x5, 0x00) => Srl { rd, rs1, rs2 },
            (0x5, 0x20) => Sra { rd, rs1, rs2 },
            (0x6, 0x00) => Or { rd, rs1, rs2 },
            (0x7, 0x00) => And { rd, rs1, rs2 },
            (0x0, 0x01) => Mul { rd, rs1, rs2 },
            (0x4, 0x01) => Div { rd, rs1, rs2 },
            (0x5, 0x01) => Divu { rd, rs1, rs2 },
            (0x6, 0x01) => Rem { rd, rs1, rs2 },
            (0x7, 0x01) => Remu { rd, rs1, rs2 },
            _ => return Err(DecodeError::Unknown(word)),
        },
        // ── OP-IMM ──
        0x13 => match funct3 {
            0x0 => Addi { rd, rs1, imm: imm_i(word) },
            0x2 => Slti { rd, rs1, imm: imm_i(word) },
            0x3 => Sltiu { rd, rs1, imm: imm_i(word) },
            0x4 => Xori { rd, rs1, imm: imm_i(word) },
            0x6 => Ori { rd, rs1, imm: imm_i(word) },
            0x7 => Andi { rd, rs1, imm: imm_i(word) },
            0x1 => match funct7 {
                0x00 => Slli { rd, rs1, shamt: rs2 as u32 },
                _ => return Err(DecodeError::BadShift(funct7 as u32)),
            },
            0x5 => match funct7 {
                0x00 => Srli { rd, rs1, shamt: rs2 as u32 },
                0x20 => Srai { rd, rs1, shamt: rs2 as u32 },
                _ => return Err(DecodeError::BadShift(funct7 as u32)),
            },
            _ => return Err(DecodeError::Unknown(word)),
        },
        // ── LOAD ──
        0x03 => {
            let imm = imm_i(word);
            match funct3 {
                0x0 => Lb { rd, rs1, imm },
                0x1 => Lh { rd, rs1, imm },
                0x2 => Lw { rd, rs1, imm },
                0x4 => Lbu { rd, rs1, imm },
                0x5 => Lhu { rd, rs1, imm },
                _ => return Err(DecodeError::Unknown(word)),
            }
        }
        // ── STORE ──
        0x23 => {
            let imm = imm_s(word);
            match funct3 {
                0x0 => Sb { rs2, rs1, imm },
                0x1 => Sh { rs2, rs1, imm },
                0x2 => Sw { rs2, rs1, imm },
                _ => return Err(DecodeError::Unknown(word)),
            }
        }
        // ── BRANCH ──
        0x63 => {
            let imm = imm_b(word);
            match funct3 {
                0x0 => Beq { rs1, rs2, imm },
                0x1 => Bne { rs1, rs2, imm },
                0x4 => Blt { rs1, rs2, imm },
                0x5 => Bge { rs1, rs2, imm },
                0x6 => Bltu { rs1, rs2, imm },
                0x7 => Bgeu { rs1, rs2, imm },
                _ => return Err(DecodeError::Unknown(word)),
            }
        }
        // ── JAL ──
        0x6F => Jal { rd, imm: imm_j(word) },
        // ── JALR ──
        0x67 => match funct3 {
            0x0 => Jalr { rd, rs1, imm: imm_i(word) },
            _ => return Err(DecodeError::Unknown(word)),
        },
        // ── LUI / AUIPC ──
        0x37 => Lui { rd, imm: (word & 0xFFFF_F000) as i32 },
        0x17 => Auipc { rd, imm: (word & 0xFFFF_F000) as i32 },
        // ── MISC-MEM ──
        0x0F => Fence,
        // ── SYSTEM：断点 / 系统调用 / 原创 slp ──
        // 自定义 `slp` 用 funct12 承载周期数（与 `ebreak`/`ecall` 同格式，无需额外寄存器）。
        0x73 => match (funct3, (word >> 20) & 0xFFF) {
            (0x0, 0x001) => Ebreak,
            (0x0, 0x000) => Ecall,
            (0x0, cycles) => Slp { cycles },
            _ => return Err(DecodeError::Unknown(word)),
        },
        _ => return Err(DecodeError::Unknown(word)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_add_and_addi() {
        // add x3, x1, x2
        assert_eq!(decode(0x002081B3).unwrap(), Instruction::Add { rd: 3, rs1: 1, rs2: 2 });
        // addi x2, x1, 7
        assert_eq!(decode(0x00708113).unwrap(), Instruction::Addi { rd: 2, rs1: 1, imm: 7 });
    }

    #[test]
    fn decodes_negative_immediates() {
        // addi x1, x1, -1
        let word = 0xFFF08093;
        assert_eq!(decode(word).unwrap(), Instruction::Addi { rd: 1, rs1: 1, imm: -1 });
    }

    #[test]
    fn decodes_branches_and_raw_system() {
        // ebreak
        assert_eq!(decode(0x00100073).unwrap(), Instruction::Ebreak);
        // ecall
        assert_eq!(decode(0x00000073).unwrap(), Instruction::Ecall);
        // slp 5 周期
        assert_eq!(decode(0x00500073).unwrap(), Instruction::Slp { cycles: 5 });
        // beq x1, x1, +8
        assert_eq!(
            decode(0x00108463).unwrap(),
            Instruction::Beq { rs1: 1, rs2: 1, imm: 8 }
        );
    }

    #[test]
    fn decodes_loads_stores_and_upper_imm() {
        // lw x5, 16(x2)
        assert_eq!(decode(0x01012283).unwrap(), Instruction::Lw { rd: 5, rs1: 2, imm: 16 });
        // sw x5, 16(x2)
        assert_eq!(decode(0x00512823).unwrap(), Instruction::Sw { rs2: 5, rs1: 2, imm: 16 });
        // lui x3, 0x12345
        assert_eq!(decode(0x123451B7).unwrap(), Instruction::Lui { rd: 3, imm: 0x12345000u32 as i32 });
    }

    #[test]
    fn mnemonic_and_accessors() {
        let add = Instruction::Add { rd: 3, rs1: 1, rs2: 2 };
        assert_eq!(add.mnemonic(), "add");
        assert_eq!(add.reads(), vec![1, 2]);
        assert_eq!(add.writes(), vec![3]);
        // 写 x0 视为不写
        let add0 = Instruction::Add { rd: 0, rs1: 1, rs2: 2 };
        assert!(add0.writes().is_empty());
    }
}
