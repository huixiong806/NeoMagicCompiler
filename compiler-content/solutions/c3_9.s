# 3-9 参考解：完整表达式求值（+ - * / 、括号、单目负号）
#
# 在 3-8 的三层结构上，只加了一个"负号层"：
#   expression := term (('+' | '-') term)*
#   term       := factor (('*' | '/') factor)*
#   factor     := '-' factor | 数字 | '(' expression ')'
#
# 负号放在最紧的那一层（factor），所以：
#   -3*4   → (-3)*4 = -12
#   2*-3   → 2*(-3) = -6
#   -(1+2) → 先算括号，再取负
#   --3    → 3（负号可以叠）
#
#   expression 层：a2 = 累加器      term 层：a1 = 累加器
#   跨函数参数/返回值：a0

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000

main:
        li      a4, INPUT
        li      a5, DISPLAY

        la      a6, pend_ptr
        la      t0, pend_buf
        sw      t0, 0(a6)

        jal     ra, expression
        jal     ra, print_int

        li      a0, 10
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
        lw      a0, 0(a4)
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
#  factor := '-' factor | 数字串 | '(' expression ')'
#  返回值在 a0
# ─────────────────────────────────────────────────────────────
factor:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a3, 0(sp)

        jal     ra, get_char
        li      t0, '-'
        beq     a0, t0, fac_neg         # 单目负号：先取负，再算后面的因子
        li      t0, '('
        beq     a0, t0, fac_paren

        # 剩下的情况：一串数字（a0 里已经是最前面那个字符）
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
        jal     ra, unget_char
        mv      a0, a3
        j       fac_ret

fac_neg:
        jal     ra, factor              # 负号后面还是一个完整的因子
        sub     a0, x0, a0              # 0 - x = -x
        j       fac_ret

fac_paren:
        jal     ra, expression          # 括号里：重新进最松的那一层
        mv      a3, a0
        jal     ra, get_char
        li      t0, ')'
        beq     a0, t0, fac_paren_ok
        jal     ra, unget_char
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

# ─────────────────────────────────────────────────────────────
        .data
pend_buf:
        .space  16
pend_ptr:
        .word   0
