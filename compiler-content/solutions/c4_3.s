# 4-3 参考解：把中缀表达式转成后缀（RPN），一行一个 token
#
# 输入：只有数字和 + - * / ( ) 的表达式，从 0xF0000004 逐字符读（读到 0 结束）
# 输出：一行一个 token —— 数字印 `NUM:值`，运算符印 `OP:符号`，括号**不输出**
#   例：1+2*3 → NUM:1 / NUM:2 / NUM:3 / OP:* / OP:+
#
# 这一关用的是调度场算法（shunting yard）：
#   数字      → 立刻输出（数字永远不用排队）
#   运算符    → 先把栈顶那些"优先级不低于自己"的运算符弹出来输出，再把自己压进栈
#   '('       → 直接压栈（它不输出，它只是一道挡板）
#   ')'       → 一直弹出并输出，直到弹到 '(' 为止（'(' 被丢掉，不输出）
#   输入结束  → 把栈里剩下的全弹出来输出
#
# 两个必须小心的地方：
#   1) 栈顶是 '(' 的时候要当成"挡住"——不能跨过它去比优先级，否则括号就白写了；
#   2) 运算符栈是往**下**长的字节栈：s3 从 OSTK_TOP 开始减，s4 记着里面有几个东西。
#      它和 sp 各管各的 —— sp 用来存返回地址，两个栈别混着用。

    .equ INPORT,   0xF0000004
    .equ DISPLAY,  0xF0000000
    .equ OSTK_TOP, 0x1800      # 运算符栈的顶：往**下**长，s3 从 0x1800 开始减
    .equ C_TAB,    9
    .equ C_NL,     10
    .equ C_SPACE,  32
    .equ C_ZERO,   48
    .equ C_NINE,   57
    .equ C_LPAREN, 40          # (
    .equ C_RPAREN, 41          # )
    .equ C_PLUS,   43          # +
    .equ C_MINUS,  45          # -
    .equ C_STAR,   42          # *
    .equ C_SLASH,  47          # /

    .text
main:
    li   sp, 0x3000            # 返回地址用的栈
    li   s3, OSTK_TOP          # 运算符栈的栈顶指针
    li   s4, 0                 # 运算符栈里现在有几个东西
    li   t0, INPORT
    la   t1, inport
    sw   t0, 0(t1)

loop:
    jal  ra, getc
    beq  a0, x0, flush         # 0 = 输入结束
    li   t2, C_SPACE
    beq  a0, t2, loop
    li   t2, C_TAB
    beq  a0, t2, loop
    li   t2, C_NL
    beq  a0, t2, loop
    li   t2, C_LPAREN
    beq  a0, t2, do_lparen
    li   t2, C_RPAREN
    beq  a0, t2, do_rparen
    li   t2, C_PLUS
    beq  a0, t2, do_op
    li   t2, C_MINUS
    beq  a0, t2, do_op
    li   t2, C_STAR
    beq  a0, t2, do_op
    li   t2, C_SLASH
    beq  a0, t2, do_op
    li   t2, C_ZERO            # 只剩数字了
    blt  a0, t2, loop
    li   t2, C_NINE
    bgt  a0, t2, loop
    jal  ra, read_number
    j    loop

# ── '('：压栈，不输出（它只是个挡板）──
do_lparen:
    addi s3, s3, -1
    li   t0, C_LPAREN
    sb   t0, 0(s3)
    addi s4, s4, 1
    j    loop

# ── ')'：一直弹出并输出，直到弹到 '(' ──
do_rparen:
dr_loop:
    beq  s4, x0, loop          # 栈空（括号不配对的输入）：收手
    lbu  t1, 0(s3)
    addi s3, s3, 1
    addi s4, s4, -1
    li   t2, C_LPAREN
    beq  t1, t2, loop          # 撞到挡板：把 '(' 丢掉，继续读输入
    addi a0, t1, 0
    jal  ra, emit_op
    j    dr_loop

# ── 二元运算符：先弹掉栈顶"优先级不低于自己"的，再压栈 ──
do_op:
    addi s2, a0, 0             # s2 = 这个运算符本身
    jal  ra, prec              # x11 = 它的优先级
    addi s1, x11, 0
op_loop:
    beq  s4, x0, op_push       # 栈空了，没人能比
    lbu  t1, 0(s3)
    li   t2, C_LPAREN
    beq  t1, t2, op_push       # '(' 挡着，谁也不能越过去
    addi s6, t1, 0
    addi a0, t1, 0
    jal  ra, prec
    blt  x11, s1, op_push      # 栈顶优先级更低 → 停手
    addi s3, s3, 1
    addi s4, s4, -1
    addi a0, s6, 0
    jal  ra, emit_op
    j    op_loop
op_push:
    addi s3, s3, -1
    sb   s2, 0(s3)
    addi s4, s4, 1
    j    loop

# ── 输入结束：把栈里剩下的全弹出来（括号不输出）──
flush:
    beq  s4, x0, done
    lbu  t1, 0(s3)
    addi s3, s3, 1
    addi s4, s4, -1
    li   t2, C_LPAREN
    beq  t1, t2, flush         # 括号不配对时留下的 '('：丢掉就是了
    addi a0, t1, 0
    jal  ra, emit_op
    j    flush

done:
    li   a0, 0
    li   a7, 0
    ecall

# ═══ 子程序：读一个多位数 → 输出 `NUM:值` ═══
# 进来时首字符（一个数字）已经被 getc 吃掉，放在 a0 里
read_number:
    addi sp, sp, -16
    sw   ra, 0(sp)
    sw   s7, 4(sp)             # s7 = 累计值（值本身不参与"是不是数字"的判断，所以它安全）
    addi s7, x0, 0
rn_loop:
    addi a0, a0, -48           # 数字字符 → 数字
    add  s7, s7, a0            # 值 = 值 * 10 + 这一位
    jal  ra, getc              # 再看下一个字符
    beq  a0, x0, rn_end
    li   t2, C_ZERO
    blt  a0, t2, rn_end
    li   t2, C_NINE
    bgt  a0, t2, rn_end
    li   t2, 10
    mul  s7, s7, t2
    j    rn_loop
rn_end:
    jal  ra, ungetc            # 让它停下来的那个字符不属于这个数，还回输入
    la   t1, s_num             # 先印 "NUM"
    addi t3, x0, 3             # 还剩 3 个字符（注意：putch 会动 t0/t1，计数只能用 t3）
rn_pre:
    beq  t3, x0, rn_colon
    lbu  a0, 0(t1)
    addi t1, t1, 1
    addi t3, t3, -1
    jal  ra, putch
    j    rn_pre
rn_colon:
    li   t0, DISPLAY
    li   t1, 58                # ':'
    sb   t1, 0(t0)
    addi a0, s7, 0             # 把数值装进 a0，交给下面印十进制
    jal  ra, print_int
    jal  ra, newline
    lw   ra, 0(sp)
    lw   s7, 4(sp)
    addi sp, sp, 16
    jalr x0, 0(ra)

# ═══ 子程序：输出 `OP:x` 加换行 ═══
emit_op:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    la   t1, s_op
    addi t3, x0, 2             # "OP" 两个字符（计数只能用 t3，putch 会动 t0/t1）
eo_pre:
    beq  t3, x0, eo_colon
    lbu  a0, 0(t1)
    addi t1, t1, 1
    addi t3, t3, -1
    jal  ra, putch
    j    eo_pre
eo_colon:
    li   t0, DISPLAY
    li   t1, 58                # ':'
    sb   t1, 0(t0)
    lw   t2, 4(sp)
    sb   t2, 0(t0)
    jal  ra, newline
    lw   ra, 0(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：运算符优先级：'*' '/' 是 2，'+' '-' 是 1，其他 0 ═══
prec:
    li   t0, C_STAR
    beq  a0, t0, prec2
    li   t0, C_SLASH
    beq  a0, t0, prec2
    li   t0, C_PLUS
    beq  a0, t0, prec1
    li   t0, C_MINUS
    beq  a0, t0, prec1
    addi x11, x0, 0
    jalr x0, 0(ra)
prec2:
    addi x11, x0, 2
    jalr x0, 0(ra)
prec1:
    addi x11, x0, 1
    jalr x0, 0(ra)

# ═══ 子程序：输出一个字符（a0）═══
putch:
    li   t0, DISPLAY
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# ═══ 子程序：输出一个换行 ═══
newline:
    li   t0, DISPLAY
    li   t1, C_NL
    sb   t1, 0(t0)
    jalr x0, 0(ra)

# ═══ 子程序：把 a0 里的非负整数按十进制印出来 ═══
print_int:
    li   t0, 10
    blt  a0, t0, pi_last
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    div  a0, a0, t0
    jal  ra, print_int         # 先把高位印完
    li   t0, 10
    lw   a0, 4(sp)
    rem  a0, a0, t0            # 再印最后一位
    lw   ra, 0(sp)
    addi sp, sp, 8
pi_last:
    addi a0, a0, 48
    li   t0, DISPLAY
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# ═══ 子程序：getc / ungetc —— 一个字符的回看缓冲 ═══
getc:
    la   t0, lookahead
    lbu  t1, 0(t0)
    beq  t1, x0, getc_fetch
    sb   x0, 0(t0)
    addi a0, t1, 0
    jalr x0, 0(ra)
getc_fetch:
    la   t0, inport
    lw   t0, 0(t0)
    lbu  a0, 0(t0)
    jalr x0, 0(ra)

ungetc:
    la   t0, lookahead
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# ── 数据 ──
    .data
s_num:
    .asciz "NUM"
s_op:
    .asciz "OP"
    .align 4                   # 下面的东西要用 lw 读，必须 4 字节对齐
inport:
    .word 0
    .align 4
lookahead:
    .space 4
