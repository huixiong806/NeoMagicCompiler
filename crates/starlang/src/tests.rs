//! 端到端测试：星咒源码 → 编译 → 汇编 → 模拟器执行 → 检查结果。
//!
//! 这是编译器正确性的主战场。每条测试都用真实的执行结果验证，
//! 而不是比对生成的汇编文本（文本随时可以优化，语义不能错）。

use crate::compile;
use riscv_assembler::{assemble, assemble_with, Options};
use riscv_simulator::{PauseReason, Simulator, SimulatorConfig};

/// 引导代码放在这个地址（代码段之后，互不干扰）。
const BOOT_ADDR: u32 = 0x800;

/// 编译、汇编、并运行。`call_fn = None` 表示跑 `main`（程序模式）。
fn run_with(source: &str, call_fn: Option<(&str, &[i32])>) -> Simulator {
    let out = match compile(source) {
        Ok(o) => o,
        Err(e) => panic!("编译失败：{}\n源码：\n{}", e, source),
    };
    let asm = match assemble(&out.assembly) {
        Ok(a) => a,
        Err(errs) => {
            let text: String = errs.iter().map(|e| format!("  {}\n", e)).collect();
            panic!(
                "汇编编译器自己的产物失败（说明代码生成有 bug）：\n{}\n生成的汇编：\n{}",
                text, out.assembly
            );
        }
    };

    let mut cfg = SimulatorConfig::new();
    cfg.pause_on = riscv_simulator::PauseOn::batch();
    cfg.max_instructions = 5_000_000;
    cfg.initial_regs.insert("sp".to_string(), crate::STACK_TOP);
    cfg = asm.base_config(cfg);

    match call_fn {
        None => {
            let mut sim = Simulator::new();
            sim.configure(cfg);
            asm.apply_to(&mut sim).expect("装载失败");
            let r = sim.run().expect("运行失败");
            report(&r, &out.assembly, "", &mut sim);
            sim
        }
        Some((name, args)) => {
            let sym = *asm.symbols.get(name).unwrap_or_else(|| {
                panic!(
                    "找不到函数 `{}`；符号表：{:?}",
                    name,
                    asm.symbols.keys().collect::<Vec<_>>()
                )
            });
            // 引导代码：设参数 → 跳到目标函数 → 结束后 ecall 退出
            let boot_src = build_boot(sym, args);
            let boot_asm = assemble_with(
                &boot_src,
                &Options { base_addr: BOOT_ADDR, ..Options::default() },
            )
            .unwrap_or_else(|e| {
                panic!(
                    "引导代码汇编失败：{:?}\n引导代码：\n{}",
                    e.iter().map(|x| x.to_string()).collect::<Vec<_>>(),
                    boot_src
                )
            });
            // 引导代码装到 0x800，然后把 PC 指过去（注意不要改 cfg.initial_pc：
            // 那会连"程序代码装到哪"一起改掉）
            let mut sim = Simulator::new();
            sim.configure(cfg);
            asm.apply_to(&mut sim).expect("装载失败");
            sim.load_binary_at(BOOT_ADDR, &boot_asm.words).expect("引导代码装载失败");
            sim.set_pc(BOOT_ADDR);
            let r = sim.run().expect("运行失败");
            report(&r, &out.assembly, &boot_src, &mut sim);
            sim
        }
    }
}

/// 运行结果里如果有运行错误，就把现场打出来（调试代码生成时非常有用）。
fn report(
    r: &riscv_simulator::RunResult,
    assembly: &str,
    boot: &str,
    sim: &mut Simulator,
) {
    if let PauseReason::Error { msg, .. } = &r.reason {
        let regs = sim.get_all_regs();
        let mut nonzero: Vec<String> = regs
            .iter()
            .filter(|(_, v)| **v != 0)
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        nonzero.sort();
        panic!(
            "运行出错：{}\n寄存器：{}\n生成的汇编：\n{}\n引导代码：\n{}",
            msg,
            nonzero.join(" "),
            assembly,
            boot
        );
    }
    if r.reason == PauseReason::Limit {
        panic!("程序跑了太多指令（可能是死循环）\n生成的汇编：\n{}", assembly);
    }
    if std::env::var("STARLANG_TRACE").is_ok() {
        let regs = sim.get_all_regs();
        let mut nonzero: Vec<String> = regs
            .iter()
            .filter(|(_, v)| **v != 0)
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        nonzero.sort();
        let trace: Vec<u32> = (0..6).map(|i| sim.get_mem(0x800 + i * 4, 4)[0] as u32).collect();
        eprintln!(
            "TRACE reason={:?} regs=[{}] boot_bytes={:?}",
            r.reason,
            nonzero.join(" "),
            trace
        );
    }
}

/// 生成"设置参数并调用目标函数"的引导代码。
fn build_boot(target: u32, args: &[i32]) -> String {
    let mut s = String::from(".text\n");
    for (i, a) in args.iter().enumerate() {
        s.push_str(&format!("li a{}, {}\n", i, a));
    }
    s.push_str(&format!("li t5, {}\n", target));
    s.push_str("jalr ra, t5, 0\n");
    s.push_str("li a7, 0\necall\n");
    s
}

/// 跑一个"函数模式"的测试：调用 `solve`，返回 a0。
fn call(source: &str, args: &[i32]) -> i32 {
    let sim = run_with(source, Some(("solve", args)));
    sim.get_reg("a0").unwrap() as i32
}

// ─────────────────────────────────────────────────────────────────────────────
//  基本表达式
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn returns_a_constant() {
    assert_eq!(call("int solve() { return 42; }", &[]), 42);
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(call("int solve(int a, int b) { return a + b; }", &[3, 7]), 10);
    assert_eq!(call("int solve(int a, int b) { return a - b; }", &[3, 7]), -4);
    assert_eq!(call("int solve(int a, int b) { return a * b; }", &[6, 7]), 42);
    assert_eq!(call("int solve(int a, int b) { return a / b; }", &[17, 5]), 3);
    assert_eq!(call("int solve(int a, int b) { return a % b; }", &[17, 5]), 2);
    assert_eq!(call("int solve(int a) { return 1 + a * 2; }", &[5]), 11);
    assert_eq!(call("int solve(int a) { return (1 + a) * 2; }", &[5]), 12);
    assert_eq!(call("int solve(int a) { return -a + 10; }", &[3]), 7);
}

#[test]
fn local_variables_and_assignments() {
    assert_eq!(
        call(
            "int solve(int a) { int b = a * 2; b = b + 1; return b; }",
            &[5]
        ),
        11
    );
    assert_eq!(
        call("int solve(int a) { int b; b = a; return b; }", &[9]),
        9
    );
    assert_eq!(
        call("int solve(int a) { int b = 1; int c = 2; return a + b + c; }", &[3]),
        6
    );
}

#[test]
fn compound_assignment() {
    assert_eq!(call("int solve(int a) { a += 5; return a; }", &[1]), 6);
    assert_eq!(call("int solve(int a) { a *= 3; return a; }", &[4]), 12);
    assert_eq!(call("int solve(int a) { a -= 1; a /= 2; return a; }", &[9]), 4);
    assert_eq!(call("int solve(int a) { a <<= 3; return a; }", &[2]), 16);
}

#[test]
fn increments_and_decrements() {
    assert_eq!(call("int solve(int a) { a++; return a; }", &[5]), 6);
    assert_eq!(call("int solve(int a) { ++a; return a; }", &[5]), 6);
    assert_eq!(call("int solve(int a) { return a++; }", &[5]), 5);
    assert_eq!(call("int solve(int a) { return a--; }", &[5]), 5);
    assert_eq!(call("int solve(int a) { int b = a++; return b * 10 + a; }", &[5]), 56);
}

#[test]
fn comparisons_produce_zero_or_one() {
    assert_eq!(call("int solve(int a, int b) { return a < b; }", &[1, 2]), 1);
    assert_eq!(call("int solve(int a, int b) { return a < b; }", &[2, 2]), 0);
    assert_eq!(call("int solve(int a, int b) { return a >= b; }", &[2, 2]), 1);
    assert_eq!(call("int solve(int a, int b) { return a == b; }", &[2, 3]), 0);
    assert_eq!(call("int solve(int a, int b) { return a != b; }", &[2, 3]), 1);
    assert_eq!(call("int solve(int a, int b) { return a > b; }", &[5, 3]), 1);
    assert_eq!(call("int solve(int a, int b) { return a <= b; }", &[5, 3]), 0);
}

#[test]
fn signed_comparison_handles_negatives() {
    assert_eq!(call("int solve(int a) { return a < 0; }", &[-5]), 1);
    assert_eq!(call("int solve(int a) { return a < 0; }", &[5]), 0);
    assert_eq!(call("int solve(int a, int b) { return a < b; }", &[-5, -1]), 1);
}

#[test]
fn logical_operators_short_circuit() {
    assert_eq!(call("int solve(int a, int b) { return a && b; }", &[1, 1]), 1);
    assert_eq!(call("int solve(int a, int b) { return a && b; }", &[1, 0]), 0);
    assert_eq!(call("int solve(int a, int b) { return a || b; }", &[0, 3]), 1);
    assert_eq!(call("int solve(int a, int b) { return a || b; }", &[0, 0]), 0);
    assert_eq!(call("int solve(int a) { return !a; }", &[0]), 1);
    assert_eq!(call("int solve(int a) { return !a; }", &[7]), 0);
    // 短路：右边有除零也不该被执行
    assert_eq!(
        call("int solve(int a) { if (a == 0) return 0; return (a != 0) || (a / 0 == 1); }", &[5]),
        1
    );
}

#[test]
fn bitwise_operators() {
    assert_eq!(call("int solve(int a, int b) { return a & b; }", &[0b1100, 0b1010]), 0b1000);
    assert_eq!(call("int solve(int a, int b) { return a | b; }", &[0b1100, 0b1010]), 0b1110);
    assert_eq!(call("int solve(int a, int b) { return a ^ b; }", &[0b1100, 0b1010]), 0b0110);
    assert_eq!(call("int solve(int a) { return ~a; }", &[0]), -1);
    assert_eq!(call("int solve(int a) { return a << 2; }", &[3]), 12);
    assert_eq!(call("int solve(int a) { return a >> 1; }", &[8]), 4);
    // 右移是**逻辑**右移：负数高位补 0，这样 `while (x) x >>= 1;` 才会终止
    assert_eq!(
        call("int solve(int a) { return a >> 1; }", &[-8]) as u32,
        0x7FFF_FFFC,
        "负数右移也要补 0"
    );
    // 移位量按 5 位取（和硬件一致）：33 & 31 = 1
    assert_eq!(call("int solve(int a) { return a >> 33; }", &[16]), 8);
    assert_eq!(call("int solve(int a) { return a << 33; }", &[1]), 2);
    // 移位量 ≥ 32 时按 5 位截断（C 里这是未定义行为，这里定义为"按硬件来"）
    assert_eq!(call("int solve(int a) { return a >> 40; }", &[16]), 0, "40 & 31 == 8");
}

#[test]
fn bit_counting_terminates_on_all_ones() {
    // 经典的 `while (x) { count += x & 1; x >>= 1; }`：用算术右移会死循环
    let src = "
        int solve(int x) {
            int c = 0;
            while (x != 0) { if ((x & 1) != 0) c = c + 1; x = x >> 1; }
            return c;
        }
    ";
    assert_eq!(call(src, &[0]), 0);
    assert_eq!(call(src, &[13]), 3);
    assert_eq!(call(src, &[-1]), 32, "0xFFFFFFFF 里有 32 个 1");
}

#[test]
fn ternary_operator() {
    assert_eq!(call("int solve(int a, int b) { return a > b ? a : b; }", &[3, 9]), 9);
    assert_eq!(call("int solve(int a, int b) { return a > b ? a : b; }", &[9, 3]), 9);
}

// ─────────────────────────────────────────────────────────────────────────────
//  控制流
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn if_else() {
    assert_eq!(
        call("int solve(int a) { if (a > 0) return 1; else return -1; }", &[5]),
        1
    );
    assert_eq!(
        call("int solve(int a) { if (a > 0) return 1; else return -1; }", &[-5]),
        -1
    );
    assert_eq!(
        call(
            "int solve(int a) { int r = 0; if (a > 10) { r = 1; } else if (a > 5) { r = 2; } else { r = 3; } return r; }",
            &[7]
        ),
        2
    );
}

#[test]
fn while_loop_sums() {
    assert_eq!(
        call("int solve(int n) { int i = 1; int s = 0; while (i <= n) { s = s + i; i = i + 1; } return s; }", &[5]),
        15
    );
    assert_eq!(
        call("int solve(int n) { int i = 1; int s = 0; while (i <= n) { s = s + i; i = i + 1; } return s; }", &[0]),
        0
    );
}

#[test]
fn for_loop_and_break_continue() {
    assert_eq!(
        call("int solve(int n) { int s = 0; for (int i = 0; i < n; i++) s += i; return s; }", &[5]),
        10
    );
    assert_eq!(
        call(
            "int solve(int n) { int s = 0; for (int i = 0; i < n; i++) { if (i == 3) break; s += i; } return s; }",
            &[10]
        ),
        3
    );
    assert_eq!(
        call(
            "int solve(int n) { int s = 0; for (int i = 0; i < n; i++) { if (i % 2 == 0) continue; s += i; } return s; }",
            &[6]
        ),
        9
    );
}

#[test]
fn do_while_runs_at_least_once() {
    assert_eq!(
        call("int solve(int n) { int i = 0; do { i++; } while (i < n); return i; }", &[0]),
        1
    );
    assert_eq!(
        call("int solve(int n) { int i = 0; do { i++; } while (i < n); return i; }", &[5]),
        5
    );
}

#[test]
fn nested_loops() {
    assert_eq!(
        call(
            "int solve(int n) { int c = 0; for (int i = 0; i < n; i++) for (int j = 0; j < n; j++) c++; return c; }",
            &[4]
        ),
        16
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  函数与递归
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn calls_another_function() {
    assert_eq!(
        call(
            "int square(int x) { return x * x; }\nint solve(int a) { return square(a) + 1; }",
            &[7]
        ),
        50
    );
}

#[test]
fn recursion_factorial() {
    let src = "
        int fact(int n) { if (n <= 1) return 1; return n * fact(n - 1); }
        int solve(int n) { return fact(n); }
    ";
    assert_eq!(call(src, &[0]), 1);
    assert_eq!(call(src, &[1]), 1);
    assert_eq!(call(src, &[5]), 120);
    assert_eq!(call(src, &[10]), 3628800);
}

#[test]
fn recursion_fibonacci() {
    let src = "
        int fib(int n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }
        int solve(int n) { return fib(n); }
    ";
    assert_eq!(call(src, &[0]), 0);
    assert_eq!(call(src, &[1]), 1);
    assert_eq!(call(src, &[10]), 55);
    assert_eq!(call(src, &[20]), 6765);
}

#[test]
fn deep_recursion_uses_the_stack() {
    let src = "
        int depth(int n) { if (n == 0) return 0; return 1 + depth(n - 1); }
        int solve(int n) { return depth(n); }
    ";
    assert_eq!(call(src, &[200]), 200);
}

#[test]
fn many_arguments() {
    assert_eq!(
        call(
            "int add6(int a, int b, int c, int d, int e, int f) { return a+b+c+d+e+f; }\nint solve(int x) { return add6(x,1,2,3,4,5); }",
            &[10]
        ),
        25
    );
}

#[test]
fn void_function() {
    let src = "
        int g = 0;
        void bump() { g = g + 1; }
        int solve(int n) { for (int i = 0; i < n; i++) bump(); return g; }
    ";
    assert_eq!(call(src, &[4]), 4);
}

// ─────────────────────────────────────────────────────────────────────────────
//  指针、数组、结构体
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pointers_and_address_of() {
    assert_eq!(
        call("int solve(int a) { int* p = &a; *p = 99; return a; }", &[1]),
        99
    );
    assert_eq!(
        call("int solve(int a) { int b = 7; int* p = &b; return *p + a; }", &[3]),
        10
    );
}

#[test]
fn swapping_through_pointers() {
    let src = "
        void swap(int* a, int* b) { int t = *a; *a = *b; *b = t; }
        int solve(int x) { int p = x; int q = x * 2; swap(&p, &q); return p * 100 + q; }
    ";
    // x=3 → p=3,q=6 → swap → p=6,q=3 → 603
    assert_eq!(call(src, &[3]), 603);
}

#[test]
fn local_arrays() {
    assert_eq!(
        call("int solve(int n) { int a[5]; for (int i = 0; i < 5; i++) a[i] = i * i; return a[n]; }", &[4]),
        16
    );
    assert_eq!(
        call("int solve(int n) { int a[3] = {10, 20, 30}; return a[n]; }", &[1]),
        20
    );
}

#[test]
fn array_parameter_sums() {
    let src = "
        int sum(int* a, int n) { int s = 0; for (int i = 0; i < n; i++) s += a[i]; return s; }
        int solve(int n) { int a[4]; a[0]=1; a[1]=2; a[2]=3; a[3]=4; return sum(a, n); }
    ";
    assert_eq!(call(src, &[4]), 10);
    assert_eq!(call(src, &[2]), 3);
}

#[test]
fn global_arrays_and_variables() {
    let src = "
        int table[4];
        int count = 0;
        int solve(int n) {
            for (int i = 0; i < 4; i++) { table[i] = n + i; }
            for (int i = 0; i < 4; i++) { count += table[i]; }
            return count;
        }
    ";
    // n=10 → 10+11+12+13 = 46
    assert_eq!(call(src, &[10]), 46);
}

#[test]
fn global_array_initializer_list() {
    let src = "
        int fib[6] = {1, 1, 2, 3, 5, 8};
        int solve(int n) { return fib[n]; }
    ";
    assert_eq!(call(src, &[0]), 1);
    assert_eq!(call(src, &[5]), 8);
}

#[test]
fn pointer_arithmetic_scales_by_element_size() {
    let src = "
        int solve(int n) { int a[4] = {5, 6, 7, 8}; int* p = a; p = p + 2; return *p; }
    ";
    assert_eq!(call(src, &[0]), 7);
}

#[test]
fn struct_member_access() {
    let src = "
        struct rune { int power; int count; };
        int solve(int p, int c) {
            struct rune r;
            r.power = p;
            r.count = c;
            return r.power * r.count;
        }
    ";
    assert_eq!(call(src, &[3, 4]), 12);
}

#[test]
fn struct_pointer_access() {
    let src = "
        struct rune { int power; int count; };
        int total(struct rune* r) { return r->power * r->count; }
        int solve(int p) { struct rune r; r.power = p; r.count = 2; return total(&r); }
    ";
    assert_eq!(call(src, &[5]), 10);
    // 也支持 (*ptr).member 写法
    let src2 = "
        struct rune { int power; int count; };
        int total(struct rune* r) { return (*r).power + (*r).count; }
        int solve(int p) { struct rune r; r.power = p; r.count = 2; return total(&r); }
    ";
    assert_eq!(call(src2, &[5]), 7);
}

#[test]
fn struct_array() {
    let src = "
        struct rune { int power; int count; };
        int solve(int n) {
            struct rune list[3];
            for (int i = 0; i < 3; i++) { list[i].power = i + 1; list[i].count = 2; }
            int s = 0;
            for (int i = 0; i < n; i++) s += list[i].power * list[i].count;
            return s;
        }
    ";
    // (1+2+3)*2 = 12
    assert_eq!(call(src, &[3]), 12);
}

#[test]
fn char_type_and_strings() {
    let src = "
        int solve(int n) { char c = 'A'; return c + n; }
    ";
    assert_eq!(call(src, &[1]), 66);

    let src2 = "
        int strlen2(char* s) { int n = 0; while (s[n] != 0) n++; return n; }
        int solve(int x) { return strlen2(\"hello\") + x; }
    ";
    assert_eq!(call(src2, &[1]), 6);
}

#[test]
fn casts_and_sizeof() {
    assert_eq!(call("int solve(int a) { return (char)a; }", &[321]), 65);
    assert_eq!(call("int solve(int a) { return sizeof(int); }", &[0]), 4);
    assert_eq!(call("int solve(int a) { return sizeof(char); }", &[0]), 1);
    assert_eq!(call("int solve(int a) { int b[10]; return sizeof(b); }", &[0]), 40);
    assert_eq!(call("int solve(int a) { return sizeof(int*); }", &[0]), 4);
}

// ─────────────────────────────────────────────────────────────────────────────
//  程序模式（I/O）
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn program_mode_prints_characters() {
    let src = "
        int main() {
            putchar('H');
            putchar('i');
            return 0;
        }
    ";
    let sim = run_with(src, None);
    assert_eq!(sim.text_output(), "Hi");
}

#[test]
fn program_mode_prints_numbers() {
    let src = "
        int main() {
            print_int(42);
            putchar(' ');
            print_int(-7);
            return 0;
        }
    ";
    let sim = run_with(src, None);
    assert_eq!(sim.text_output(), "42 -7");
}

#[test]
fn program_mode_reads_input_and_echoes() {
    let src = "
        int main() {
            int c;
            int upper = 0;
            while ((c = getchar()) != 0) {
                if (c >= 'a' && c <= 'z') putchar(c - 32);
                else putchar(c);
            }
            return upper;
        }
    ";
    let out = compile(src).unwrap();
    let asm = assemble(&out.assembly).unwrap();
    let mut cfg = SimulatorConfig::new();
    cfg.pause_on = riscv_simulator::PauseOn::batch();
    cfg.max_instructions = 5_000_000;
    cfg.initial_regs.insert("sp".to_string(), crate::STACK_TOP);
    cfg = asm.base_config(cfg);
    let mut sim = Simulator::new();
    sim.configure(cfg);
    asm.apply_to(&mut sim).unwrap();
    let text = "aZ9";
    sim.push_mmio(0xF000_0004, &text.bytes().map(|b| b as u32).collect::<Vec<_>>());
    let r = sim.run().unwrap();
    assert_eq!(r.reason, PauseReason::Exit { code: 0 });
    assert_eq!(sim.text_output(), "AZ9");
}

#[test]
fn program_mode_prints_strings() {
    let src = "
        int main() { print_str(\"Sylas\"); return 0; }
    ";
    let sim = run_with(src, None);
    assert_eq!(sim.text_output(), "Sylas");
}

// ─────────────────────────────────────────────────────────────────────────────
//  错误信息
// ─────────────────────────────────────────────────────────────────────────────

fn err(src: &str) -> String {
    match compile(src) {
        Ok(_) => panic!("本来期望编译失败"),
        Err(e) => format!("{}", e),
    }
}

#[test]
fn syntax_errors_have_line_numbers() {
    let e = err("int solve() {\n  return 1 +\n}\n");
    assert!(e.contains("第 3 行"), "{}", e);
}

#[test]
fn undefined_variable_is_reported_in_chinese() {
    let e = err("int solve(int a) { return b + a; }");
    assert!(e.contains("没有定义过") && e.contains('b'), "{}", e);
}

#[test]
fn undefined_function_is_reported() {
    let e = err("int solve(int a) { return nope(a); }");
    assert!(e.contains("没有定义过函数"), "{}", e);
}

#[test]
fn wrong_argument_count_is_reported() {
    let e = err("int f(int a, int b) { return a + b; }\nint solve(int a) { return f(a); }");
    assert!(e.contains("参数"), "{}", e);
}

#[test]
fn assigning_to_a_non_lvalue_is_reported() {
    let e = err("int solve(int a) { 1 = a; return 0; }");
    assert!(e.contains("左边"), "{}", e);
}

#[test]
fn unknown_struct_member_is_reported() {
    let e = err("struct p { int x; };\nint solve(int a) { struct p q; q.y = 1; return 0; }");
    assert!(e.contains("没有成员") || e.contains('y'), "{}", e);
}

#[test]
fn break_outside_loop_is_reported() {
    let e = err("int solve(int a) { break; return 0; }");
    assert!(e.contains("break"), "{}", e);
}

#[test]
fn too_many_arguments_is_reported() {
    let e = err("int f(int a,int b,int c,int d,int e,int g,int h) { return a; }\nint solve(int a){ return f(a,a,a,a,a,a,a); }");
    assert!(e.contains("参数"), "{}", e);
}

#[test]
fn preprocessor_directives_are_rejected_clearly() {
    let e = err("#include <stdio.h>\nint solve() { return 0; }");
    // `#` 不是合法字符 → 明确的词法错误
    assert!(e.contains("第 1 行"), "{}", e);
}
