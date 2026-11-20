# 3-12 综合参考解：咒文解析机
#
# 输入是多行"咒文"，每一行是下面三种之一：
#   表达式               （算完就丢掉，不输出）
#   let 名字 = 表达式     （存进变量表）
#   print 表达式          （算完打一行结果）
#
# 输出：每一条 print 打一行；没有 print 的行不产生输出；空行跳过。
#
#   > 1+2*3
#   > let x = (1+2)*3
#   > print x+1
#   10
#   > print -x/2
#   -4
#
# 结构（就是前几关合起来的一台小机器）：
#   program   := 一行一行读，每行先看第一个词是 let / print / 其它
#   expression := term (('+'|'-') term)*
#   term       := factor (('*'|'/') factor)*
#   factor     := '-' factor | 数字串 | '(' expression ')' | 变量名
#
# 变量表在 0x2000 起，一个字母一个槽（每槽 4 字节）：x → 0x2000 + 23*4

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000
        .equ VARS, 0x2000

main:
        li      a4, INPUT
        li      a5, DISPLAY

        la      a6, pend_ptr
        la      t0, pend_buf
        sw      t0, 0(a6)

# ── 一行一行地处理：空行、分号、空白都会被 skip_filler 吃掉 ──
stmt_loop:
        jal     ra, skip_filler
        beqz    a0, prog_end            # 输入结束
        li      t0, 'l'
        beq     a0, t0, try_let
        li      t0, 'p'
        beq     a0, t0, try_print
        jal     ra, unget_char          # 其它：光一个表达式，算完丢掉
        jal     ra, expression
        j       stmt_loop

prog_end:
        li      a0, 0
        li      a7, 0
        ecall

# ── let 名字 = 表达式 ──
try_let:
        jal     ra, expect_word         # 吃掉 "et"
        jal     ra, skip_space
        jal     ra, read_name           # a0 = 名字首字母
        mv      s2, a0                  # s2 = 名字首字母
                                        # （不能用 s0：求值层里的 fac_var 会用 s0）
        jal     ra, skip_space
        jal     ra, get_char            # 期望 '='
        li      t0, '='
        beq     a0, t0, do_let
        jal     ra, unget_char
do_let:
        jal     ra, skip_space
        jal     ra, expression
        mv      t1, a0
        mv      a0, s2
        jal     ra, name_slot           # a0 = 变量槽地址
        sw      t1, 0(a0)
        j       stmt_loop

# ── print 表达式 ──
try_print:
        jal     ra, get_char            # 'p' 已读掉，看下一个是不是 'r'
        li      t0, 'r'
        bne     a0, t0, print_plain
        jal     ra, expect_word         # 吃掉 "int"
        j       do_print
print_plain:
        # 不是 "print"：那就当它是以 p 开头的变量名（把字符还回去由求值层处理）
        jal     ra, unget_char
do_print:
        jal     ra, skip_space
        jal     ra, expression
        jal     ra, print_int
        li      t0, 10                  # 每条结果一行
        sw      t0, 0(a5)
        j       stmt_loop

# ─────────────────────────────────────────────────────────────
#  语句层的小工具
#
#  注意：这几个函数**自己也要调用别的函数**（get_char / unget_char），
#  而 `ra` 只有一个。所以进来先把 `ra` 压栈、返回前再取回来：
#  否则里层的 `jal ra, get_char` 会冲掉外层的返回地址，
#  末尾的 `ret` 就跳回自己身上，变成死循环。
# ─────────────────────────────────────────────────────────────

skip_space:
        addi    sp, sp, -4
        sw      ra, 0(sp)
ss_loop:
        jal     ra, get_char
        li      t0, ' '
        beq     a0, t0, ss_loop
        li      t0, 9
        beq     a0, t0, ss_loop
        jal     ra, unget_char
        lw      ra, 0(sp)
        addi    sp, sp, 4
        ret

# 跳过空白、换行、回车、分号；返回跳完之后的第一个字符（0 = 输入结束）
skip_filler:
        addi    sp, sp, -4
        sw      ra, 0(sp)
sf_loop:
        jal     ra, get_char
        beqz    a0, sf_done
        li      t0, ' '
        beq     a0, t0, sf_loop
        li      t0, 9
        beq     a0, t0, sf_loop
        li      t0, 10
        beq     a0, t0, sf_loop
        li      t0, 13
        beq     a0, t0, sf_loop
        li      t0, ';'
        beq     a0, t0, sf_loop
sf_done:
        lw      ra, 0(sp)
        addi    sp, sp, 4
        ret

expect_word:
        addi    sp, sp, -4
        sw      ra, 0(sp)
        jal     ra, get_char
        jal     ra, get_char
        jal     ra, get_char
        lw      ra, 0(sp)
        addi    sp, sp, 4
        ret

# 读一个变量名，返回首字母在 a0
read_name:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        jal     ra, get_char
        mv      s0, a0
rn_skip:
        jal     ra, get_char
        li      t0, 'a'
        blt     a0, t0, rn_done
        li      t0, 'z'
        bgt     a0, t0, rn_done
        j       rn_skip
rn_done:
        jal     ra, unget_char
        mv      a0, s0
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# 字母（a0）→ 变量槽地址（a0 = VARS + (字母 - 'a') * 4）
name_slot:
        li      t0, 'a'
        sub     a0, a0, t0
        slli    a0, a0, 2
        li      t0, VARS
        add     a0, a0, t0
        ret

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
#  表达式求值
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

# factor := '-' factor | 数字串 | '(' expression ')' | 变量名
factor:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a3, 0(sp)

        jal     ra, get_char
        li      t0, '-'
        beq     a0, t0, fac_neg
        li      t0, '('
        beq     a0, t0, fac_paren
        li      t0, 'a'
        blt     a0, t0, fac_digits
        li      t0, 'z'
        bgt     a0, t0, fac_digits
        j       fac_var

fac_digits:
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
        jal     ra, factor
        sub     a0, x0, a0
        j       fac_ret

fac_paren:
        jal     ra, expression
        mv      a3, a0
        jal     ra, get_char
        li      t0, ')'
        beq     a0, t0, fac_paren_ok
        jal     ra, unget_char
fac_paren_ok:
        mv      a0, a3
        j       fac_ret

fac_var:
        mv      s0, a0
fac_var_skip:
        jal     ra, get_char
        li      t0, 'a'
        blt     a0, t0, fac_var_done
        li      t0, 'z'
        bgt     a0, t0, fac_var_done
        j       fac_var_skip
fac_var_done:
        jal     ra, unget_char
        mv      a0, s0
        jal     ra, name_slot
        lw      a3, 0(a0)
        mv      a0, a3

fac_ret:
        lw      a3, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  打印整数
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
