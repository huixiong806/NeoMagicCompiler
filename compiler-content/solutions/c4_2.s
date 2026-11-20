# 4-2 参考解：词法分析（数字与字符串）—— `0x2A` / `'A'` / "hello" 都要会读
#
# 在 4-1 的基础上加四件事：
#   1) 十六进制整数：`0x2A` → 输出 `NUM:42`（数值一律用十进制印出来）
#   2) 字符字面量：`'A'` → 输出 `CHR:65`（印的是码值，不是字符）
#   3) 字符串字面量：`"hello"` → 输出 `STR:hello`（内容照原样，不含两边的引号）
#   4) 转义：`\n` `\t` `\\` `\"` `\'` —— 字符和字符串里都能用
#
# 判断一个数字是十进制还是十六进制，只能靠"多看一个字符"：
#   首字符是 '0' 时再看一眼——是 x / X 就按十六进制读（0x 两个字符都吞掉），
#   不是就把那一眼还回去、按十进制读（'0' 这一位自己也算进去）。
#   只看"第二个字符是不是数字"是不够的：'0x2A' 和 '0' 都得以 0 开头。
#
# 规矩（这一关踩过的坑，写在这里备查）：
#   * 跨 jal 要保住的值只能放 s0–s11：t0–t6 / a0–a7 会被子程序静默冲掉——
#     getc 自己就用 t0/t1，所以"缓冲区指针"这类跨调用的值一律放 s 寄存器。
#   * next_token 的返回值：0 = 输入结束（main 收工），非 0 = 印好了一个 token。

    .equ INPORT,   0xF0000004  # 输入端口
    .equ DISPLAY,  0xF0000000  # 显示端口
    .equ C_TAB,    9
    .equ C_NL,     10
    .equ C_SPACE,  32
    .equ C_DQUOTE, 34          # "
    .equ C_SQUOTE, 39          # '
    .equ C_ZERO,   48
    .equ C_NINE,   57
    .equ C_UPPER_A, 65
    .equ C_UPPER_F, 70
    .equ C_UPPER_X, 88
    .equ C_UPPER_Z, 90
    .equ C_BACKSLASH, 92       # \
    .equ C_LOWER_A, 97
    .equ C_LOWER_F, 102
    .equ C_LOWER_X, 120
    .equ C_LOWER_Z, 122

    .text
main:
    li   sp, 0x3000            # 自己的栈：sp 往下长
    li   t0, INPORT            # 把输入端口地址记在一个全局变量里，
    la   t1, inport            # 这样任何子程序都能安全地读字符（不占用 s 寄存器）
    sw   t0, 0(t1)
    li   t0, kwtab             # 填关键字表
    la   t1, kw0
    sw   t1, 0(t0)
    la   t1, kw1
    sw   t1, 4(t0)
    la   t1, kw2
    sw   t1, 8(t0)
    la   t1, kw3
    sw   t1, 12(t0)
    la   t1, kw4
    sw   t1, 16(t0)
    la   t1, kw5
    sw   t1, 20(t0)

loop:
    jal  ra, next_token
    beq  a0, x0, done          # 0 = 输入结束
    j    loop                  # 非 0 = 刚印完一个 token，接着切下一个

done:
    li   a0, 0
    li   a7, 0
    ecall

# ═══ 子程序：读一个 token 并输出一行 ═══
# 返回 a0：0 = 输入结束；1 = 这一轮印好了一个 token
next_token:
    addi sp, sp, -24
    sw   ra, 0(sp)
    sw   s4, 4(sp)             # s4/s5/s6 在下面查关键字表时用：
    sw   s5, 8(sp)             # str_eq 会把 t0/t1 冲掉，所以表和计数必须放 s 寄存器
    sw   s6, 12(sp)
    jal  ra, skip_ws           # 空白直接跳过；a0 回来时是第一个非空白字符
    beq  a0, x0, nt_ret        # 0 = 输入结束：a0 还是 0，main 就收工
    addi t1, a0, 0             # t1 = 首字符（a0 会被后面的子程序用掉，先抄一份）
    li   t2, C_UPPER_A         # 字母 → 关键字 / 标识符
    blt  t1, t2, nt_lower
    li   t2, C_UPPER_Z
    bgt  t1, t2, nt_lower
    j    nt_word
nt_lower:
    li   t2, C_LOWER_A
    blt  t1, t2, nt_digit
    li   t2, C_LOWER_Z
    bgt  t1, t2, nt_digit
    j    nt_word
nt_digit:
    li   t2, C_ZERO            # 数字开头 → 十进制或十六进制整数
    blt  t1, t2, nt_quote
    li   t2, C_NINE
    bgt  t1, t2, nt_quote
    addi a0, t1, 0             # 首字符要在 a0 里：read_number 从 a0 收参数
    jal  ra, read_number       # 从第二个字符开始接着读
    jal  ra, emit_num
    jal  ra, emit_nl
    li   a0, 1
    j    nt_ret
nt_quote:
    li   t2, C_DQUOTE          # 双引号 → 字符串字面量
    bne  t1, t2, nt_chr
    jal  ra, read_string
    jal  ra, emit_nl
    li   a0, 1
    j    nt_ret
nt_chr:
    li   t2, C_SQUOTE          # 单引号 → 字符字面量
    bne  t1, t2, nt_op
    jal  ra, read_char
    jal  ra, emit_nl
    li   a0, 1
    j    nt_ret
nt_op:
    addi a0, t1, 0             # 首字符要在 a0 里：下面的子程序都从 a0 收参数
    jal  ra, read_oper         # 自己印 OP:xxx
    jal  ra, emit_nl
    li   a0, 1
    j    nt_ret
nt_word:
    addi a0, t1, 0             # 首字符也要放进 a0：read_word 从 a0 收参数
    jal  ra, read_word         # a0 回来时是 buffer 的地址，整词已经以 0 收尾
    addi s6, a0, 0             # s6 = 刚读下来的这个词
    la   s4, kwtab             # s4 = 关键字表
    li   s5, 6                 # s5 = 还有几个槽没比
nt_scan:
    beq  s5, x0, nt_id
    addi a0, s6, 0
    lw   a1, 0(s4)
    jal  ra, str_eq
    bne  a0, x0, nt_kw
    addi s4, s4, 4
    addi s5, s5, -1
    j    nt_scan
nt_kw:
    li   t3, 6
    sub  t3, t3, s5            # 命中第 (6 - s5) 个槽
    slli t3, t3, 2
    la   t4, kwtab
    add  t4, t4, t3
    lw   a1, 0(t4)
    la   a0, s_kw
    jal  ra, emit_tok
    jal  ra, emit_nl
    li   a0, 1
    j    nt_ret
nt_id:
    la   a0, s_id
    addi a1, s6, 0
    jal  ra, emit_tok
    jal  ra, emit_nl
    li   a0, 1
nt_ret:
    lw   ra, 0(sp)
    lw   s4, 4(sp)
    lw   s5, 8(sp)
    lw   s6, 12(sp)
    addi sp, sp, 24
    jalr x0, 0(ra)

# ═══ 子程序：跳过空白 ═══
# 返回 a0 = 第一个非空白字符（还没被消耗掉，还在回看缓冲里）；0 = 输入结束
skip_ws:
    addi sp, sp, -4
    sw   ra, 0(sp)
sw_loop:
    jal  ra, getc
    beq  a0, x0, sw_done
    li   t2, C_SPACE
    beq  a0, t2, sw_loop
    li   t2, C_TAB
    beq  a0, t2, sw_loop
    li   t2, C_NL
    beq  a0, t2, sw_loop
    jal  ra, ungetc            # 不是空白：还回去，让 read_* 再 getc 一次拿到它
sw_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    jalr x0, 0(ra)

# ═══ 子程序：读一个十进制或十六进制整数 → a0 ═══
# 进来时首字符（一个数字）已经在 a0 里，而且它还在回看缓冲里
# （skip_ws 把它还回去了），所以下面第一次 getc 拿到的就是它。
# 只有首字符是 '0' 才需要"再看一眼"：0x / 0X 才有十六进制。
read_number:
    addi sp, sp, -16
    sw   ra, 0(sp)
    sw   s7, 4(sp)             # s7 = 累计值
    sw   s3, 8(sp)             # s3 借给 hex_value 暂存字符
    jal  ra, getc              # 首字符（复看一次）
    li   t3, C_ZERO
    bne  a0, t3, rn_dec        # 不是 '0' 开头 → 只可能是十进制
    jal  ra, getc              # 首字符是 '0'：再看一眼是不是 x / X
    li   t3, C_UPPER_X
    beq  a0, t3, rn_hex
    li   t3, C_LOWER_X
    beq  a0, t3, rn_hex
    jal  ra, ungetc            # 不是 x：把多看的那一眼还回去
    li   a0, C_ZERO            # 首字符 '0' 自己就是十进制的第一位（多看的那一眼已还回去）
    j    rn_dec
rn_hex:
    addi s7, x0, 0             # 已经吃掉 '0' 和 x/X，一位一位拼
rn_hex_loop:
    jal  ra, getc
    beq  a0, x0, rn_done       # 输入结束（十六进制没读满也不管了）
    jal  ra, hex_value         # a1 = 这一位的值；-1 = 不是十六进制位
    blt  a1, x0, rn_hex_end
    slli s7, s7, 4             # 值 = 值 * 16 + 这一位
    add  s7, s7, a1
    j    rn_hex_loop
rn_hex_end:
    addi a0, s3, 0             # hex_value 把那个"不是十六进制位"的字符留在 s3 里
    jal  ra, ungetc
    j    rn_done

# 十进制：a0 是第一个要算进去的数字字符
rn_dec:
    addi s7, x0, 0
rn_dec_loop:
    addi a0, a0, -48           # 数字字符 → 数字
    li   t2, 10
    mul  s7, s7, t2            # 值 = 值 * 10 + 这一位
    add  s7, s7, a0
    jal  ra, getc              # 看下一个
    beq  a0, x0, rn_done
    li   t2, C_ZERO
    blt  a0, t2, rn_dec_end    # 不是数字：这个数完了（先别把它算进去）
    li   t2, C_NINE
    bgt  a0, t2, rn_dec_end
    j    rn_dec_loop
rn_dec_end:
    jal  ra, ungetc            # 让它停下来的那个字符不属于这个数
rn_done:
    addi a0, s7, 0
    lw   ra, 0(sp)
    lw   s7, 4(sp)
    lw   s3, 8(sp)
    addi sp, sp, 16
    jalr x0, 0(ra)

# ═══ 子程序：判断 a0 是不是十六进制位 ═══
# 返回 a1 = 0..15；不是十六进制位时 a1 = -1，并且把原字符留在 s3 里
hex_value:
    addi s3, a0, 0             # s3 = 原字符（还回输入时要用）
    li   t2, C_ZERO
    blt  a0, t2, hv_no
    li   t2, C_NINE
    bgt  a0, t2, hv_letter
    addi a1, a0, -48
    jalr x0, 0(ra)
hv_letter:
    addi t3, a0, 0             # 小写先折成大写，'a' 与 'A' 相差 32
    li   t2, C_LOWER_A
    blt  t3, t2, hv_check
    li   t2, C_LOWER_Z
    bgt  t3, t2, hv_check
    addi t3, t3, -32
hv_check:
    li   t2, C_UPPER_A
    blt  t3, t2, hv_no
    li   t2, C_UPPER_F
    bgt  t3, t2, hv_no
    addi a1, t3, -55           # 'A' → 10
    jalr x0, 0(ra)
hv_no:
    addi a1, x0, -1
    jalr x0, 0(ra)

# ═══ 子程序：读一个字符字面量 ═══
# 进来时 a0 = 左边的单引号（它还在回看缓冲里，先 getc 掉），返回时已经吃掉右边的单引号。
# 输出 `CHR:码值`：印的是码值，不是那个字符本身
read_char:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   s3, 4(sp)
    jal  ra, getc              # 吃掉左边那个单引号
    jal  ra, getc              # 再读字符本体
    addi s3, a0, 0             # s3 = 字符本体
    li   t0, C_BACKSLASH
    bne  s3, t0, rc_after      # 不是反斜杠就是普通字符
    jal  ra, getc              # 是反斜杠：再读一个，转成真正的码值
    jal  ra, escape_value
    addi s3, a0, 0
rc_after:
    jal  ra, getc              # 收尾的那个单引号
    la   a0, s_chr
    jal  ra, emit_label        # 印 "CHR:"
    addi a0, s3, 0
    jal  ra, print_int         # 再印十进制码值（CHR:65 里的 65）
    lw   ra, 0(sp)
    lw   s3, 4(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：读一个字符串字面量 ═══
# 进来时 a0 = 左边的双引号（先 getc 掉），返回时已经吃掉右边的双引号。
# 输出 `STR:内容`，内容按原样输出，转义字符换成它真正的那个字节
read_string:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   s3, 4(sp)
    jal  ra, getc              # 吃掉左边那个双引号
    li   s3, C_DQUOTE          # s3 = 收尾用的那个双引号
    la   a0, s_str
    jal  ra, emit_label        # 先印 "STR:"
rs_loop:
    jal  ra, getc
    beq  a0, x0, rs_end        # 输入提前结束也得收手，别死循环
    beq  a0, s3, rs_end        # 遇到收尾双引号
    li   t0, C_BACKSLASH
    bne  a0, t0, rs_plain
    jal  ra, getc
    jal  ra, escape_value      # 转义 → 真正的那个字符
rs_plain:
    li   t0, DISPLAY           # 边读边印，不用先攒起来
    sb   a0, 0(t0)
    j    rs_loop
rs_end:
    lw   ra, 0(sp)
    lw   s3, 4(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：反斜杠后面的那个字符 → 真正的码值 ═══
# '\n' → 10、'\t' → 9、'\\' → 92、'\'' 和 '\"' 就是引号本身，
# 其他情况（比如 '\0'）原样返回，够这一关用了
escape_value:
    li   t0, 110               # 'n'
    beq  a0, t0, ev_nl
    li   t0, 116               # 't'
    beq  a0, t0, ev_tab
    jalr x0, 0(ra)
ev_nl:
    li   a0, C_NL
    jalr x0, 0(ra)
ev_tab:
    li   a0, C_TAB
    jalr x0, 0(ra)

# ═══ 子程序：把一整个标识符/关键字读进 buffer → a0 = buffer 的地址 ═══
# 名字 = 字母或数字（首字符一定是字母，next_token 已经判过）。
# 缓冲区游标必须放 s 寄存器：getc 每次都把 t0 冲掉。
read_word:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   s5, 4(sp)
    la   s5, buffer
rw_loop:
    jal  ra, getc
    addi t3, a0, 0
    beq  t3, x0, rw_end
    li   t2, C_UPPER_A
    blt  t3, t2, rw_lower
    li   t2, C_UPPER_Z
    bgt  t3, t2, rw_lower
    j    rw_store
rw_lower:
    li   t2, C_LOWER_A
    blt  t3, t2, rw_digit
    li   t2, C_LOWER_Z
    bgt  t3, t2, rw_digit
    j    rw_store
rw_digit:
    li   t2, C_ZERO            # 名字里可以有数字：x10 是一个名字，不是 0x10
    blt  t3, t2, rw_stop
    li   t2, C_NINE
    bgt  t3, t2, rw_stop
rw_store:
    sb   t3, 0(s5)
    addi s5, s5, 1
    j    rw_loop
rw_stop:
    jal  ra, ungetc            # 让这个词停下来的字符不属于它
rw_end:
    sb   x0, 0(s5)             # 0 收尾，后面 str_eq 直接用
    la   a0, buffer
    lw   ra, 0(sp)
    lw   s5, 4(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：读一个符号并输出 OP:x ═══
# 进来时 a0 = 首字符（它还在回看缓冲里）：先把它吃掉，再"多看一个字符"决定是
# 单字符还是双字符符号（== != <= >= && ||）
read_oper:
    addi sp, sp, -32
    sw   ra, 0(sp)
    sw   s0, 4(sp)
    sw   s1, 8(sp)
    sw   s2, 12(sp)
    sw   s3, 16(sp)
    addi s1, a0, 0             # s1 = 首字符
    jal  ra, getc              # 把首字符从回看缓冲里吃掉
    jal  ra, getc              # 多看一个字符
    addi s3, a0, 0
    li   t2, 61                # '='
    bne  s3, t2, ro_same
    li   t5, 61                # '='
    beq  s1, t5, ro_two        # ==
    li   t5, 33                # '!'
    beq  s1, t5, ro_two        # !=
    li   t5, 60                # '<'
    beq  s1, t5, ro_two        # <=
    li   t5, 62                # '>'
    beq  s1, t5, ro_two        # >=
    j    ro_one
ro_same:
    bne  s3, s1, ro_one
    li   t5, 38                # '&'
    beq  s1, t5, ro_two
    li   t5, 124               # '|'
    beq  s1, t5, ro_two
    j    ro_one
ro_two:
    la   s0, o_slot
    sb   s1, 0(s0)
    sb   s3, 1(s0)
    sb   x0, 2(s0)
    li   s2, 0                 # 0 = 正常
    j    ro_emit
ro_one:
    la   t0, o_one
    li   t1, 16                # 单字符符号表（16 个）
ro_scan:
    beq  t1, x0, ro_unknown
    lbu  t2, 0(t0)
    beq  t2, s1, ro_ok
    addi t0, t0, 1
    addi t1, t1, -1
    j    ro_scan
ro_ok:
    la   s0, o_slot
    sb   s1, 0(s0)
    sb   x0, 1(s0)
    addi a0, s3, 0
    jal  ra, ungetc
    li   s2, 0
    j    ro_emit
ro_unknown:
    li   s2, 1
    addi a1, s1, 0
    la   a0, s_err
    jal  ra, emit_tok
    addi a0, x0, 1
    j    ro_ret
ro_emit:
    la   a0, s_op
    mv   a1, s0
    jal  ra, emit_tok
    li   s2, 0
    addi a0, x0, 0
ro_ret:
    lw   ra, 0(sp)
    lw   s0, 4(sp)
    lw   s1, 8(sp)
    lw   s2, 12(sp)
    lw   s3, 16(sp)
    addi sp, sp, 32
    jalr x0, 0(ra)

# ═══ 子程序：把 a0 处的字符串（0 结尾）原样输出 ═══
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

# ═══ 子程序：输出 `标签:`（a0 = 标签字符串的地址）═══
emit_label:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, emit_str
    la   a0, s_colon
    jal  ra, emit_str
    lw   ra, 0(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：输出 `标签:值` ═══
# a1 小于 256 当字符印；否则当字符串地址印一串字（OP:x 与 KW:/ID: 共用）
emit_tok:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   a1, 4(sp)
    jal  ra, emit_label
    lw   a0, 4(sp)
    li   t0, 256
    bltu a0, t0, et_char
    jal  ra, emit_str
    j    et_done
et_char:
    li   t0, DISPLAY
    sb   a0, 0(t0)
et_done:
    lw   ra, 0(sp)
    addi sp, sp, 12
    jalr x0, 0(ra)

# ═══ 子程序：输出 `NUM:` 加十进制数值 ═══
emit_num:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)             # 先存住数值：下面还要用它
    la   a0, s_num
    jal  ra, emit_label        # 印 "NUM:"（冒号不能忘了）
    lw   a0, 4(sp)
    jal  ra, print_int
    lw   ra, 0(sp)
    addi sp, sp, 8
    jalr x0, 0(ra)

# ═══ 子程序：换行 ═══
emit_nl:
    li   t0, DISPLAY
    li   t1, C_NL
    sb   t1, 0(t0)
    jalr x0, 0(ra)

# ═══ 子程序：把 a0 里的非负整数按十进制印出来 ═══
# 递归：先把 a0 / 10 印完（高位），回来再印 a0 % 10（最低位）
print_int:
    li   t0, 10
    blt  a0, t0, pi_last
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)             # 原值存在自己的栈帧里，回来还要用它取最低位
    div  a0, a0, t0
    jal  ra, print_int
    li   t0, 10
    lw   a0, 4(sp)
    rem  a0, a0, t0
    lw   ra, 0(sp)
    addi sp, sp, 8
pi_last:
    addi a0, a0, 48
    li   t0, DISPLAY
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# ═══ 子程序：字符串相等比较（a0、a1 = 两个 0 结尾的字符串）→ a0 = 0/1 ═══
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

# ═══ 子程序：getc / ungetc —— 一个字符的回看缓冲（<= 就是靠它认出来的）═══
getc:
    la   t0, lookahead
    lbu  t1, 0(t0)
    beq  t1, x0, getc_fetch
    sb   x0, 0(t0)
    addi a0, t1, 0
    jalr x0, 0(ra)
getc_fetch:
    la   t0, inport
    lw   t0, 0(t0)             # t0 = 输入端口地址
    lbu  a0, 0(t0)             # 读一个字符（读到 0 就是输入结束）
    jalr x0, 0(ra)

ungetc:
    la   t0, lookahead
    sb   a0, 0(t0)
    jalr x0, 0(ra)

# ── 数据 ──
    .data
s_colon:
    .asciz ":"
s_kw:
    .asciz "KW"
s_id:
    .asciz "ID"
s_num:
    .asciz "NUM"
s_op:
    .asciz "OP"
s_chr:
    .asciz "CHR"
s_str:
    .asciz "STR"
s_err:
    .asciz "ERR"
kw0:
    .asciz "if"
kw1:
    .asciz "else"
kw2:
    .asciz "while"
kw3:
    .asciz "return"
kw4:
    .asciz "int"
kw5:
    .asciz "void"
    .align 4                   # 下面的东西要用 lw 读，必须 4 字节对齐
o_one:
    .asciz "+-*/=<>(){};,!&|"
    .align 4
o_slot:
    .space 4
    .align 4
lookahead:
    .space 4
    .align 4
kwtab:
    .word 0, 0, 0, 0, 0, 0
buffer:
    .space 512
    .align 4
inport:
    .word 0                    # 输入端口地址（启动时填，子程序都从这里取）
