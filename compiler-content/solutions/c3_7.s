# 3-7 参考解：表达式求值（只有 + - * /，先乘除后加减）
#
# 结构：两级递归下降
#   expression := term (('+' | '-') term)*      ← 加减这一层
#   term       := factor (('*' | '/') factor)*  ← 乘除这一层（更深一层）
#   factor     := 一串数字
#
# 关键手法：**偷看一个字符（lookahead）**。
#   每个循环先偷看下一个字符，是"自己这一层的运算符"就吃掉继续算，
#   不是就原样留在 lookahead 里返回给上一层 —— 这是递归下降的标准做法。
#
# 寄存器分工（两层各用各的，互不干扰）：
#   expression 层：a2 = 累加器，a3 = 乘除层交回来的值
#   term       层：a1 = 累加器，a3 = 数字层交回来的值
#   跨函数参数/返回值：a0

        .text
        .equ INPUT, 0xF0000004
        .equ DISPLAY, 0xF0000000

# ─────────────────────────────────────────────────────────────
#  main：读一个字符流，把它当成算式算出来，输出结果
# ─────────────────────────────────────────────────────────────
main:
        li      a4, INPUT               # a4 = 输入端口地址（全局都从它读）
        li      a5, DISPLAY             # a5 = 显示端口地址

        la      a6, pend_ptr            # 放回缓冲区：一开始是空的
        la      t0, pend_buf
        sw      t0, 0(a6)

        jal     ra, expression          # a0 = 整个算式的值
        jal     ra, print_int           # 十进制输出 a0

        li      a0, 10                  # 换行
        li      a7, 1
        ecall

        li      a0, 0                   # 正常结束
        li      a7, 0
        ecall

# ─────────────────────────────────────────────────────────────
#  字符输入：getchar / ungetchar
#  没放回字符时从输入端口读；读到 0 表示输入结束。
# ─────────────────────────────────────────────────────────────
get_char:
        lw      t0, 0(a6)               # t0 = 放回缓冲区当前指针
        la      t1, pend_buf
        beq     t0, t1, get_char_read   # 缓冲区空 → 真的去读输入
        lbu     a0, 0(t0)               # 取出放回的那个字符
        addi    t0, t0, 1               # 指针后移（这个字符被吃掉了）
        sw      t0, 0(a6)
        ret

get_char_read:
        lw      a0, 0(a4)               # a0 = 输入端口的下一个字符（0 = 结束）
        ret

unget_char:
        # 把 a0 里的字符放回缓冲区，让下一次 get_char 再读到它
        lw      t0, 0(a6)
        addi    t0, t0, -1
        sb      a0, 0(t0)
        sw      t0, 0(a6)
        ret

# ─────────────────────────────────────────────────────────────
#  expression := term (('+' | '-') term)*
#  返回值在 a0
# ─────────────────────────────────────────────────────────────
expression:
        addi    sp, sp, -8              # 本层要用 a2/a3 做临时，先存起来
        sw      ra, 4(sp)
        sw      a2, 0(sp)

        jal     ra, term                # a0 = 第一个"项"
        mv      a2, a0                  # a2 = 累计和

exp_loop:
        jal     ra, get_char            # 偷看下一个字符
        li      t0, '+'
        beq     a0, t0, exp_add
        li      t0, '-'
        beq     a0, t0, exp_sub
        # 不是加减号：原样放回去，交给上面的层（或者被丢弃）
        jal     ra, unget_char
        j       exp_done

exp_add:
        jal     ra, term                # a0 = 下一个"项"
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
#  返回值在 a0
# ─────────────────────────────────────────────────────────────
term:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a1, 0(sp)

        jal     ra, factor              # a0 = 第一个"因子"
        mv      a1, a0                  # a1 = 累计积

term_loop:
        jal     ra, get_char
        li      t0, '*'
        beq     a0, t0, term_mul
        li      t0, '/'
        beq     a0, t0, term_div
        jal     ra, unget_char          # 不是乘除号 → 还回去，返回
        j       term_done

term_mul:
        jal     ra, factor
        mul     a1, a1, a0
        j       term_loop

term_div:
        jal     ra, factor
        div     a1, a1, a0              # 有符号除法（除数为 0 的算式不会出现）
        j       term_loop

term_done:
        mv      a0, a1
        lw      a1, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  factor := 一串十进制数字
#  返回值在 a0
# ─────────────────────────────────────────────────────────────
factor:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a3, 0(sp)

        li      a3, 0                   # a3 = 正在拼的数

fac_loop:
        jal     ra, get_char
        li      t0, '0'
        blt     a0, t0, fac_end         # 比 '0' 小 → 不是数字
        li      t0, '9'
        bgt     a0, t0, fac_end         # 比 '9' 大 → 不是数字
        addi    a0, a0, -48             # '0'(48) → 0，'9'(57) → 9
        li      t0, 10
        mul     a3, a3, t0              # 值 = 值 * 10 + 这一位
        add     a3, a3, a0
        j       fac_loop

fac_end:
        jal     ra, unget_char          # 第一个非数字字符还回输入
        mv      a0, a3
        lw      a3, 0(sp)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────
#  print_int：把 a0 按十进制输出（带负号）
# ─────────────────────────────────────────────────────────────
print_int:
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a0, 0(sp)

        bgez    a0, pi_pos              # 非负 → 直接打
        li      t0, '-'
        sw      t0, 0(a5)               # 写显示端口就是输出一个字符
        lw      a0, 0(sp)
        sub     a0, x0, a0              # 取相反数，绝对值 = 0 - x
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

# 只输出非负整数（a0 >= 0）：从最高位开始，靠递归来"倒着打"
print_uint:
        li      t0, 10
        blt     a0, t0, pu_one          # 只剩一位 → 直接打
        addi    sp, sp, -8
        sw      ra, 4(sp)
        sw      a0, 0(sp)
        div     a0, a0, t0              # a0 = 高位部分
        jal     ra, print_uint          # 先把高位打完
        lw      a0, 0(sp)
        li      t0, 10
        rem     a0, a0, t0              # a0 = 最低位数字
        addi    a0, a0, 48              # 数字 → 字符
        sw      a0, 0(a5)
        lw      ra, 4(sp)
        addi    sp, sp, 8
        ret

pu_one:
        addi    a0, a0, 48              # 数字 → 字符（'0' = 48）
        sw      a0, 0(a5)
        ret

# ─────────────────────────────────────────────────────────────
#  数据：放回字符用的小缓冲区（先放回的先读到）
# ─────────────────────────────────────────────────────────────
        .data
pend_buf:
        .space  16
pend_ptr:
        .word   0
