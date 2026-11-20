# 4-5 参考解：把一行表达式解析成 AST，并以括号化前缀形式打印出来（Lisp 风格）
#
# 输入：0xF0000004 按字节读字符（读到 0 = 输入结束）；
#       一行只含非负整数、+ - * /、括号、空白。
# 输出：一行文本，例如 1+2*3 -> (+ 1 (* 2 3))， (1+2)*3 -> (* (+ 1 2) 3)
#
# ── 这一关真正的难点：输出的顺序和"看到的顺序"是反的 ──
#
# 4-4 吐的是后缀："看完一棵子树才吐"，所以一边读一边吐完全没问题。
# 这一关要吐前缀：`(+ 左 右)` 里的**加号必须排在左子树前面**。
# 可是从左往右读的时候，`3+5` 的那个加号是**读完 3 之后**才看到的 ——
# 想让加号先出去，就得先知道它在、先知道右子树是什么。一句话：
# **树得先搭好，才能从根往下念。**
#
# （这就是编译器为什么一定要有 AST / 一定要先把源码放进内存：
#   "流式"的写法只能从叶子往根讲，讲不了从根往叶子讲。）
#
# 于是这一关走两遍：
#   第一遍（main）：把整行字符读进内存里的 inbuf，一个不落（空白顺手丢掉）。
#   第二遍（print_expr / print_term / print_atom）：在 inbuf 上"劈树"，边劈边印。
#
# ── 怎么在内存里劈树 ──
#
# 给一片字符 buf[begin, end)，**树顶 = 这片里最右边那个"深度 0"的本档运算符**：
#   - "本档"：print_expr 只找 + -，print_term 只找 * /（三层和 4-4 完全一样）；
#   - "深度 0"：括号里的运算符不算 —— 它属于括号里那棵更小的树；
#   - "最右边"：同级运算符**左结合**，`1-2-3` 是 `(1-2)-3`，树顶正是最右边那个减号。
# 找到它，就把这片劈成左片 [begin, 位置) 和右片 (位置, end)：
#   先吐 `( 运算符 空格`，再递归左片、递归右片，最后补 `)`。
# 本档一个运算符都没有 -> 整片交给下一档；三档都没有 -> 这片是叶子（一个数字，
# 或者一对包着整片的括号），把数字印出来。
#
# 例子：`2+3*4-5`
#   整片里最右边的深度 0 的 +- 是最后那个 '-'  -> 树顶 '-'，左片 "2+3*4"，右片 "5"
#   左片里最右边的深度 0 的 +- 是那个 '+'      -> "2" 和 "3*4"
#   "3*4" 里最右边的深度 0 的 */ 是那个 '*'    -> "3" 和 "4"
#   拼起来：(- (+ 2 (* 3 4)) 5)
#
# 寄存器分工（全程序统一，t 寄存器会被任何辅助函数冲掉，跨 jal 要保的只能放 s 寄存器）：
#   s3 = inbuf 基址
#   s0 = 当前这片字符的起点      s1 = 终点（不含）
#   s2 = 在这片里找到的树顶运算符的位置
#   s4-s8 = find_last_op 自己的扫描下标 / 括号深度 / 暂存的运算符字符
# main 里的 t2 = 输入端口地址，t3 = 缓冲区上限（那一小段没调用任何函数，随便用）。

    .text
main:
    # ── 第一遍：把整行读进 inbuf ──
    la   s3, inbuf
    li   s1, 0                       # s1 = 已经存下的字符数
    li   t2, 0xF0000004
    li   t3, 1024                    # 缓冲区上限（保险丝，正常输入远远用不到）
read_loop:
    lbu  t0, 0(t2)                   # 用 lbu 读一个字节；lw 会一次吃掉 4 个字节
    beq  t0, x0, read_done           # 0 = 输入结束
    li   t1, 32                      # 空白只是分隔符，不往缓冲区里放
    beq  t0, t1, read_loop
    li   t1, 10
    beq  t0, t1, read_loop
    li   t1, 13
    beq  t0, t1, read_loop
    li   t1, 9
    beq  t0, t1, read_loop
    bge  s1, t3, read_done           # 缓冲区满了（正常输入到不了这里）
    add  t1, s3, s1
    sb   t0, 0(t1)
    addi s1, s1, 1
    j    read_loop
read_done:
    # ── 第二遍：从根往下，把这棵树印成一行 ──
    li   s0, 0                       # 整行就是 buf[0, s1)
    jal  ra, print_expr
    li   a0, 10                      # 收尾一个换行（判定按行比对，不影响结果）
    jal  ra, put_ch
    li   a0, 0
    li   a7, 0
    ecall
    li   a7, 0
    ecall

# ───────────────────────── 输出 ─────────────────────────

# put_ch：输出 a0 的低字节（这里是 ASCII 字符）
put_ch:
    li   a7, 1
    ecall
    ret

# put_dec：把 a0 里的非负整数按十进制输出（各位先压栈，再反向弹出）
# 这是个**叶子函数**（不 jal 任何东西），所以不必存 ra。
put_dec:
    mv   t2, a0
    li   t6, 0
put_dec_loop:
    li   t1, 10
    rem  t3, t2, t1
    addi t3, t3, 48
    div  t2, t2, t1
    addi sp, sp, -1
    sb   t3, 0(sp)
    addi t6, t6, 1
    bne  t2, x0, put_dec_loop
put_dec_out:
    lbu  a0, 0(sp)
    addi sp, sp, 1
    li   a7, 1
    ecall
    addi t6, t6, -1
    bne  t6, x0, put_dec_out
    ret

# ───────────────────────── 找树顶 ─────────────────────────

# find_last_op：a0 / a1 = 本档的那一对运算符（如 '+' 和 '-'），s0 / s1 = 这一片 [begin, end)
#   返回 a0 = 最后一个"深度 0"的运算符的位置；一个都没有就返回 s1。
#   （s1 正好当"没找到"的哨兵：任何真实位置都 < s1。）
# 它只用 s4-s8，**不碰 s0/s1/s2**，所以调用者不用为它做额外备份。
find_last_op:
    mv   s7, a0                      # 本档的第一个运算符
    mv   s8, a1                      # 本档的第二个运算符
    mv   s4, s0                      # s4 = 扫描下标
    li   s5, 0                       # s5 = 括号深度
    mv   s6, s1                      # s6 = 目前找到的位置（先当成"没找到"）
fl_loop:
    bge  s4, s1, fl_end              # 扫到终点就收工
    add  t1, s3, s4
    lbu  t0, 0(t1)                   # t0 = buf[s4]
    li   t1, 40                      # '('
    beq  t0, t1, fl_open
    li   t1, 41                      # ')'
    beq  t0, t1, fl_close
    bne  s5, x0, fl_next             # 深度不是 0 -> 括号里的运算符，不是这一片的树顶
    beq  t0, s7, fl_hit
    beq  t0, s8, fl_hit
    j    fl_next
fl_hit:
    mv   s6, s4                      # 记下它；后面再遇到就覆盖 —— 要的就是最后一个
    j    fl_next
fl_open:
    addi s5, s5, 1
    j    fl_next
fl_close:
    addi s5, s5, -1
fl_next:
    addi s4, s4, 1
    j    fl_loop
fl_end:
    mv   a0, s6
    ret

# ───────────────────────── 从根往叶子印（三档） ─────────────────────────
#
# 三档的结构是一模一样的，只有三处不同：
#   1. 找哪一对运算符；2. 左片递归到哪一档；3. 右片交给哪一档。
# 左结合意味着：树顶右边那一片一定是"更低一档"的东西（`a*b/c` 的点在最后一个 '/'，
# 它右边只剩一个叶子），而左边还可能挂着同档的运算符 —— 所以左片递归到本档、右片降一档。
#
# 每个函数都把自己的 s0/s1/s2 存进栈帧再改、返回前还原：
# 三层是递归的，而 s 寄存器在子调用里同样是"会变的"，不存就会互相踩。

# print_expr：加法那一档（+ -）
print_expr:
    addi sp, sp, -16
    sw   ra, 0(sp)
    sw   s0, 4(sp)
    sw   s1, 8(sp)
    sw   s2, 12(sp)
    li   a0, 43                      # '+'
    li   a1, 45                      # '-'
    jal  ra, find_last_op
    beq  a0, s1, pe_leaf             # 这一片里没有 +- -> 整片交给乘法那一档
    mv   s2, a0                      # s2 = 树顶
    li   a0, 40                      # '(' —— 先把根吐出去
    jal  ra, put_ch
    add  t1, s3, s2
    lbu  a0, 0(t1)                   # 树顶的运算符本身
    jal  ra, put_ch
    li   a0, 32                      # ' '
    jal  ra, put_ch
    mv   s1, s2                      # 左片 = [begin, 树顶)
    jal  ra, print_expr              # 左片还是加法这一档（左结合）
    li   a0, 32
    jal  ra, put_ch
    lw   s1, 8(sp)                   # 右片 = (树顶, end]
    addi s0, s2, 1
    jal  ra, print_term              # 右片降一档
    li   a0, 41                      # ')'
    jal  ra, put_ch
    j    pe_ret
pe_leaf:
    jal  ra, print_term
pe_ret:
    lw   s2, 12(sp)
    lw   s1, 8(sp)
    lw   s0, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 16
    ret

# print_term：乘法那一档（* /）
print_term:
    addi sp, sp, -16
    sw   ra, 0(sp)
    sw   s0, 4(sp)
    sw   s1, 8(sp)
    sw   s2, 12(sp)
    li   a0, 42                      # '*'
    li   a1, 47                      # '/'
    jal  ra, find_last_op
    beq  a0, s1, pt_leaf             # 这一片里没有 */ -> 整片交给最底层
    mv   s2, a0
    li   a0, 40                      # '('
    jal  ra, put_ch
    add  t1, s3, s2
    lbu  a0, 0(t1)
    jal  ra, put_ch
    li   a0, 32
    jal  ra, put_ch
    mv   s1, s2
    jal  ra, print_term              # 左片还是乘法这一档
    li   a0, 32
    jal  ra, put_ch
    lw   s1, 8(sp)
    addi s0, s2, 1
    jal  ra, print_atom              # 右片只剩一个最底层的东西
    li   a0, 41                      # ')'
    jal  ra, put_ch
    j    pt_ret
pt_leaf:
    jal  ra, print_atom
pt_ret:
    lw   s2, 12(sp)
    lw   s1, 8(sp)
    lw   s0, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 16
    ret

# print_atom：最底层 —— 一个数字，或者一整对括号（"(" expr ")"）
print_atom:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   s0, 4(sp)
    sw   s1, 8(sp)
    bge  s0, s1, patom_ret           # 空片（整行都是空白）-> 什么都不印
    add  t1, s3, s0
    lbu  t0, 0(t1)
    li   t1, 40                      # '('
    bne  t0, t1, patom_number
    addi s0, s0, 1                   # 剥掉最外面那对括号：
    addi s1, s1, -1                  # '(' 之后、')' 之前，从最高那一档重新开始
    jal  ra, print_expr
    j    patom_ret
patom_number:
    li   t2, 0                       # 十进制累加值
    mv   t4, s0                      # t4 = 读数字的下标
patom_loop:
    bge  t4, s1, patom_out
    add  t1, s3, t4
    lbu  t5, 0(t1)
    li   t1, 10                      # 值 = 值 * 10 + (字符 - '0')
    mul  t2, t2, t1
    addi t5, t5, -48
    add  t2, t2, t5
    addi t4, t4, 1
    j    patom_loop
patom_out:
    mv   a0, t2
    jal  ra, put_dec                 # 叶子就是一个数字：直接印它
patom_ret:
    lw   s1, 8(sp)
    lw   s0, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 12
    ret

    .data
inbuf:
    .space 1024                      # 第一遍把整行读进来，第二遍在它上面劈树
