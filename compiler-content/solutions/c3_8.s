# 3-8 参考解：带括号的表达式求值（递归下降，"一层套一层"）
#
# 三层结构，一层管一种"结合方式"：
#   expression := term (('+' | '-') term)*      ← 加减层：最松
#   term       := factor (('*' | '/') factor)*  ← 乘除层：更紧
#   factor     := 一串数字 | '(' expression ')' ← 因子层：最紧
#
# 括号为什么能自动配对？因为 factor 看到 '(' 时**调用了 expression 本身**：
#   外面那一层的进度被留在寄存器/栈里，引擎重新进一次加减层，
#   等它遇到 ')' 就返回一个数 —— 那就是括号里的结果。
#   括号套括号，只是同一个函数又进了一层（栈又长了一截）。
#
#   expression 层：a2 = 累加器      term 层：a1 = 累加器
#   跨函数参数/返回值：a0

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000

main:
        li      a4, INPUT               # a4 = 输入端口地址
        li      a5, DISPLAY             # a5 = 显示端口地址

        la      a6, pend_ptr            # 放回缓冲区指针（一开始是空的）
        la      t0, pend_buf
        sw      t0, 0(a6)

        jal     ra, expression          # a0 = 整个算式的值（最外面那层）
        jal     ra, print_int

        li      a0, 10                  # 换行
        li      a7, 1
        ecall

        li      a0, 0
        li      a7, 0
        ecall

# ─────────────────────────────────────────────────────────────
#  字符输入
# ─────────────────────────────────────────────────────────────
get_char:
        lw      t0, 0(a6)
        la      t1, pend_buf
        beq     t0, t1, get_char_read
        lbu     a0, 0(t0)
        addi    t0, t0, 1
        sw      t0, 0(a6)
        ret

get_char_read:
        lw      a0, 0(a4)               # 读到 0 = 输入结束
        ret

unget_char:
        lw      t0, 0(a6)
        addi    t0, t0, -1
        sb      a0, 0(t0)
        sw      t0, 0(a6)
        ret

# ─────────────────────────────────────────────────────────────
#  expression := term (('+' | '-') term)*
# ─────────────────────────────────────────────────────────────
expression:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a2, 0(sp)

        jal     ra, term
        mv      a2, a0

exp_loop:
        jal     ra, get_char
        li      t0, '+'
        beq     a0, t0, exp_add
        li      t0, '-'
        beq     a0, t0, exp_sub
        # 不是加减号：原样放回（可能是 ')'，也可能是算式的结尾）
        jal     ra, unget_char
        j       exp_done

exp_add:
        jal     ra, term
        add     a2, a2, a0
        j       exp_loop

exp_sub:
        jal     ra, term
        sub     a2, a2, a0
        j       exp_loop

exp_done:
        mv      a0, a2
        lw      a2, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  term := factor (('*' | '/') factor)*
# ─────────────────────────────────────────────────────────────
term:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a1, 0(sp)

        jal     ra, factor
        mv      a1, a0

term_loop:
        jal     ra, get_char
        li      t0, '*'
        beq     a0, t0, term_mul
        li      t0, '/'
        beq     a0, t0, term_div
        jal     ra, unget_char
        j       term_done

term_mul:
        jal     ra, factor
        mul     a1, a1, a0
        j       term_loop

term_div:
        jal     ra, factor
        div     a1, a1, a0
        j       term_loop

term_done:
        mv      a0, a1
        lw      a1, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  factor := 一串数字 | '(' expression ')'
# ─────────────────────────────────────────────────────────────
factor:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a3, 0(sp)

        jal     ra, get_char
        li      t0, '('
        beq     a0, t0, fac_paren       # 看到左括号 → 又进一层加减层

        # 否则：这必须是一串数字
        li      a3, 0
fac_loop:
        li      t0, '0'
        blt     a0, t0, fac_end
        li      t0, '9'
        bgt     a0, t0, fac_end
        addi    a0, a0, -48
        li      t0, 10
        mul     a3, a3, t0
        add     a3, a3, a0
        jal     ra, get_char
        j       fac_loop

fac_end:
        jal     ra, unget_char          # 第一个非数字字符还回去
        mv      a0, a3
        j       fac_ret

fac_paren:
        # 左边那个 '(' 已经吃掉了，现在重新进一次最松的那一层
        jal     ra, expression          # a0 = 括号里的值
        mv      a3, a0
        jal     ra, get_char
        li      t0, ')'
        beq     a0, t0, fac_paren_ok    # 收掉右括号
        jal     ra, unget_char          # 没右括号也不崩，把字符还回去
fac_paren_ok:
        mv      a0, a3

fac_ret:
        lw      a3, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  print_int / print_uint：十进制输出
# ─────────────────────────────────────────────────────────────
print_int:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a0, 0(sp)
        bgez    a0, pi_pos
        li      t0, '-'
        sw      t0, 0(a5)
        lw      a0, 0(sp)
        sub     a0, x0, a0
        jal     ra, print_uint
        j       pi_done
pi_pos:
        lw      a0, 0(sp)
        jal     ra, print_uint
pi_done:
        lw      a0, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

print_uint:
        li      t0, 10
        blt     a0, t0, pu_one
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a0, 0(sp)
        div     a0, a0, t0
        jal     ra, print_uint          # 先打高位
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

# ─────────────────────────────────────────────────────────────
        .data
pend_buf:
        .space  16
pend_ptr:
        .word   0
