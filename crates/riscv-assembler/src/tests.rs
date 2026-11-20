//! 汇编器集成测试：从源码到机器码，再真的跑一遍。

use crate::{assemble, assemble_with, AsmError, Options};
use riscv_simulator::{PauseReason, Simulator, SimulatorConfig};

/// 汇编 + 运行，返回模拟器（已跑完）。
fn run(source: &str) -> Simulator {
    run_with(source, &Options::default())
}

fn run_with(source: &str, options: &Options) -> Simulator {
    let asm = match assemble_with(source, options) {
        Ok(a) => a,
        Err(errs) => panic!("汇编失败：\n{}", format_errors(&errs)),
    };
    let mut sim = Simulator::new();
    let cfg = asm.base_config(SimulatorConfig::new());
    sim.configure(cfg);
    asm.apply_to(&mut sim).expect("装载失败");
    sim.run().expect("运行失败");
    sim
}

fn format_errors(errs: &[AsmError]) -> String {
    errs.iter().map(|e| format!("  {}\n", e)).collect()
}

fn expect_errors(source: &str) -> Vec<AsmError> {
    match assemble(source) {
        Ok(_) => panic!("本来期望报错，结果汇编成功了"),
        Err(e) => e,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  基本流程
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn assembles_and_runs_a_simple_program() {
    // 记录寄存器初值靠不上的东西，所以这里自己算
    let src = "
        .text
            addi x1, x0, 10
            addi x2, x0, 32
            add  x3, x1, x2      # 42
            addi x17, x0, 0      # SYS_EXIT
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x3"), Some(42));
}

#[test]
fn labels_and_forward_branches_work() {
    let src = "
        .text
            addi x1, x0, 0
            addi x2, x0, 5
        loop:
            addi x1, x1, 1
            blt  x1, x2, loop
            addi x17, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x1"), Some(5), "循环应当停在 5");
}

#[test]
fn backward_branch_offset_is_negative() {
    let src = "
        .text
            addi x1, x0, 3
        top:
            addi x1, x1, -1
            bnez x1, top
            addi x17, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x1"), Some(0));
}

#[test]
fn li_loads_arbitrary_constants() {
    let src = "
        .text
            li x1, 0x12345678
            li x2, -1
            li x3, 0
            addi x17, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x1"), Some(0x1234_5678));
    assert_eq!(sim.get_reg("x2"), Some(0xFFFF_FFFF));
    assert_eq!(sim.get_reg("x3"), Some(0));
}

#[test]
fn li_into_a_register_then_use_it_as_base() {
    let src = "
        .text
            li   x1, 0x1000
            addi x2, x0, 99
            sw   x2, 0(x1)
            lw   x3, 0(x1)
            addi x17, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x3"), Some(99));
    assert_eq!(sim.get_mem(0x1000, 4), 99u32.to_le_bytes().to_vec());
}

#[test]
fn data_section_is_loaded_into_memory() {
    let src = "
        .text
            la   x1, message
            lbu  x2, 0(x1)
            lbu  x3, 5(x1)
            addi x17, x0, 0
            ecall
        .data
        message:
            .asciz \"hello!\"
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x2"), Some(b'h' as u32));
    assert_eq!(sim.get_reg("x3"), Some(b'!' as u32), "第 6 个字节是 '!'");
    assert_eq!(sim.get_cstr(0x1000, 16), "hello!");
}

#[test]
fn data_section_supports_words_and_bytes() {
    let src = "
        .text
            li  x1, 0x2000
            lw  x2, 0(x1)
            lbu x3, 4(x1)
            addi x17, x0, 0
            ecall
        .data
            .org 0x2000
        nums:
            .word 0xDEADBEEF
            .byte 7, 8
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x2"), Some(0xDEAD_BEEF));
    assert_eq!(sim.get_reg("x3"), Some(7));
}

#[test]
fn equs_and_expressions_are_evaluated() {
    let src = "
        .equ BASE, 0x1000
        .equ SIZE, 4
        .text
            li   x1, BASE + SIZE*2
            li   x2, (1+2)*(3+4)
            addi x17, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("x1"), Some(0x1008));
    assert_eq!(sim.get_reg("x2"), Some(21));
}

#[test]
fn word_directive_can_reference_labels() {
    let src = "
        .text
        target:
            nop
            li  x1, 0x3000
            lw  x2, 0(x1)
            addi x17, x0, 0
            ecall
        .data
            .org 0x3000
            .word target
    ";
    let asm = assemble(src).unwrap();
    let target = *asm.symbols.get("target").unwrap();
    let sim = run(src);
    assert_eq!(sim.get_reg("x2"), Some(target));
}

#[test]
fn subroutine_call_and_return() {
    let src = "
        .text
        main:
            jal  ra, square     # a0 里放好参数再调用
            addi a7, x0, 2      # 十进制输出 a0
            ecall
            addi a7, x0, 0
            ecall
        square:
            mul  a0, a0, a0
            ret
    ";
    let asm = assemble(src).unwrap();
    // main 在 0；main 体 4 条指令（jal + addi + ecall + addi + ecall = 5 条）→ square 在 20
    assert_eq!(asm.symbols.get("main"), Some(&0u32));
    assert_eq!(asm.symbols.get("square"), Some(&20u32), "square 标签地址不对");
    let mut sim = Simulator::new();
    sim.configure(asm.base_config(SimulatorConfig::new()));
    sim.set_reg("a0", 7).unwrap();
    asm.apply_to(&mut sim).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.text_output.trim(), "49");
    assert_eq!(sim.get_reg("a0"), Some(49));
}

#[test]
fn stack_save_and_restore_via_directives() {
    let src = "
        .text
            li   sp, 0x3000
            li   a0, 1234
            addi sp, sp, -8
            sw   a0, 4(sp)
            li   a0, 0
            lw   a0, 4(sp)
            addi sp, sp, 8
            addi a7, x0, 0
            ecall
    ";
    let sim = run(src);
    assert_eq!(sim.get_reg("a0"), Some(1234));
    assert_eq!(sim.get_reg("sp"), Some(0x3000));
}

#[test]
fn slp_consumes_cycles() {
    let src = "
        .text
            slp 500
            addi x17, x0, 0
            ecall
    ";
    let mut sim = Simulator::new();
    let asm = assemble(src).unwrap();
    sim.configure(asm.base_config(SimulatorConfig::new()));
    asm.apply_to(&mut sim).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Exit { code: 0 });
    assert_eq!(r.tick, 503, "slp 自身 1 个周期 + 等待 500 + 2 条收尾指令");
}

#[test]
fn ebreak_is_assembled_and_pauses_in_debug_mode() {
    let src = "
        .text
            addi x1, x0, 1
            ebreak
            addi x1, x1, 1
            addi x17, x0, 0
            ecall
    ";
    let asm = assemble(src).unwrap();
    let mut cfg = SimulatorConfig::debug_mode();
    cfg.pause_on.ecall = false;
    let mut sim = Simulator::new();
    sim.configure(asm.base_config(cfg));
    asm.apply_to(&mut sim).unwrap();
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Ebreak);
    assert_eq!(sim.get_reg("x1"), Some(1));
}

// ─────────────────────────────────────────────────────────────────────────────
//  布局与元数据
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn base_address_can_be_moved() {
    let src = "
        .text
        start:
            nop
    ";
    let opts = Options { base_addr: 0x400, ..Options::default() };
    let asm = assemble_with(src, &opts).unwrap();
    assert_eq!(asm.base_addr, 0x400);
    assert_eq!(asm.symbols.get("start"), Some(&0x400));
    assert_eq!(asm.words.len(), 1);
}

#[test]
fn source_map_links_addresses_and_lines() {
    let src = ".text\naddi x1, x0, 1\naddi x2, x0, 2\n";
    let asm = assemble(src).unwrap();
    assert_eq!(asm.addr_to_line(0), Some(2));
    assert_eq!(asm.addr_to_line(4), Some(3));
    assert_eq!(asm.line_to_addr(3), Some(4));
}

#[test]
fn org_creates_a_gap_filled_with_zeros() {
    let src = "
        .text
            addi x1, x0, 1
            .org 0x20
            addi x2, x0, 2
    ";
    let asm = assemble(src).unwrap();
    assert_eq!(asm.words.len(), 9, "0x20/4 + 1 条指令");
    assert_eq!(asm.words[1], 0);
    assert_eq!(asm.symbols.get("__none"), None);
    assert_eq!(asm.source_map[1].0, 0x20);
}

#[test]
fn duplicate_labels_are_reported() {
    let errs = expect_errors(".text\na:\nnop\na:\nnop\n");
    assert!(errs.iter().any(|e| e.message.contains("重复定义")), "{:?}", errs);
    // 报错指向**重复出现的那一行**（第 4 行）
    assert_eq!(errs[0].line, 4);
}

// ─────────────────────────────────────────────────────────────────────────────
//  错误信息
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unknown_mnemonic_is_reported_with_line_number() {
    let errs = expect_errors(".text\n  frobnicate x1, x2\n");
    assert!(errs[0].message.contains("frobnicate"), "{:?}", errs);
    assert_eq!(errs[0].line, 2);
}

#[test]
fn unknown_directive_is_reported() {
    let errs = expect_errors(".text\n  .frobnicate 1\n");
    assert!(errs[0].message.contains("指示符"), "{:?}", errs);
}

#[test]
fn unknown_label_is_reported() {
    let errs = expect_errors(".text\n  j nowhere\n");
    assert!(errs[0].message.contains("nowhere"), "{:?}", errs);
}

#[test]
fn whitelist_rejects_disallowed_mnemonics() {
    let opts = Options { available_ops: vec!["addi".into(), "ecall".into()], ..Options::default() };
    let src = ".text\n addi x1, x0, 1\n mul x2, x1, x1\n addi a7, x0, 0\n ecall\n";
    let errs = assemble_with(src, &opts).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("mul")), "{:?}", errs);
    assert_eq!(errs[0].line, 3);
}

/// 四条"交换操作数"的伪分支指令：`bgt` `ble` `bgtu` `bleu`。
///
/// 这四条最容易写反（`b_type` 的参数是 rs2 在前、rs1 在后），而且反了之后
/// **不会报错**，只会在运行时静默走错分支 —— 一个用它做数字范围判断的
/// 编译器关卡就是这样产出错误代码的。所以这里用真的跑一遍来钉死语义。
#[test]
fn swapped_operand_branches_have_the_right_semantics() {
    // (助记符, 比较用的 x2 初值, 期望"没跳走（落到下一条）"的次数)
    //   循环变量 x1 取 0,1,2,3,4；条件成立就跳到 `taken`，
    //   于是 `x3` 恰好数出"没跳走"的次数 —— 用它反推分支语义有没有反。
    let cases: &[(&str, i32, i32)] = &[
        ("bgt", 3, 4),  // x1 >  3 只在 x1=4 成立 → 落下去 4 次
        ("ble", 3, 1),  // x1 <= 3 在 0..3 成立 → 只落下去 1 次（x1=4）
        ("bgtu", 3, 4), // 无符号 x1 > 3 → 同上
        ("bleu", 3, 1), // 无符号 x1 <= 3 → 同上
        ("ble", 0, 4),  // 边界：x1 <= 0 只在 x1=0 成立 → 落下去 4 次
        ("bgt", 4, 5),  // 边界：x1 最大就是 4，一次都不成立 → 全落下去
    ];
    for (mnemonic, bound, expected) in cases {
        let src = format!(
            "
            .text
                addi x1, x0, 0
                addi x2, x0, {}
                addi x3, x0, 0
            loop:
                {}   x1, x2, taken
                addi x3, x3, 1
            taken:
                addi x1, x1, 1
                addi x4, x0, 4
                bge  x4, x1, loop
                addi x17, x0, 0
                ecall
            ",
            bound, mnemonic
        );
        let sim = run(&src);
        let got = sim.get_reg("x3").unwrap_or(999);
        assert_eq!(
            got, *expected as u32,
            "`{} x1, x2, taken`（x2={}）应当有 {} 次没跳走，实际 {} 次\
             —— 操作数很可能写反了（b_type 的参数是 rs2 在前、rs1 在后）",
            mnemonic, bound, expected, got
        );
    }
}

#[test]
fn whitelist_allows_pseudo_instructions_made_of_allowed_parts() {
    // 白名单只有 lui/addi 时，`li` 也应该能用（它就是 lui+addi）
    let opts = Options { available_ops: vec!["lui".into(), "addi".into(), "ecall".into()], ..Options::default() };
    let src = ".text\n li x1, 0x12345\n addi a7, x0, 0\n ecall\n";
    let asm = assemble_with(src, &opts).unwrap();
    assert_eq!(asm.words.len(), 4);
}

#[test]
fn forbidden_registers_are_rejected_by_the_assembler() {
    let opts = Options { forbidden_regs: vec!["s1".into()], ..Options::default() };
    let src = ".text\n addi x9, x0, 1\n";
    let errs = assemble_with(src, &opts).unwrap_err();
    assert!(errs[0].message.contains("x9"), "{:?}", errs);
}

#[test]
fn instructions_in_data_section_are_rejected() {
    let errs = expect_errors(".text\nnop\n.data\naddi x1, x0, 1\n");
    assert!(errs.iter().any(|e| e.message.contains("数据段")), "{:?}", errs);
}

#[test]
fn branch_too_far_is_reported_helpfully() {
    // 造一段超过 ±4KB 的代码，然后往回跳
    let mut src = String::from(".text\nfar:\n");
    for _ in 0..1200 {
        src.push_str("nop\n");
    }
    src.push_str("beq x1, x2, far\n");
    let errs = expect_errors(&src);
    assert!(errs.iter().any(|e| e.message.contains("跳得太远")), "{:?}", errs);
}

#[test]
fn empty_source_is_reported() {
    let errs = expect_errors("# 只有注释\n");
    assert!(errs[0].message.contains("没有任何指令"), "{:?}", errs);
}

#[test]
fn errors_are_sorted_and_carry_line_numbers() {
    let errs = expect_errors(".text\n bogus1\n bogus2\n");
    assert_eq!(errs.len(), 2);
    assert_eq!(errs[0].line, 2);
    assert_eq!(errs[1].line, 3);
}

// ─────────────────────────────────────────────────────────────────────────────
//  与模拟器的一致性
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn every_entry_in_the_op_table_can_be_encoded() {
    // 编码表里每条指令都应当能编码出来（用合法操作数）
    let samples: &[(&str, &str)] = &[
        ("add", "x1, x2, x3"),
        ("sub", "x1, x2, x3"),
        ("sll", "x1, x2, x3"),
        ("slt", "x1, x2, x3"),
        ("sltu", "x1, x2, x3"),
        ("xor", "x1, x2, x3"),
        ("srl", "x1, x2, x3"),
        ("sra", "x1, x2, x3"),
        ("or", "x1, x2, x3"),
        ("and", "x1, x2, x3"),
        ("mul", "x1, x2, x3"),
        ("div", "x1, x2, x3"),
        ("divu", "x1, x2, x3"),
        ("rem", "x1, x2, x3"),
        ("remu", "x1, x2, x3"),
        ("addi", "x1, x2, 1"),
        ("slti", "x1, x2, 1"),
        ("sltiu", "x1, x2, 1"),
        ("xori", "x1, x2, 1"),
        ("ori", "x1, x2, 1"),
        ("andi", "x1, x2, 1"),
        ("slli", "x1, x2, 1"),
        ("srli", "x1, x2, 1"),
        ("srai", "x1, x2, 1"),
        ("lb", "x1, 0(x2)"),
        ("lh", "x1, 0(x2)"),
        ("lw", "x1, 0(x2)"),
        ("lbu", "x1, 0(x2)"),
        ("lhu", "x1, 0(x2)"),
        ("sb", "x1, 0(x2)"),
        ("sh", "x1, 0(x2)"),
        ("sw", "x1, 0(x2)"),
        ("beq", "x1, x2, tgt"),
        ("bne", "x1, x2, tgt"),
        ("blt", "x1, x2, tgt"),
        ("bge", "x1, x2, tgt"),
        ("bltu", "x1, x2, tgt"),
        ("bgeu", "x1, x2, tgt"),
        ("jal", "ra, tgt"),
        ("jalr", "ra, 0(x2)"),
        ("lui", "x1, 0x1"),
        ("auipc", "x1, 0x1"),
        ("fence", ""),
        ("ebreak", ""),
        ("ecall", ""),
    ];
    for (mnemonic, operands) in samples {
        let src = format!(".text\ntgt:\n  {} {}\n", mnemonic, operands);
        let asm = assemble(&src)
            .unwrap_or_else(|e| panic!("`{}` 编不出来：{}", mnemonic, format_errors(&e)));
        assert!(!asm.words.is_empty(), "`{}` 没有产出机器码", mnemonic);
    }
    assert_eq!(samples.len(), crate::OP_TABLE.len(), "编码表有新增指令但测试没覆盖");
}
