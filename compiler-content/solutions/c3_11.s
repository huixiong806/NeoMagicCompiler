# 3-11 参考解：小脚本解释器（let / print）
#
# 能跑这样的脚本：
#   let x = 3; print x+1;
#   let a = (2+3)*4; print a; print a/2;
#
# 结构 = 3-9 的表达式求值（表达式那一层一个字都没改） + 语句层：
#
#   program   := statement*                    ← 语句层：一直读到输入结束
#   statement := 'let' 名字 '=' expression      ← 赋值：把值写进变量槽
#              | 'print' expression             ← 打印：把值打出来
#              | expression                     ← 光一个表达式（算了不打印）
#
# 变量表：就放在 0x2000 起的一段内存里，一个字母一个槽（每个 4 字节）。
#   x → 0x2000 + ('x' - 'a') * 4 = 0x2000 + 23*4 = 0x205C
#   「名字 → 地址」这段换算由 name_slot 负责，求值层只认地址。
#
# 分工：语句层用 s0；表达式层照旧用 a1/a2/a3；函数调用参数/返回值用 a0。

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000
        .equ VARS, 0x2000               # 变量表起点

main:
        li      a4, INPUT
        li      a5, DISPLAY

        la      a6, pend_ptr
        la      t0, pend_buf
        sw      t0, 0(a6)

# ── 语句层：一次读一条语句，直到输入结束 ──
stmt_loop:
        jal     ra, skip_filler         # 跳过空白、换行、分号
        beqz    a0, prog_end            # 读到 0 = 输入结束（跳过了说明就是空白）

        li      t0, 'l'
        beq     a0, t0, try_let
        li      t0, 'p'
        beq     a0, t0, try_print
        jal     ra, unget_char          # 都不是：这是个光秃秃的表达式
        jal     ra, expression
        j       stmt_loop

prog_end:
        li      a0, 0
        li      a7, 0
        ecall

# ── let 名字 = 表达式 ──
try_let:
        jal     ra, expect_word         # 吃掉 'e'、't'
        jal     ra, skip_space
        jal     ra, get_char            # 名字的首字母
        mv      s2, a0                  # s2 = 名字（注意：不能用 s0，
                                        #      求值层里的 fac_var 会用到 s0）
        li      t2, 0                   # 先假设名字只有一个字母
let_name_skip:                          # 多字母名字的余下字母在这里吃掉
        jal     ra, get_char
        li      t0, 'a'
        blt     a0, t0, let_name_done
        li      t0, 'z'
        bgt     a0, t0, let_name_done
        li      t2, 1                   # 这个字母也属于名字
        j       let_name_skip
let_name_done:
        # 循环要么停在一个非小写字母上，要么停在 0（输入结束）上：
        # 前者要放回去（那是 '=' 之类），后者不能放（0 是结束标记）
        bnez    a0, let_name_putback
        li      t2, 1                   # 0 也要放回去，统一在下面处理
let_name_putback:
        beqz    t2, let_name_ready
        jal     ra, unget_char
let_name_ready:
        jal     ra, skip_space
        jal     ra, get_char            # 期望 '='
        li      t0, '='
        beq     a0, t0, do_let
        jal     ra, unget_char
do_let:
        jal     ra, skip_space
        jal     ra, expression          # a0 = 右值
        mv      t1, a0
        mv      a0, s2
        jal     ra, name_slot           # a0 = 这个变量的槽地址
        sw      t1, 0(a0)               # 存进去
        j       stmt_loop

# ── print 表达式 ──
try_print:
        # 'p' 已经读掉了，先确认后面是 "rint"
        jal     ra, get_char
        li      t0, 'r'
        bne     a0, t0, print_plain
        jal     ra, expect_word         # 吃掉 'i'、'n'、't'
        j       do_print
print_plain:
        # 不是 "print"：那就是一个以 p 开头的变量名，把它读完整
        # （这里只回退一个字符就够：变量名只有一个字母，剩下的字母由 read_name 跳过）
        jal     ra, unget_char
do_print:
        jal     ra, skip_space
        jal     ra, expression
        jal     ra, print_int
        li      t0, 10                  # 每条 print 结果占一行
        sw      t0, 0(a5)
        j       stmt_loop

# ─────────────────────────────────────────────────────────────
#  语句层的几个小工具
#
#  注意：这几个函数**自己也要调用别的函数**（get_char / unget_char），
#  而 `ra` 只有一个。所以每个函数进来先 `ra` 压栈、返回前再取回来，
#  否则里层的 `jal ra, get_char` 会把外层的返回地址冲掉，
#  函数末尾的 `ret` 就会跳回自己身上，变成死循环。
# ─────────────────────────────────────────────────────────────

# skip_space：跳过空格与制表符（**不跳换行**，换行是语句分隔符）
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

# skip_filler：跳过不影响语句的空隙（空白、换行、分号、回车），
# 返回值在 a0：忽略这些之后看到的第一个字符（0 = 输入结束）
skip_filler:
        addi    sp, sp, -4
        sw      ra, 0(sp)
sf_loop:
        jal     ra, get_char
        beqz    a0, sf_done             # 输入结束：原样返回 0
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
        lw      ra, 0(sp)               # a0 里就是要返回的那个字符
        addi    sp, sp, 4
        ret

# expect_word：确定吃掉紧跟其后的 3 个字符（用于 let / print 剩下的字母）
expect_word:
        addi    sp, sp, -4
        sw      ra, 0(sp)
        jal     ra, get_char
        jal     ra, get_char
        jal     ra, get_char
        lw      ra, 0(sp)
        addi    sp, sp, 4
        ret

# name_slot：把字母（a0）换算成变量槽地址（a0 = VARS + (字母 - 'a') * 4）
name_slot:
        li      t0, 'a'
        sub     a0, a0, t0
        slli    a0, a0, 2               # 每个变量占 4 字节
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
#  表达式求值（与 3-9 相同：加减层 → 乘除层 → 因子层）
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
        blt     a0, t0, fac_digits      # 'a' 之前 → 只能是数字
        li      t0, 'z'
        bgt     a0, t0, fac_digits
        j       fac_var                 # 小写字母 → 变量名

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
        # 名字剩下的字母吃掉，再按首字母取槽地址，读出来当值
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
