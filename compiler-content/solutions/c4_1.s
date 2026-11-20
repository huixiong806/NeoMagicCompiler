# 4-1 参考解：把星咒源码切成 token，一行一个 `KIND:VALUE`
#
# 约定（关卡页里讲过）：
#   输入：星咒源码文本，从 MMIO 输入端口 0xF0000004 逐字符读；读到 0 就是输入结束
#   输出：显示端口 0xF0000000，一行一个 token：`KW:if` / `ID:x` / `NUM:42` / `OP:+`
#   空白（空格、制表、换行）直接跳过，不产生 token
#   遇到不认识的字符：输出 `ERR:字符`，然后结束
#
# 三个关键决定：
#   1) 读字符用 lbu（读一个字节）。本引擎里 lbu / lw 实测**都只消费一个字符**，
#      所以这不影响正确性 —— 但 lbu 表达的是"要一个字节"的意思，语义对得上；
#   2) 回看缓冲：标识符到哪里结束、`==` 是不是两个字符，都要多看一个字符才能决定，
#      那个字符不属于当前 token，必须能还回去（放在固定地址 0x2000）；
#   3) 地址一律用常量（下面的 A_xxx），数据用 .org 钉死 —— 一眼能看出读的是哪一块。
#
# 最要紧的一条纪律：**跨 jal 还活着的值只能放 s0–s11**（t0–t6 / a0–a7 会被被调函数冲掉）。
# 数字累计值 t6 只在 num_loop 内部连着用、不跨调用，所以放 t 寄存器也可以；
# 而"当前字符 / 运算符槽"这类跨 next_char 的东西一律走 s 寄存器或纯常量地址。
#
# 还有一条同样要命：**一个 t 寄存器不能同时当两个活变量的家**。
# 这里踩过：关键字表下标 t4 和单字符运算符表的倒计数 t4 撞车 —— 扫完一次运算符，
# 下一次进来比关键字就从一个乱了的数开始，整关卡进死循环（自检报"超过指令上限"）。
# 现在运算符表的倒计数用 t6，t4 只归 word_cmp 用。

    .equ INPORT,   0xF0000004  # 输入端口
    .equ DISPLAY,  0xF0000000  # 显示端口
    .equ A_LK,     0x2000      # 回看缓冲（1 字节，0 = 空）
    .equ A_BUF,    0x2100      # 一个词（关键字/标识符）的缓冲区
    .equ A_OSLOT,  0x2200      # 当前运算符（最多两个字 + 0）
    .equ A_DIGITS, 0x2320      # print_int 生成数字用的缓冲末尾（从末尾往前写）
    .equ A_KWTAB,  0x1040      # 6 个关键字字符串的地址
    .equ A_OONE,   0x1060      # 单字符运算符表
    # 字符串常量（与文件末尾 .org 一一对应）
    .equ S_KW,     0x1000      # "KW"
    .equ S_ID,     0x1004      # "ID"
    .equ S_NUM,    0x1008      # "NUM"
    .equ S_OP,     0x100C      # "OP"
    .equ S_ERR,    0x1010      # "ERR"
    .equ S_COLON,  0x1014      # ":"
    .equ K0,       0x1018      # "if"
    .equ K1,       0x101C      # "else"
    .equ K2,       0x1024      # "while"
    .equ K3,       0x102C      # "return"
    .equ K4,       0x1034      # "int"
    .equ K5,       0x1038      # "void"
    .equ C_NL,     10
    .equ C_SPACE,  32
    .equ C_COLON,  58
    .equ C_ZERO,   48
    .equ C_NINE,   57
    .equ C_UP_A,   65
    .equ C_UP_Z,   90
    .equ C_LO_A,   97
    .equ C_LO_Z,   122

    .text
main:
    li   sp, 0x3000            # 自己的栈
    li   s9, 0                 # s9 = 当前的"下一个字符"（0 = 输入结束）

    # ── 启动：把 6 个关键字串的地址填进表里 ──
    li   t0, A_KWTAB
    li   t1, K0
    sw   t1, 0(t0)
    li   t1, K1
    sw   t1, 4(t0)
    li   t1, K2
    sw   t1, 8(t0)
    li   t1, K3
    sw   t1, 12(t0)
    li   t1, K4
    sw   t1, 16(t0)
    li   t1, K5
    sw   t1, 20(t0)

    # ── 主循环：拿一个 token，输出一行；拿不到就收工 ──
main_loop:
    jal  ra, next_char         # s9 = 下一个字符
    beq  s9, x0, done          # 输入结束
    li   t2, C_SPACE           # 空白：跳过，不产生 token
    beq  s9, t2, main_loop
    li   t2, C_NL
    beq  s9, t2, main_loop
    li   t2, 9                 # 制表
    beq  s9, t2, main_loop
    addi t1, s9, 0             # t1 = 首字符

    li   t2, C_UP_A            # 大写字母？
    blt  t1, t2, ml_lo
    li   t2, C_UP_Z
    bgt  t1, t2, ml_lo
    j    do_word
ml_lo:
    li   t2, C_LO_A            # 小写字母？
    blt  t1, t2, ml_digit
    li   t2, C_LO_Z
    bgt  t1, t2, ml_digit
    j    do_word
ml_digit:
    li   t2, C_ZERO            # 数字？
    blt  t1, t2, do_op
    li   t2, C_NINE
    bgt  t1, t2, do_op
    j    do_number
do_op:
    addi a0, t1, 0             # 首字符交给 read_op
    j    read_op

    # ── 标识符 / 关键字：把整段字母读进 A_BUF，再和 6 个关键字比 ──
do_word:
    li   t6, A_BUF             # t6 = 写到哪儿了
word_loop:
    sb   t1, 0(t6)
    addi t6, t6, 1
    jal  ra, next_char
    addi t1, s9, 0
    beq  t1, x0, word_end
    li   t2, C_UP_A
    blt  t1, t2, wl_lo
    li   t2, C_UP_Z
    bgt  t1, t2, wl_lo
    j    word_loop
wl_lo:
    li   t2, C_LO_A
    blt  t1, t2, word_end
    li   t2, C_LO_Z
    bgt  t1, t2, word_end
    j    word_loop
word_end:
    sb   x0, 0(t6)             # 0 结尾
    # 让它停下来的那个字符不属于这个词：放回回看缓冲，交给下一轮
    beq  s9, x0, word_cmp      # 输入结束就不用还了
    li   t0, A_LK
    sb   s9, 0(t0)
    li   t0, A_KWTAB           # 表首地址
    li   t4, 0                 # 已比过几个
word_cmp:
    li   t2, 6
    beq  t4, t2, do_id
    slli t3, t4, 2             # t3 = 表里的字节偏移
    li   t5, A_KWTAB
    add  t3, t5, t3            # t3 = 这一项的地址（下面 str_eq 不会动 t3）
    lw   a1, 0(t3)             # a1 = 第 t4 个关键字串
    li   a0, A_BUF
    jal  ra, str_eq            # a0 = 1 表示一样
    beq  a0, x0, word_next
    li   a0, S_KW
    lw   a1, 0(t3)
    jal  ra, emit_str2         # `KW:关键字`
    j    main_loop
word_next:
    addi t4, t4, 1
    j    word_cmp
do_id:
    li   a0, S_ID
    li   a1, A_BUF
    jal  ra, emit_str2         # `ID:名字`
    j    main_loop

    # ── 数字：一位一位拼成数值，再印 `NUM:值` ──
    # 注意：字符流统一走 next_char，结果在 s9 里；主循环也是 s9。
    # 所以这里把 s9（= 首字符）交给 num_loop，而不是去读 a0 —— a0 早就被别人用过了。
do_number:
    addi a0, s9, 0             # a0 = 这一位的字符（首字符来自主循环的 s9）
    li   t6, 0                 # t6 = 累计值
num_loop:
    addi t1, a0, -48           # a0 里是这一位的字符
    li   t0, 10
    mul  t6, t6, t0
    add  t6, t6, t1
    jal  ra, next_char
    addi a0, s9, 0
    beq  a0, x0, num_done
    li   t0, C_ZERO
    blt  a0, t0, num_back
    li   t0, C_NINE
    bgt  a0, t0, num_back
    j    num_loop
num_back:
    li   t0, A_LK              # 不是数字了：放回回看缓冲
    sb   a0, 0(t0)
num_done:
    addi a1, t6, 0
    li   a0, S_NUM
    jal  ra, emit_num          # `NUM:值`
    j    main_loop

    # ── 符号：先多看一个字符，决定是一个字还是两个字 ──
    #
    # 两字符运算符一共八个：== != <= >= && || << >>。
    # 判据分两类，最容易漏的是第一类里的 `==`：
    #   a) 第二个字符是 '='：首字符可能是 '='  '!'  '<'  '>'   → 两字符
    #   b) 两个字符相同：'&&' '||' '<<' '>>'                   → 两字符
    # `==` 走的是 (a) 而首字符就是 '=' —— 4-2 的 read_oper 里那条显式的
    # `beq s1, '=' → two` 就是为它写的，漏了它 `==` 就会掉到单字符分支变成两个 `OP:=`。
read_op:
    # 协议（和 4-2 的 read_oper 是同一条规矩）："进来时首字符已经被吃掉了"。
    # 区别只在谁来吃：4-2 是 skip_ws 把首字符 ungetc 回去、read_oper 进来自己 getc 吃掉；
    # 本关的主循环直接把首字符放在 s9 里、再随 a0 传进来 —— 效果一样，都必须是"已吃掉"。
    # 所以这里只需再读一个字符（下一次 next_char），千万不要重复读首字符。
    addi s1, a0, 0             # s1 = 首字符（主循环把首字符放在 a0 里备用）
    jal  ra, next_char
    beq  s9, x0, op_last       # 输入结束：单字符，没什么可还
    addi s3, s9, 0             # s3 = 第二个字符
    li   t2, 61                # '='
    bne  s3, t2, op_same
    li   t2, 61                # '=' → ==
    beq  s1, t2, op_two
    li   t2, 33                # '!' → !=
    beq  s1, t2, op_two
    li   t2, 60                # '<' → <=
    beq  s1, t2, op_two
    li   t2, 62                # '>' → >=
    beq  s1, t2, op_two
    j    op_one
op_same:
    bne  s3, s1, op_one
    li   t2, 38                # '&' → &&
    beq  s1, t2, op_two
    li   t2, 124               # '|' → ||
    beq  s1, t2, op_two
    li   t2, 60                # '<' → <<
    beq  s1, t2, op_two
    li   t2, 62                # '>' → >>
    beq  s1, t2, op_two
    j    op_one
op_two:
    li   t0, A_OSLOT           # 两个字都写进运算符槽（用常量地址，不靠寄存器跨 next_char 活着）
    sb   s1, 0(t0)
    sb   s3, 1(t0)
    sb   x0, 2(t0)
    j    op_emit
op_one:
    li   t3, A_OONE            # 单字符运算符表
    li   t6, 16                # 还剩几个字符没查（注意：**不能**用 t4 —— t4 是
                               # word_cmp 的关键字表下标，两者会互相踩，踩了会死循环）
op_scan:
    beq  t6, x0, op_bad
    lbu  t5, 0(t3)
    beq  t5, s1, op_ok
    addi t3, t3, 1
    addi t6, t6, -1
    j    op_scan
op_ok:
    li   t0, A_OSLOT
    sb   s1, 0(t0)
    sb   x0, 1(t0)
    li   t5, A_LK
    sb   s3, 0(t5)             # 多看的那一眼不是它的：放回回看缓冲
    j    op_emit
op_last:
    # 首字符之后输入就结束了：只可能是单字符运算符，也不能是"两个字才算"的 ! & |
    # —— 直接落到单字符表去查，查不到就是 ERR:
    li   s3, 0
    j    op_one
op_emit:
    li   a0, S_OP
    li   a1, A_OSLOT
    jal  ra, emit_str2         # `OP:符号`
    j    main_loop
op_bad:
    li   a0, S_ERR
    addi a1, s1, 0
    jal  ra, emit_str_c        # `ERR:字符`
    j    done

done:
    li   a0, 0
    li   a7, 0
    ecall

# ════════════════════════ 叶子子程序（都不动 sp / ra） ════════════════════════

# next_char：读一个字符 → s9。回看缓冲里 0 表示空
next_char:
    li   t0, A_LK
    lbu  s9, 0(t0)
    beq  s9, x0, nc_fetch
    sb   x0, 0(t0)             # 用掉它
    jalr x0, 0(ra)
nc_fetch:
    li   t0, INPORT
    lbu  s9, 0(t0)
    jalr x0, 0(ra)

# putch：a0 = 一个字符
putch:
    li   t0, DISPLAY
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# newline
newline:
    li   t0, DISPLAY
    li   t1, C_NL
    sb   t1, 0(t0)
    jalr x0, 0(ra)

# colon：印一个 ':'
colon:
    li   t0, DISPLAY
    li   t1, C_COLON
    sb   t1, 0(t0)
    jalr x0, 0(ra)

# print_int：a0 = 非负整数，按十进制印
# 做法（和 3-1 一样）：在 A_DIGITS 里**从末尾往前**生成各位数字，
# 末尾留一个 0 当哨兵，再从第一个数字往后印到哨兵为止。
#
# 不要写成"从 A_DIGITS 开始往前写、再用地址相等结束循环"：那样一旦
# 待印的位数和缓冲区长度对不上，多出来的空字节会连着一串 \0 一起印出去。
print_int:
    li   t5, 10                # 除数
    li   t6, A_DIGITS          # t6 = 缓冲区末尾
    sb   x0, 0(t6)             # 末尾放哨兵 0
    addi t0, a0, 0             # t0 = 待转换的值
pi_loop:
    addi t6, t6, -1            # 往前挪一格
    rem  t1, t0, t5            # 取最低位
    div  t0, t0, t5
    addi t1, t1, 48            # 数字 → 字符
    sb   t1, 0(t6)
    bne  t0, x0, pi_loop       # 还有高位就继续
pi_print:
    li   t2, DISPLAY
pi_out:
    lbu  t1, 0(t6)             # 读到哨兵 0 就停
    beq  t1, x0, pi_done
    sb   t1, 0(t2)
    addi t6, t6, 1
    j    pi_out
pi_done:
    jalr x0, 0(ra)

# emit_str：a0 = 字符串（0 结尾）
emit_str:
    addi sp, sp, -4
    sw   ra, 0(sp)
es_loop:
    lbu  t0, 0(a0)
    beq  t0, x0, es_done
    li   t1, DISPLAY
    sb   t0, 0(t1)
    addi a0, a0, 1
    j    es_loop
es_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    jalr x0, 0(ra)

# emit_str2：a0 = 前缀串，a1 = 第二段串 → 印 `前缀:第二段` 再换行
emit_str2:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   a1, 4(sp)
    jal  ra, emit_str
    jal  ra, colon
    lw   a0, 4(sp)
    jal  ra, emit_str
    jal  ra, newline
    lw   ra, 0(sp)
    addi sp, sp, 12
    jalr x0, 0(ra)

# emit_str_c：a0 = 前缀串，a1 = 一个字符 → 印 `前缀:字符` 再换行
emit_str_c:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   a1, 4(sp)
    jal  ra, emit_str
    jal  ra, colon
    lw   a0, 4(sp)
    jal  ra, putch
    jal  ra, newline
    lw   ra, 0(sp)
    addi sp, sp, 12
    jalr x0, 0(ra)

# emit_num：a0 = "NUM"，a1 = 数值 → 印 `NUM:值` 再换行
emit_num:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   a1, 4(sp)
    jal  ra, emit_str
    jal  ra, colon
    lw   a0, 4(sp)
    jal  ra, print_int
    jal  ra, newline
    lw   ra, 0(sp)
    addi sp, sp, 12
    jalr x0, 0(ra)

# str_eq：a0 与 a1 两个字符串一模一样吗？→ a0 = 1 / 0
str_eq:
se_loop:
    lbu  t0, 0(a0)
    lbu  t1, 0(a1)
    bne  t0, t1, se_no
    beq  t0, x0, se_yes
    addi a0, a0, 1
    addi a1, a1, 1
    j    se_loop
se_yes:
    addi a0, x0, 1
    jalr x0, 0(ra)
se_no:
    addi a0, x0, 0
    jalr x0, 0(ra)

# ── 数据：用 .org 钉在固定地址，和上面的 S_xxx / Kx / A_xxx 一一对应 ──
    .data
    .org 0x1000
    .asciz "KW"                # S_KW
    .org 0x1004
    .asciz "ID"                # S_ID
    .org 0x1008
    .asciz "NUM"               # S_NUM
    .org 0x100C
    .asciz "OP"                # S_OP
    .org 0x1010
    .asciz "ERR"               # S_ERR
    .org 0x1014
    .asciz ":"                 # S_COLON
    .org 0x1018
    .asciz "if"                # K0
    .org 0x101C
    .asciz "else"              # K1
    .org 0x1024
    .asciz "while"             # K2
    .org 0x102C
    .asciz "return"            # K3
    .org 0x1034
    .asciz "int"               # K4
    .org 0x1038
    .asciz "void"              # K5
    .org 0x1040
    .word 0, 0, 0, 0, 0, 0     # A_KWTAB
    .org 0x1060
    .asciz "+-*/=<>(){};,"      # A_OONE：16 个单字符运算符
                                # 注意这里**没有** ! & |
                                # —— 它们只在 != && || 里作为两字符运算符出现，
                                #    单独出现就是"不认识的字符"，要走 ERR:
    .org 0x2000
    .space 4                   # A_LK：回看缓冲
    .org 0x2100
    .space 128                 # A_BUF
    .org 0x2200
    .space 8                   # A_OSLOT
    .org 0x2300
    .space 40                  # A_DIGITS：print_int 生成数字用的缓冲（从末尾往前写）
