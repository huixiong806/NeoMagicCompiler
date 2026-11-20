# 4-6 参考解：极简程序的语义检查（重复声明 / 未声明就使用）
#
# 输入：0xF0000004 按字节读字符（读到 0 = 输入结束），一段只有两种语句的程序：
#         int 名字;      ← 声明
#         print 名字;    ← 使用
#       变量名限定单个小写字母 a-z；空格、\n、\r 当语句分隔符跳过。
# 输出：按遇到的顺序，一行一个错误码：
#         int 名字;   这个名字已经声明过     -> ERR:dup:名字
#         print 名字; 这个名字没有声明过     -> ERR:undecl:名字
#       检查通过的语句什么也不输出；整段干净时**一行都不输出**。
#
# 结构：没有嵌套，就是**一条直线 + 一个账本**。
#   账本（符号表）= 0x3000 起的 26 个字节，第 (字母 - 'a') 格非 0 表示"已声明"。
#   主循环每转一圈处理一条语句：读关键字 → 读名字 → 吃掉 ';' → 查账本/记账本。
#
# 寄存器分工：
#   s0 = 输入端口 0xF0000004      s1 = 回退槽（0 = 空）      s2 = 符号表基址 0x3000
#   s3 = 打印时的串指针           s4 = 名字暂存（报错时要印的那个字母）
#   t0 = 当前字符   t1 = 临时   t2 = 名字序号（字母 - 'a'）   t3/t4 = 打印用
#
# ⚠ 这里有一个非常隐蔽的坑，两个分支都踩了：**名字在 t0 里，而吃掉 ';' 的那次 next_char
#   会把 t0 冲成 ';'**。报错时再去印 t0，印出来的就是分号（ERR:undecl:;），
#   而账本查得完全正确 —— 错只错在"印谁"。
#   所以名字必须在吃掉 ';' **之前**先搬到一个不会被冲掉的寄存器（这里是 s4）。

    .text
main:
    # 账本清零：0x3000 起 26 个字节
    li   s2, 0x3000
    li   t0, 26
    li   t1, 0
zero_loop:
    sb   t1, 0(s2)
    addi s2, s2, 1
    addi t0, t0, -1
    bne  t0, x0, zero_loop
    li   s2, 0x3000

    li   s0, 0xF0000004
    li   s1, 0
    # 循环：每条语句一圈
scan_loop:
    jal  ra, next_char               # 读一个非空白字符
    beq  t0, x0, scan_done           # 读到 0 = 输入结束，收工
    li   t1, 105                     # 'i' -> int
    beq  t0, t1, stmt_int
    li   t1, 112                     # 'p' -> print
    beq  t0, t1, stmt_print
    j    scan_loop                   # 不认识的东西直接跳过（本关不会出现）

# ── 处理 "int 名字;" ──
stmt_int:
    jal  ra, next_char               # 吃掉 'n'
    jal  ra, next_char               # 吃掉 't'
    jal  ra, next_char               # 变量名字母
    mv   s4, t0                      # ★ 先存住名字：下面吃掉 ';' 会把 t0 冲成 ';'
    addi t2, t0, -97                 # 'a' = 97 -> 0..25
    jal  ra, next_char               # 吃掉 ';'
    add  t1, s2, t2
    lbu  t1, 0(t1)
    beq  t1, x0, int_declare         # 账本上这格还空着 -> 登记
    # 已经声明过 -> ERR:dup:<名字>
    la   a0, s_dup
    jal  ra, put_s
    mv   a0, s4                      # 名字从 s4 里取（t0 现在是 ';'）
    jal  ra, put_ch
    jal  ra, put_nl
    j    scan_loop
int_declare:
    add  t1, s2, t2
    li   t3, 1
    sb   t3, 0(t1)                   # 登记（第二次 int a; 虽然报错，a 仍然是"已声明"的）
    j    scan_loop

# ── 处理 "print 名字;" ──
stmt_print:
    jal  ra, next_char               # r
    jal  ra, next_char               # i
    jal  ra, next_char               # n
    jal  ra, next_char               # t
    jal  ra, next_char               # 变量名字母
    mv   s4, t0                      # ★ 同上：先把名字搬到 s4
    addi t2, t0, -97
    jal  ra, next_char               # 吃掉 ';'
    add  t1, s2, t2
    lbu  t1, 0(t1)
    bne  t1, x0, scan_loop           # 声明过 -> 合法，什么也不输出
    # 没声明过 -> ERR:undecl:<名字>
    la   a0, s_undecl
    jal  ra, put_s
    mv   a0, s4                      # 名字从 s4 里取（t0 现在是 ';'）
    jal  ra, put_ch
    jal  ra, put_nl
    j    scan_loop

scan_done:
    li   a0, 0
    li   a7, 0
    ecall
    li   a7, 0
    ecall

# ───────────────────────── 字符输入 ─────────────────────────

# next_char：取下一个非空白字符到 t0（回退槽优先）；0 = 输入结束
#   空白（空格、\n、\r、\t）直接跳过 —— 它们只是语句之间的分隔符
next_char:
next_char_loop:
    beq  s1, x0, next_char_port
    mv   t0, s1
    li   s1, 0
    j    next_char_check
next_char_port:
    lbu  t0, 0(s0)                   # 用 lbu 读一个字节；lw 会一次吃掉 4 个字节
next_char_check:
    beq  t0, x0, next_char_done
    li   t1, 32
    beq  t0, t1, next_char_loop
    li   t1, 10
    beq  t0, t1, next_char_loop
    li   t1, 13
    beq  t0, t1, next_char_loop
    li   t1, 9
    beq  t0, t1, next_char_loop
next_char_done:
    ret

# ───────────────────────── 输出 ─────────────────────────

# put_s：a0 = 以长度字节开头的字符串地址，按长度输出
put_s:
    lbu  t3, 0(a0)                   # t3 = 长度
    addi s3, a0, 1                   # s3 = 内容起点
put_s_loop:
    beq  t3, x0, put_s_end
    lbu  a0, 0(s3)
    li   a7, 1
    ecall
    addi s3, s3, 1
    addi t3, t3, -1
    j    put_s_loop
put_s_end:
    ret

# put_ch：输出 a0 的低字节
put_ch:
    li   a7, 1
    ecall
    ret

# put_nl：输出一个 '\n'
put_nl:
    li   a0, 10
    li   a7, 1
    ecall
    ret

    .data
s_dup:
    .byte 8
    .ascii "ERR:dup:"
    .byte 0
s_undecl:
    .byte 11
    .ascii "ERR:undecl:"
    .byte 0
