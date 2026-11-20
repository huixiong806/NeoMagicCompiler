# 3-10 参考解：定点输出（输入是"×100 之后"的整数，输出要插上小数点）
#
# 输入 1234 → 输出 12.34        输入 300 → 输出 3.00
# 输入 5    → 输出 0.05         输入 -5  → 输出 -0.05
#
# 算法：
#   1. 取绝对值，把符号单独记住；负号先打出去
#   2. 整数部分 = |v| / 100，小数部分 = |v| % 100
#   3. 打整数部分；再打 '.'；最后打两位小数（**不足两位要补 0**：5 要打成 05）
#
# 三个坑（也是这关真正要学的）：
#   - 小数部分必须补零：0.05 不能输出成 0.5
#   - 小数部分必须**恰好两位**：3.00 的两个 0 都要打
#   - 负数：负号打在整数部分前面，小数部分仍然打绝对值

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000

main:
        li      a4, INPUT
        li      a5, DISPLAY

        jal     ra, read_int            # a0 = 输入的那个整数

        mv      s0, a0                  # s0 = 原值（留着算符号）
        bgez    s0, mi_pos
        li      t0, '-'
        sw      t0, 0(a5)               # 负号
        sub     s0, x0, s0              # s0 = |值|
mi_pos:
        li      t1, 100
        div     s1, s0, t1              # s1 = 整数部分
        rem     s2, s0, t1              # s2 = 小数部分（0..99）

        mv      a0, s1
        jal     ra, print_uint          # 打整数部分

        li      t0, '.'
        sw      t0, 0(a5)               # 小数点

        mv      a0, s2
        jal     ra, print_two_digits    # 打两位小数（不足两位补 0）

        li      a0, 10
        li      a7, 1
        ecall

        li      a0, 0
        li      a7, 0
        ecall

# ─────────────────────────────────────────────────────────────
#  read_int：把输入里的十进制整数读进 a0（读到 0 = 输入结束）
# ─────────────────────────────────────────────────────────────
read_int:
        li      a3, 0                   # a3 = 累加器
        li      t2, 1                   # t2 = 符号：1 正 / -1 负
ri_skip:
        lw      t0, 0(a4)               # 跳过前面的空格、换行
        li      t1, ' '
        beq     t0, t1, ri_skip
        li      t1, 10
        beq     t0, t1, ri_skip
        li      t1, 13
        beq     t0, t1, ri_skip
        li      t1, 9
        beq     t0, t1, ri_skip
        li      t1, '-'
        bne     t0, t1, ri_loop
        li      t2, -1
        lw      t0, 0(a4)               # 负号后面接着读第一位数字
ri_loop:
        li      t1, '0'
        blt     t0, t1, ri_done
        li      t1, '9'
        bgt     t0, t1, ri_done
        addi    t0, t0, -48
        li      t1, 10
        mul     a3, a3, t1
        add     a3, a3, t0
        lw      t0, 0(a4)
        j       ri_loop
ri_done:
        mul     a0, a3, t2
        ret

# ─────────────────────────────────────────────────────────────
#  print_two_digits：把 a0（0..99）打成两位，不足两位补 '0'
# ─────────────────────────────────────────────────────────────
print_two_digits:
        li      t0, 10
        blt     a0, t0, ptd_lead0       # 小于 10 → 先补一个 0
        div     t1, a0, t0              # 十位
        rem     t2, a0, t0              # 个位
        addi    t1, t1, 48
        sw      t1, 0(a5)
        addi    t2, t2, 48
        sw      t2, 0(a5)
        ret
ptd_lead0:
        li      t0, '0'
        sw      t0, 0(a5)
        addi    a0, a0, 48
        sw      a0, 0(a5)
        ret

# ─────────────────────────────────────────────────────────────
#  print_uint：输出非负整数 a0
# ─────────────────────────────────────────────────────────────
print_uint:
        li      t0, 10
        blt     a0, t0, pu_one
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a0, 0(sp)
        div     a0, a0, t0
        jal     ra, print_uint
        lw      a0, 0(sp)
        li      t0, 10
        rem     a0, a0, t0
        addi    a0, a0, 48
        sw      a0, 0(a5)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret
pu_one:
        addi    a0, a0, 48
        sw      a0, 0(a5)
        ret
