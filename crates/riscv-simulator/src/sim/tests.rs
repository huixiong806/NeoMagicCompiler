//! 模拟器集成测试：直接用手工编码的机器码验证执行语义。

use super::*;
use crate::config::DEFAULT_RAM_SIZE;
use crate::sim::WordSize;

// ─────────────────────────────────────────────────────────────────────────────
//  极简编码器：只覆盖测试需要的指令，独立于汇编器（保证两条路径互相验证）
// ─────────────────────────────────────────────────────────────────────────────

fn r_type(funct7: u32, rs2: u32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

fn i_type(imm: i32, rs1: u32, funct3: u32, rd: u32, opcode: u32) -> u32 {
    ((imm as u32 & 0xFFF) << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | opcode
}

fn s_type(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm = (imm as u32) & 0xFFF; // 取低 12 位，负数自动成为补码形式
    ((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (funct3 << 12) | ((imm & 0x1F) << 7) | opcode
}

fn b_type(imm: i32, rs2: u32, rs1: u32, funct3: u32, opcode: u32) -> u32 {
    let imm = imm as u32;
    (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3F) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (funct3 << 12)
        | (((imm >> 1) & 0xF) << 8)
        | (((imm >> 11) & 1) << 7)
        | opcode
}

fn j_type(imm: i32, rd: u32, opcode: u32) -> u32 {
    let imm = imm as u32;
    (((imm >> 20) & 1) << 31)
        | (((imm >> 1) & 0x3FF) << 21)
        | (((imm >> 11) & 1) << 20)
        | (((imm >> 12) & 0xFF) << 12)
        | (rd << 7)
        | opcode
}

fn add(rd: u32, rs1: u32, rs2: u32) -> u32 {
    r_type(0, rs2, rs1, 0, rd, 0x33)
}
fn sub(rd: u32, rs1: u32, rs2: u32) -> u32 {
    r_type(0x20, rs2, rs1, 0, rd, 0x33)
}
fn mul(rd: u32, rs1: u32, rs2: u32) -> u32 {
    r_type(0x01, rs2, rs1, 0, rd, 0x33)
}
fn div(rd: u32, rs1: u32, rs2: u32) -> u32 {
    r_type(0x01, rs2, rs1, 4, rd, 0x33)
}
fn rem(rd: u32, rs1: u32, rs2: u32) -> u32 {
    r_type(0x01, rs2, rs1, 6, rd, 0x33)
}
fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 0, rd, 0x13)
}
/// `lui rd, 0xF0000` → 0xF000_0000（MMIO 基址）
fn mmio_base(rd: u32) -> u32 {
    lui(rd, 0xF000_0000u32 as i32)
}
fn andi(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 7, rd, 0x13)
}
fn slli(rd: u32, rs1: u32, sh: u32) -> u32 {
    i_type(sh as i32, rs1, 1, rd, 0x13)
}
fn srai(rd: u32, rs1: u32, sh: u32) -> u32 {
    i_type(((0x20 << 5) | sh) as i32, rs1, 5, rd, 0x13)
}
fn lw(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 2, rd, 0x03)
}
fn lb(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 0, rd, 0x03)
}
fn lbu(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 4, rd, 0x03)
}
fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    s_type(imm, rs2, rs1, 2, 0x23)
}
fn sb(rs2: u32, rs1: u32, imm: i32) -> u32 {
    s_type(imm, rs2, rs1, 0, 0x23)
}
fn beq(rs1: u32, rs2: u32, imm: i32) -> u32 {
    b_type(imm, rs2, rs1, 0, 0x63)
}
fn bne(rs1: u32, rs2: u32, imm: i32) -> u32 {
    b_type(imm, rs2, rs1, 1, 0x63)
}
fn blt(rs1: u32, rs2: u32, imm: i32) -> u32 {
    b_type(imm, rs2, rs1, 4, 0x63)
}
fn jal(rd: u32, imm: i32) -> u32 {
    j_type(imm, rd, 0x6F)
}
fn jalr(rd: u32, rs1: u32, imm: i32) -> u32 {
    i_type(imm, rs1, 0, rd, 0x67)
}
fn lui(rd: u32, imm: i32) -> u32 {
    ((imm as u32) & 0xFFFF_F000) | (rd << 7) | 0x37
}
fn ebreak() -> u32 {
    0x0010_0073
}
fn ecall() -> u32 {
    0x0000_0073
}
fn slp(cycles: u32) -> u32 {
    (cycles << 20) | 0x73
}

/// `li rd, imm`：lui + addi 展开（只用于 |imm| < 2^31 的测试常量）。
fn li(rd: u32, imm: i32) -> Vec<u32> {
    let hi = (imm.wrapping_add(0x800)) >> 12;
    let lo = imm - (hi << 12);
    if hi != 0 {
        vec![lui(rd, hi << 12), addi(rd, rd, lo)]
    } else {
        vec![addi(rd, 0, lo)]
    }
}

/// 把即刻数写进寄存器，不做 lui（仅用于 12 位以内的值）。
fn li_small(rd: u32, imm: i32) -> u32 {
    addi(rd, 0, imm)
}

fn run_program(words: &[u32]) -> Simulator {
    let mut sim = Simulator::new();
    let cfg = SimulatorConfig::new();
    sim.configure(cfg);
    sim.load_binary(words).expect("加载代码失败");
    sim.run().expect("运行失败");
    sim
}

// ─────────────────────────────────────────────────────────────────────────────
//  基础运算
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn add_and_sub_work() {
    let words = [
        addi(1, 0, 10),
        addi(2, 0, 3),
        add(3, 1, 2), // 13
        sub(4, 3, 2), // 10
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x3"), Some(13));
    assert_eq!(sim.get_reg("x4"), Some(10));
    assert_eq!(sim.get_reg("x0"), Some(0), "x0 恒为 0");
}

#[test]
fn x0_is_never_written() {
    let words = [addi(0, 0, 99), addi(1, 0, 5), add(2, 0, 1), addi(17, 0, 0), ecall()];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x0"), Some(0));
    assert_eq!(sim.get_reg("x2"), Some(5), "x0 读出恒为 0");
}

#[test]
fn mul_div_rem_and_div_by_zero() {
    let words = [
        addi(1, 0, 17),
        addi(2, 0, 5),
        mul(3, 1, 2), // 85
        div(4, 1, 2), // 3
        rem(5, 1, 2), // 2
        addi(6, 0, 0),
        div(7, 1, 6), // 除零 → -1
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x3"), Some(85));
    assert_eq!(sim.get_reg("x4"), Some(3));
    assert_eq!(sim.get_reg("x5"), Some(2));
    assert_eq!(sim.get_reg("x7"), Some(u32::MAX), "除零按 RISC-V 规定返回 -1");
}

#[test]
fn signed_shifts_and_negative_values() {
    let words = [
        addi(1, 0, -8),
        srai(2, 1, 1), // -4
        slli(3, 1, 2), // -32
        addi(4, 0, 1),
        sub(5, 4, 1), // 9
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x2"), Some((-4i32) as u32));
    assert_eq!(sim.get_reg("x3"), Some((-32i32) as u32));
    assert_eq!(sim.get_reg("x5"), Some(9));
}

// ─────────────────────────────────────────────────────────────────────────────
//  内存
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn load_store_roundtrip() {
    let mut words = li(2, 0x1000);
    words.extend([
        addi(1, 0, 42),
        sw(1, 2, 0),
        lw(5, 2, 0),
        addi(17, 0, 0),
        ecall(),
    ]);
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x5"), Some(42));
    assert_eq!(sim.get_mem(0x1000, 4), 42u32.to_le_bytes().to_vec());
}

#[test]
fn byte_store_and_signed_load() {
    let mut words = li(2, 0x2000);
    words.extend([
        addi(1, 0, -1),
        sb(1, 2, 0),
        lb(3, 2, 0),  // 符号扩展 → -1
        lbu(4, 2, 0), // 无符号 → 255
        addi(17, 0, 0),
        ecall(),
    ]);
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x3"), Some(u32::MAX));
    assert_eq!(sim.get_reg("x4"), Some(255));
}

#[test]
fn segfault_reports_address() {
    let mut words = li(1, 0x7FFF_0000);
    words.push(lw(2, 1, 0));
    words.push(li_small(17, 0));
    words.push(ecall());
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    match r.reason {
        PauseReason::Error { msg, .. } => assert!(msg.contains("段错误"), "实际信息：{}", msg),
        other => panic!("期望段错误，实际 {:?}", other),
    }
}

#[test]
fn misaligned_word_access_is_an_error() {
    let mut words = li(1, 0x1001);
    words.push(lw(2, 1, 0));
    words.push(li_small(17, 0));
    words.push(ecall());
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    match r.reason {
        PauseReason::Error { msg, .. } => assert!(msg.contains("未对齐"), "实际信息：{}", msg),
        other => panic!("期望未对齐错误，实际 {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  分支与循环
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn branch_taken_and_not_taken() {
    // 相等则跳到设置为 1 的位置，否则设为 2
    let words = [
        addi(1, 0, 5),
        addi(2, 0, 5),
        beq(1, 2, 8),  // 跳到 +8（第 5 条）
        addi(3, 0, 2), // 不该执行
        addi(3, 0, 1),
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x3"), Some(1));
}

#[test]
fn signed_comparison_branch() {
    let words = [
        addi(1, 0, -3),
        addi(2, 0, 1),
        blt(1, 2, 8),  // -3 < 1 → 跳
        addi(3, 0, 0),
        addi(3, 0, 7),
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x3"), Some(7), "blt 必须按有符号比较");
}

#[test]
fn loop_sums_to_ten() {
    // x1 = 累加器, x2 = 计数器；求 1..=5 的和
    let words = [
        addi(1, 0, 0),  // 0: sum = 0
        addi(2, 0, 1),  // 4: i = 1
        addi(3, 0, 6),  // 8: limit = 6
        // 12: loop
        add(1, 1, 2),   // sum += i
        addi(2, 2, 1),  // i++
        blt(2, 3, -8),  // if i < 6 → back to 12
        addi(17, 0, 0),
        ecall(),
    ];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x1"), Some(15), "1+2+3+4+5 = 15");
}

#[test]
fn infinite_loop_hits_instruction_limit() {
    let words = [jal(0, 0)];
    let mut cfg = SimulatorConfig::new();
    cfg.max_instructions = 500;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Limit);
    assert!(r.tick <= 501);
}

// ─────────────────────────────────────────────────────────────────────────────
//  函数调用与栈
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn jal_and_ret_call_a_subroutine() {
    // 主程序：x1=7，调用 square 子程序，然后输出结果
    // 返回地址放在 x5（不破坏 x1）——这正是要教的"ra 只是普通寄存器"
    //  0: addi a7, 0, 2      指定 SYS_PUTC_INT
    //  1: jal  x5, +16       → 子程序在序号 5（0x14）；x5 = 8
    //  2: ecall              输出 a0 = "49"
    //  3: addi a7, 0, 0      SYS_EXIT
    //  4: ecall              结束
    //  5: square: mul a0, x1, x1   a0 = 49
    //  6:         jalr x0, x5, 0   返回序号 2
    let words = [
        addi(17, 0, 2),
        jal(5, 16),
        ecall(),
        addi(17, 0, 0),
        ecall(),
        mul(10, 1, 1),
        jalr(0, 5, 0),
    ];
    // 先自己核对解码结果，避免测试里的编码器本身出错
    assert_eq!(crate::decode::decode(words[1]).unwrap(), Instruction::Jal { rd: 5, imm: 16 });
    assert_eq!(1 * 4 + 16, 5 * 4, "jal 必须落在 mul 上");
    assert_eq!(
        crate::decode::decode(words[6]).unwrap(),
        Instruction::Jalr { rd: 0, rs1: 5, imm: 0 }
    );

    let mut cfg = SimulatorConfig::new();
    cfg.initial_regs.insert("x1".to_string(), 7);

    let mut probe = Simulator::new();
    probe.configure(cfg.clone());
    probe.load_binary(&words).unwrap();
    let mut visited = Vec::new();
    for _ in 0..16 {
        if probe.is_halted() {
            break;
        }
        visited.push(probe.get_pc());
        let _ = probe.step(1).unwrap();
    }
    assert_eq!(
        visited,
        vec![0x0, 0x4, 0x14, 0x18, 0x8, 0xC, 0x10],
        "执行路径应当是「调用 → 返回 → 输出 → 结束」"
    );

    // 单独再跑一遍，只为取到 a0 与 x5 的终值
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Exit { code: 49 }, "退出码就是 a0（49）");
    assert_eq!(sim.get_reg("a0"), Some(49), "square(7) = 49");
    assert_eq!(sim.get_reg("x5"), Some(8), "jal 把下一条指令地址写回 x5");
    assert_eq!(sim.text_output(), "49", "子程序留下的 a0 被主程序输出");
}

#[test]
fn nested_calls_need_a_stack() {
    // 栈指针由配置给出（0x3000），程序里不再需要"载入 0x3000"的指令
    //  0: addi a0, 0, 3          a0 = 3
    //  1: jal  ra, f             → f 在序号 6（偏移 20；序号 2..5 是主程序收尾）
    //  2: addi a7, 0, 2          输出 a0
    //  3: ecall
    //  4: addi a7, 0, 0 ; 5: ecall  （结束）
    //  6: f: addi sp, sp, -8     压栈
    //  7:    sw   ra, 4(sp)      保存 ra
    //  8:    addi a0, 0, 5
    //  9:    jal  x6, g          → g 在序号 13（偏移 16）；用 x6 而不是 ra，
    //                            这样"只有一个 ra"的痛点才暴露出来
    // 10:    lw   ra, 4(sp)      恢复 ra（这一步若缺失就会死循环）
    // 11:    addi sp, sp, 8      弹栈
    // 12:    jalr x0, ra, 0      返回主程序
    // 13: g: mul  a0, a0, a0     a0 = a0²
    // 14:    jalr x0, x6, 0      返回 f
    let words = [
        addi(10, 0, 3),
        jal(1, 20),
        addi(17, 0, 2),
        ecall(),
        addi(17, 0, 0),
        ecall(),
        addi(2, 2, -8),
        sw(1, 2, 4),
        addi(10, 0, 5),
        jal(6, 16),
        lw(1, 2, 4),
        addi(2, 2, 8),
        jalr(0, 1, 0),
        mul(10, 10, 10),
        jalr(0, 6, 0),
    ];
    // 手工核对跳转目标与关键编码
    assert_eq!(1 * 4 + 20, 6 * 4, "jal 到 f");
    assert_eq!(9 * 4 + 16, 13 * 4, "jal 到 g");
    assert_eq!(
        crate::decode::decode(words[6]).unwrap(),
        Instruction::Addi { rd: 2, rs1: 2, imm: -8 },
        "压栈要用负立即数"
    );
    assert_eq!(
        crate::decode::decode(words[7]).unwrap(),
        Instruction::Sw { rs2: 1, rs1: 2, imm: 4 },
        "sw 的偏移必须是 +4"
    );

    let mut cfg = SimulatorConfig::new();
    cfg.initial_regs.insert("sp".to_string(), 0x3000);

    let mut visited = Vec::new();
    let mut probe = Simulator::new();
    probe.configure(cfg.clone());
    probe.load_binary(&words).unwrap();
    for _ in 0..30 {
        if probe.is_halted() {
            break;
        }
        visited.push(probe.get_pc());
        let _ = probe.step(1).unwrap();
    }
    assert_eq!(
        visited,
        vec![0, 4, 24, 28, 32, 36, 52, 56, 40, 44, 48, 8, 12, 16, 20],
        "主程序 → f → g → 逐层返回 → 主程序输出 → 结束"
    );
    assert_eq!(probe.get_reg("a0"), Some(25), "f 把 a0 设为 5，g 把它平方成 25");

    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(
        r.reason,
        PauseReason::Exit { code: 25 },
        "必须能顺利返回并结束（否则说明 ra 没被恢复）"
    );
    assert_eq!(sim.get_reg("sp"), Some(0x3000), "栈指针被正确恢复");
    assert_eq!(sim.get_reg("x1"), Some(8), "ra 被正确恢复为 f 的返回地址");
    assert_eq!(sim.text_output(), "25", "主程序输出的是 g 计算出的 25");
    // 栈里保存的 ra 应等于"从 f 返回后主程序继续执行的地址"（序号 2 → 8）
    assert_eq!(
        sim.get_mem(0x2FFC, 4),
        8u32.to_le_bytes().to_vec(),
        "栈里保存的是 jal 写入的返回地址"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  暂停条件
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ebreak_pauses_only_when_enabled() {
    let words = [addi(1, 0, 1), ebreak(), addi(1, 1, 1), addi(17, 0, 0), ecall()];

    let mut cfg = SimulatorConfig::new();
    cfg.pause_on.ebreak = false;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Exit { code: 0 });
    assert_eq!(sim.get_reg("x1"), Some(2), "未开启断点时应继续执行");

    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Ebreak);
    assert_eq!(sim.get_reg("x1"), Some(1), "断点处应停下");
    // 从断点继续：剩下的指令执行完，程序正常结束
    let r2 = sim.run().unwrap();
    assert_eq!(sim.get_reg("x1"), Some(2));
    assert_eq!(r2.reason, PauseReason::Exit { code: 0 });
}

#[test]
fn slp_consumes_cycles() {
    let words = [addi(1, 0, 1), slp(1000), addi(1, 1, 1), addi(17, 0, 0), ecall()];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.tick, 5 + 1000, "slp 把等待周期计入已消耗周期");
}

// ─────────────────────────────────────────────────────────────────────────────
//  系统调用与 MMIO
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn sys_exit_reports_code() {
    let words = [addi(10, 0, 7), addi(17, 0, 0), ecall()];
    let sim = run_program(&words);
    assert!(sim.is_halted());
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.exit_code, Some(7));
}

#[test]
fn sys_putc_and_putc_int_write_text() {
    let mut words = vec![];
    for ch in "Hi".bytes() {
        words.push(addi(10, 0, ch as i32));
        words.push(addi(17, 0, 1));
        words.push(ecall());
    }
    words.push(addi(10, 0, 42));
    words.push(addi(17, 0, 2));
    words.push(ecall());
    words.push(addi(17, 0, 0));
    words.push(ecall());
    let sim = run_program(&words);
    assert_eq!(sim.text_output(), "Hi42");
}

#[test]
fn mmio_input_queue_feeds_program() {
    // 读 3 个值到 x1..x3，然后求和（x20 = 输入端口地址）
    let words = [
        mmio_base(20),
        addi(20, 20, 4), // x20 = 0xF0000004（输入端口）
        lw(1, 20, 0),
        lw(2, 20, 0),
        add(3, 1, 2),
        addi(17, 0, 0),
        ecall(),
    ];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    sim.push_mmio(MMIO_INPUT, &[10, 32]);
    let r = sim.run().unwrap();
    assert_eq!(sim.get_reg("x3"), Some(42));
    assert_eq!(r.reason, PauseReason::Exit { code: 0 });
}

#[test]
fn empty_mmio_read_can_pause() {
    let words = [
        mmio_base(20),
        addi(20, 20, 4), // x20 = 输入端口
        lw(1, 20, 0),
        addi(17, 0, 0),
        ecall(),
    ];
    let mut cfg = SimulatorConfig::new();
    cfg.pause_on.mmio_read_empty = true;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::MmioReadEmpty { addr: MMIO_INPUT });
    // 外部补数据后继续（reset 保留输入队列，符合"输入属于测试环境"的约定）
    sim.push_mmio(MMIO_INPUT, &[9]);
    sim.reset();
    let r2 = sim.run().unwrap();
    assert_eq!(sim.get_reg("x1"), Some(9));
    assert_eq!(r2.reason, PauseReason::Exit { code: 0 });
}

#[test]
fn mmio_write_pause_gives_value() {
    let words = [
        mmio_base(20), // x20 = 0xF0000000（显示端口）
        addi(1, 0, 65),
        sw(1, 20, 0),
        addi(17, 0, 0),
        ecall(),
    ];
    let mut cfg = SimulatorConfig::new();
    cfg.pause_on.mmio_write = true;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::MmioWrite { addr: MMIO_DISPLAY, value: 65 });
    assert_eq!(r.text_output, "A");
}

#[test]
fn sys_getint_parses_decimal() {
    let words = [addi(17, 0, 4), ecall(), addi(17, 0, 0), ecall()];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    sim.push_mmio(MMIO_INPUT, b"  -1234x".iter().map(|&b| b as u32).collect::<Vec<_>>().as_slice());
    sim.run().unwrap();
    assert_eq!(sim.get_reg("a0"), Some((-1234i32) as u32));
}

#[test]
fn unknown_syscall_is_an_error() {
    let words = [addi(17, 0, 77), ecall()];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert!(r.reason.is_failure());
}

// ─────────────────────────────────────────────────────────────────────────────
//  安全字段
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn allowed_ops_whitelist_is_enforced_at_runtime() {
    let words = [addi(1, 0, 1), mul(2, 1, 1), addi(17, 0, 0), ecall()];
    let mut cfg = SimulatorConfig::new();
    cfg.allowed_ops = vec!["addi".into(), "ecall".into()];
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    match r.reason {
        PauseReason::Error { msg, .. } => assert!(msg.contains("mul"), "实际：{}", msg),
        other => panic!("期望白名单错误，实际 {:?}", other),
    }
}

#[test]
fn forbidden_registers_are_rejected() {
    let words = [addi(9, 0, 1), addi(17, 0, 0), ecall()];
    let mut cfg = SimulatorConfig::new();
    cfg.forbidden_regs = vec!["s1".into()]; // x9
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    match r.reason {
        PauseReason::Error { msg, .. } => assert!(msg.contains("x9"), "实际：{}", msg),
        other => panic!("期望禁止寄存器错误，实际 {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  单步、倒退、重置
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn step_and_step_back_restore_state() {
    let words = [
        addi(1, 0, 1),
        addi(1, 1, 1),
        addi(1, 1, 1),
        addi(17, 0, 0),
        ecall(),
    ];
    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();

    sim.step(3).unwrap();
    assert_eq!(sim.get_reg("x1"), Some(3));
    assert_eq!(sim.get_pc(), 12);
    assert_eq!(sim.get_tick(), 3);

    let back = sim.step_back(2);
    assert_eq!(back.reverted, 2);
    assert_eq!(sim.get_reg("x1"), Some(1));
    assert_eq!(sim.get_pc(), 4, "倒退 2 条后 PC 回到第 2 条指令");
    assert_eq!(sim.get_tick(), 1);

    // 再前进，状态应一致
    sim.step(1).unwrap();
    assert_eq!(sim.get_reg("x1"), Some(2));
}

#[test]
fn rewind_to_arbitrary_tick() {
    let mut words: Vec<u32> = (0..9).map(|_| addi(1, 1, 1)).collect();
    words.push(addi(17, 0, 0));
    words.push(ecall());
    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    sim.step(5).unwrap();
    assert_eq!(sim.get_reg("x1"), Some(5));
    let r = sim.rewind_to(2);
    assert_eq!(r.reverted, 3);
    assert_eq!(sim.get_tick(), 2);
    assert_eq!(sim.get_reg("x1"), Some(2));
}

#[test]
fn memory_writes_are_undone_on_step_back() {
    let mut words = li(2, 0x1000);
    words.extend([addi(1, 0, 5), sw(1, 2, 0), addi(17, 0, 0), ecall()]);
    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    sim.step(4).unwrap();
    assert_eq!(sim.get_mem(0x1000, 4), 5u32.to_le_bytes().to_vec());
    sim.step_back(1);
    assert_eq!(sim.get_mem(0x1000, 4), 0u32.to_le_bytes().to_vec(), "内存写入被撤销");
}

#[test]
fn reset_returns_to_initial_state_and_keeps_output() {
    let words = [addi(1, 0, 5), addi(17, 0, 1), addi(10, 1, 0), ecall()];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    sim.run().unwrap();
    assert_eq!(sim.get_reg("x1"), Some(5));
    assert_eq!(sim.text_output(), "\u{5}");
    sim.reset();
    assert_eq!(sim.get_reg("x1"), Some(0));
    assert_eq!(sim.get_pc(), 0);
    assert_eq!(sim.text_output(), "\u{5}", "reset 不清空 MMIO 输出");
}

#[test]
fn history_capacity_limits_rewind() {
    let words: Vec<u32> = std::iter::repeat(addi(1, 1, 1)).take(50).chain([ecall()]).collect();
    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    cfg.history_capacity = 10;
    let mut sim = Simulator::new();
    sim.configure(cfg);
    sim.load_binary(&words).unwrap();
    sim.step(20).unwrap();
    let r = sim.step_back(100);
    assert_eq!(r.reverted, 10, "最多只能回退历史容量条");
    assert_eq!(sim.get_tick(), 10);
    assert_eq!(sim.get_reg("x1"), Some(10));
}

// ─────────────────────────────────────────────────────────────────────────────
//  杂项
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn default_ram_size_is_64k() {
    let sim = Simulator::new();
    assert_eq!(sim.get_mem(DEFAULT_RAM_SIZE - 1, 1).len(), 1);
}

#[test]
fn lui_and_auipc() {
    let words = [lui(1, 0x1234_5000u32 as i32), addi(17, 0, 0), ecall()];
    let sim = run_program(&words);
    assert_eq!(sim.get_reg("x1"), Some(0x1234_5000));
}

#[test]
fn word_size_helpers() {
    assert_eq!(WordSize::Byte.bytes(), 1);
    assert_eq!(WordSize::Half.bytes(), 2);
    assert_eq!(WordSize::Word.bytes(), 4);
}

#[test]
fn abi_and_standard_register_names_are_interchangeable() {
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.set_reg("sp", 0x2000).unwrap();
    assert_eq!(sim.get_reg("x2"), Some(0x2000));
    sim.set_reg("x10", 7).unwrap();
    assert_eq!(sim.get_reg("a0"), Some(7));
    assert!(sim.set_reg("x99", 0).is_err());
}

#[test]
fn putc_syscall_does_not_stop_the_program() {
    // SYS_PUTC 只是输出，不应像 SYS_EXIT 那样暂停执行
    let words = [addi(10, 0, 65), addi(17, 0, 1), ecall(), addi(17, 0, 0), ecall()];
    let mut sim = Simulator::new();
    sim.configure(SimulatorConfig::new());
    sim.load_binary(&words).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Exit { code: 65 }, "退出码来自 a0（'A' = 65）");
    assert_eq!(sim.text_output(), "A");
    assert_eq!(r.instructions_executed, 5, "5 条指令全部执行完");
}
