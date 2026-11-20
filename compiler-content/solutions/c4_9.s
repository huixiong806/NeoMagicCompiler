# ═══════════════════════════════════════════════════════════════════════════
# 4-9 参考解：把「分支与循环」编译成星之语
#
# 玩家写的仍然是一台**编译器**（程序模式）：
#   输入：星咒源码文本（从 0xF0000004 逐字符读，读到 0 表示输入结束）
#   输出：星之语汇编文本（逐字符写到显示端口 0xF0000000）
#
# 本关语法（在 4-8 上加了两条语句和一个"条件"）：
#   程序 := 语句*
#   语句 := 'int' 名字 '=' expr ';'
#         | 名字 '=' expr ';'
#         | 'if' '(' 条件 ')' '{' 语句* '}' ['else' '{' 语句* '}']
#         | 'while' '(' 条件 ')' '{' 语句* '}'
#         | '{' 语句* '}'
#         | expr ';'
#   条件 := expr [('<' | '<=' | '>' | '>=' | '==' | '!=') expr]
#   expr  := term (('+' | '-') term)*
#   term  := unary (('*' | '/') unary)*
#   unary := '-' unary | '(' expr ')' | 数字 | 名字
#
# ── 生成出来的代码（和 4-7/4-8 同一套，也是 4-10 ~ 4-12 在用的那套）──
#   1) 二元运算一律"左值压栈"：
#        <左边算进 a0>
#        addi sp, sp, -4
#        sw a0, 0(sp)
#        <右边算进 a0>
#        lw a1, 0(sp)
#        addi sp, sp, 4
#        add a0, a1, a0        ← a1 = 左，a0 = 右；sub/div 的顺序绝不能反
#   2) 标签统一叫 L1、L2、L3…（按"需要的顺序"发号，从 1 开始）：
#        while：先发"循环头"，再发"出口"
#        if：先发"假分支"，有 else 时再发"结束"
#   3) 条件算成 0/1 放进 a0（真=1、假=0），分支统一写 beqz：
#        while：  L<头>: <条件> beqz a0, L<出> <body> j L<头>  L<出>:
#        if：     <条件> beqz a0, L<假> <then> [j L<结束> L<假>: <else> L<结束>:]
#                  没有 else 时 L<假>: 就在 then 后面（不吐多余的 j）
#      比较：左值压栈 → 右值进 a0 → 弹回 a1（a1=左、a0=右），然后
#        a <  b   slt a0, a1, a0               （左 < 右）
#        a >  b   slt a0, a0, a1               （右 < 左）
#        a <= b   slt a0, a0, a1 ; seqz a0, a0 （右 < 左 取反）
#        a >= b   slt a0, a1, a0 ; seqz a0, a0 （左 < 右 取反）
#        a == b   sub a0, a1, a0 ; seqz a0, a0
#        a != b   sub a0, a1, a0 ; snez a0, a0
#      （没有比较符时，表达式自己的值就是条件：非 0 即真）
#   4) if / while 都不动"值约定"：语句跑完 a0 里是它自然留下的值——
#      while 退出时条件刚算成 0，所以 a0 = 0；if 没走进去时也是 0（条件值），
#      走进去就是 then 里最后一条语句的值。
#   指令一律不缩进、不带注释，一行一条，行尾都是 \n。
#
# 编译器自己的实现要点：凡是跨 jal 还要用的中间结果一律放**栈帧**或 s 寄存器
# （辅助函数会冲掉 t0–t6；ecall 只写 a0）。
# ═══════════════════════════════════════════════════════════════════════════

.data
src:        .space 1024
out:        .space 8192
tinstr:     .space 16
decls:      .space 32
cur:        .word 0
wc:         .word 0
labelno:    .word 0
str_data:   .asciz ".data\n"
str_text:   .asciz ".text\nmain:\n"
str_word0:  .asciz ": .word 0\n"
str_la:     .asciz "la a1, "
str_nl:     .asciz "\n"
str_li:     .asciz "li a0, "
str_lw:     .asciz "lw a0, 0(a1)\n"
str_sw:     .asciz "sw a0, 0(a1)\n"
str_push:   .asciz "addi sp, sp, -4\nsw a0, 0(sp)\n"
str_pop:    .asciz "lw a1, 0(sp)\naddi sp, sp, 4\n"
str_add:    .asciz "add a0, a1, a0\n"
str_sub:    .asciz "sub a0, a1, a0\n"
str_mul:    .asciz "mul a0, a1, a0\n"
str_div:    .asciz "div a0, a1, a0\n"
str_neg:    .asciz "sub a0, x0, a0\n"
str_label:  .asciz "L"
str_colon:  .asciz ":\n"
str_beqz:   .asciz "beqz a0, L"
str_j:      .asciz "j L"
str_slt_l:  .asciz "slt a0, a1, a0\n"
str_slt_r:  .asciz "slt a0, a0, a1\n"
str_sle:    .asciz "slt a0, a0, a1\nseqz a0, a0\n"
str_sge:    .asciz "slt a0, a1, a0\nseqz a0, a0\n"
str_seq:    .asciz "sub a0, a1, a0\nseqz a0, a0\n"
str_sne:    .asciz "sub a0, a1, a0\nsnez a0, a0\n"
str_ret:    .asciz "ret\n"
kw_int:     .asciz "int"
kw_if:      .asciz "if"
kw_else:    .asciz "else"
kw_while:   .asciz "while"

.text
# ── 入口 ──────────────────────────────────────────────────────
compile_entry:
    # 1) 把输入读进 src（读到 0 表示输入结束）
    la   a1, src
    li   a2, 0xF0000004
read_loop:
    lw   a0, 0(a2)
    beqz a0, read_done
    sb   a0, 0(a1)
    addi a1, a1, 1
    j    read_loop
read_done:
    sb   x0, 0(a1)

    # 2) 预扫：哪些字母被声明过（数据段要写在代码段前面）
    jal  ra, prescan_decls

    # 3) 吐 ".data" 和所有变量，再吐 ".text" / "main:"
    la   a0, str_data
    jal  ra, emit_str
    jal  ra, emit_data_seg
    la   a0, str_text
    jal  ra, emit_str

    # 4) 编译所有语句
    jal  ra, gen_stmts

    # 5) 收尾
    la   a0, str_ret
    jal  ra, emit_str

    # 6) 把 out 整块写到显示端口
    la   s0, out
    li   s1, 0xF0000000
emit_out_loop:
    lbu  a0, 0(s0)
    beqz a0, emit_out_done
    sw   a0, 0(s1)
    addi s0, s0, 1
    j    emit_out_loop
emit_out_done:
    li   a0, 0
    li   a7, 0
    ecall

# ═══════════════════════════════════════════════════════════════════════════
#  拼文本：emit_char / emit_str / emit_num
# ═══════════════════════════════════════════════════════════════════════════

# ── emit_char：把 a0 里的一个字符接到 out 末尾 ──────────────────
emit_char:
    la   t0, out
emit_char_skip:
    lbu  t1, 0(t0)
    beqz t1, emit_char_put
    addi t0, t0, 1
    j    emit_char_skip
emit_char_put:
    sb   a0, 0(t0)
    addi t0, t0, 1
    sb   x0, 0(t0)
    ret

# ── emit_str：把 a0 指向的 NUL 结尾字符串接到 out 末尾 ───────────
emit_str:
    la   t0, out
emit_str_skip:
    lbu  t1, 0(t0)
    beqz t1, emit_str_copy
    addi t0, t0, 1
    j    emit_str_skip
emit_str_copy:
    lbu  t1, 0(a0)
    beqz t1, emit_str_done
    sb   t1, 0(t0)
    addi a0, a0, 1
    addi t0, t0, 1
    j    emit_str_copy
emit_str_done:
    sb   x0, 0(t0)
    ret

# ── emit_num：把 a0 里的非负整数按十进制接到 out 末尾（a0 被破坏）──
emit_num:
    la   t0, out
emit_num_skip:
    lbu  t1, 0(t0)
    beqz t1, emit_num_start
    addi t0, t0, 1
    j    emit_num_skip
emit_num_start:
    la   t1, tinstr
    addi t2, t1, 15
    sb   x0, 0(t2)
    addi t2, t2, -1
    li   t3, 10
    bnez a0, emit_num_digits
    li   t4, 48
    sb   t4, 0(t2)
    addi t2, t2, -1
    j    emit_num_copy
emit_num_digits:
    remu t4, a0, t3
    addi t4, t4, 48
    sb   t4, 0(t2)
    addi t2, t2, -1
    divu a0, a0, t3
    bnez a0, emit_num_digits
emit_num_copy:
    addi t2, t2, 1
emit_num_copy_loop:
    lbu  t4, 0(t2)
    beqz t4, emit_num_done
    sb   t4, 0(t0)
    addi t0, t0, 1
    addi t2, t2, 1
    j    emit_num_copy_loop
emit_num_done:
    sb   x0, 0(t0)
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  词法：peek / adv / skip_ws / getc / getnum / match
#  ⚠ adv 只动 t0–t3，a0 不碰；跨 jal 的值放栈帧或 s 寄存器。
# ═══════════════════════════════════════════════════════════════════════════

# ── reset_lex：词法游标归零，并把第一个字符读进 wc ──────────────
reset_lex:
    la   t0, cur
    sw   x0, 0(t0)
    la   t1, src
    lbu  t2, 0(t1)
    la   t1, wc
    sw   t2, 0(t1)
    ret

# ── adv：游标前进一个字符（把下一个字符读进 wc）───────────────────
adv:
    la   t0, cur
    lw   t1, 0(t0)
    addi t1, t1, 1
    sw   t1, 0(t0)
    la   t2, src
    add   t2, t2, t1
    lbu  t3, 0(t2)
    la   t0, wc
    sw   t3, 0(t0)
    ret

# ── peek：看当前位置的字符（不吃掉）────────────────────────────
peek:
    la   t6, wc
    lw   a0, 0(t6)
    ret

# ── is_ws：a0 里的字符是空白吗？是返回 1 ────────────────────────
is_ws:
    li   t0, 32
    beq  a0, t0, is_ws_yes
    li   t0, 10
    beq  a0, t0, is_ws_yes
    li   t0, 13
    beq  a0, t0, is_ws_yes
    li   t0, 9
    beq  a0, t0, is_ws_yes
    li   a0, 0
    ret
is_ws_yes:
    li   a0, 1
    ret

# ── skip_ws ───────────────────────────────────────────────────
skip_ws:
    addi sp, sp, -4
    sw   ra, 0(sp)
skip_ws_loop:
    jal  ra, peek
    jal  ra, is_ws
    beqz a0, skip_ws_done
    jal  ra, adv
    j    skip_ws_loop
skip_ws_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── getc：跳过空白后，返回当前字符并**吃掉**它（到头返回 0）────────
getc:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, skip_ws
    la   t6, wc
    lw   t5, 0(t6)
    beqz t5, getc_done
    jal  ra, adv
getc_done:
    addi a0, t5, 0
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── match：跳过空白后，当前字符正好是 a0 就给吃掉并返回 1 ──────────
match:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    jal  ra, skip_ws
    jal  ra, peek
    lw   t0, 4(sp)
    bne  a0, t0, match_no
    jal  ra, adv
    li   a0, 1
    j    match_done
match_no:
    li   a0, 0
match_done:
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── eat_semi：吃掉 ';' ─────────────────────────────────────────
eat_semi:
    addi sp, sp, -4
    sw   ra, 0(sp)
    li   a0, 59                 # ';'
    jal  ra, match
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── getnum：把一串十进制数字读成一个整数（游标停在数字后面）────────
getnum:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, skip_ws
    li   t4, 0
    li   t5, 10
getnum_loop:
    la   t6, wc
    lw   t1, 0(t6)
    li   t0, 48
    blt  t1, t0, getnum_done
    li   t0, 58
    bge  t1, t0, getnum_done
    mul  t4, t4, t5
    addi t1, t1, -48
    add  t4, t4, t1
    jal  ra, adv
    j    getnum_loop
getnum_done:
    addi a0, t4, 0
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  两遍扫描：预扫变量 + 吐数据段
# ═══════════════════════════════════════════════════════════════════════════

# ── check_kw：a0 指向 NUL 结尾的关键字；当前位置（cur）正好是它 → 1 ──
check_kw:
    la   t0, src
    la   t1, cur
    lw   t1, 0(t1)
    add  t0, t0, t1
    addi t2, a0, 0
check_kw_loop:
    lbu  t3, 0(t2)
    beqz t3, check_kw_yes
    lbu  t4, 0(t0)
    bne  t3, t4, check_kw_no
    addi t2, t2, 1
    addi t0, t0, 1
    j    check_kw_loop
check_kw_yes:
    li   a0, 1
    ret
check_kw_no:
    li   a0, 0
    ret

# ── eat_word：把 a0 指向的关键字逐字符吃掉 ─────────────────────
eat_word:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
eat_word_loop:
    lw   t0, 4(sp)
    lbu  t1, 0(t0)
    beqz t1, eat_word_done
    jal  ra, adv
    lw   t0, 4(sp)
    addi t0, t0, 1
    sw   t0, 4(sp)
    j    eat_word_loop
eat_word_done:
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── prescan_decls：只找 `int 字母`，把字母记进 decls（26 个格子）──
prescan_decls:
    la   s3, src
    li   s5, 0
prescan_len:
    add  t0, s3, s5
    lbu  t1, 0(t0)
    beqz t1, prescan_loop
    addi s5, s5, 1
    j    prescan_len
prescan_loop:
    lbu  t0, 0(s3)
    beqz t0, prescan_done
    li   t1, 105                 # 'i'
    bne  t0, t1, prescan_next
    addi t0, s3, 4               # 变量名那一格
    add  t1, s3, s5              # 源码结尾（0 那一格）
    bgeu t0, t1, prescan_next    # 连 "int X" 都放不下
    lbu  t1, 1(s3)
    li   t2, 110                 # 'n'
    bne  t1, t2, prescan_next
    lbu  t1, 2(s3)
    li   t2, 116                 # 't'
    bne  t1, t2, prescan_next
    lbu  t1, 3(s3)               # int 后面必须是空白
    li   t0, 32
    beq  t1, t0, prescan_mark
    li   t0, 10
    beq  t1, t0, prescan_mark
    j    prescan_next
prescan_mark:
    lbu  t1, 4(s3)               # 变量名（单个字母 a–z）
    li   t0, 97
    blt  t1, t0, prescan_next
    li   t0, 123
    bge  t1, t0, prescan_next
    addi t1, t1, -97
    la   t0, decls
    add  t0, t0, t1
    li   t1, 1
    sb   t1, 0(t0)
    addi s3, s3, 4
    j    prescan_loop
prescan_next:
    addi s3, s3, 1
    j    prescan_loop
prescan_done:
    j    reset_lex

# ── emit_data_seg：按字母顺序吐出声明过的变量 ─────────────────
emit_data_seg:
    addi sp, sp, -4
    sw   ra, 0(sp)
    li   s2, 97                  # 'a'
emit_data_loop:
    li   t0, 123
    bge  s2, t0, emit_data_done
    la   t1, decls
    addi t2, s2, -97
    add  t1, t1, t2
    lbu  t1, 0(t1)
    beqz t1, emit_data_next
    addi a0, s2, 0
    jal  ra, emit_char
    la   a0, str_word0
    jal  ra, emit_str
emit_data_next:
    addi s2, s2, 1
    j    emit_data_loop
emit_data_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── emit_la_var：吐 `la a1, <名字>\n` ──────────────────────────
emit_la_var:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    la   a0, str_la
    jal  ra, emit_str
    lw   a0, 4(sp)
    jal  ra, emit_char
    la   a0, str_nl
    jal  ra, emit_str
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  标签发号器：L1、L2、L3…（按"用到的顺序"，从 1 开始）
# ═══════════════════════════════════════════════════════════════════════════

# ── next_label：a0 = 新编号 ───────────────────────────────────
next_label:
    la   t0, labelno
    lw   a0, 0(t0)
    addi a0, a0, 1
    sw   a0, 0(t0)
    ret

# ── emit_label_def：吐 `L<编号>:\n` ───────────────────────────
emit_label_def:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    la   a0, str_label
    jal  ra, emit_str
    lw   a0, 4(sp)
    jal  ra, emit_num
    la   a0, str_colon
    jal  ra, emit_str
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── emit_beqz：吐 `beqz a0, L<编号>\n` ────────────────────────
emit_beqz:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    la   a0, str_beqz
    jal  ra, emit_str
    lw   a0, 4(sp)
    jal  ra, emit_num
    la   a0, str_nl
    jal  ra, emit_str
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── emit_j：吐 `j L<编号>\n` ──────────────────────────────────
emit_j:
    addi sp, sp, -8
    sw   ra, 0(sp)
    sw   a0, 4(sp)
    la   a0, str_j
    jal  ra, emit_str
    lw   a0, 4(sp)
    jal  ra, emit_num
    la   a0, str_nl
    jal  ra, emit_str
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  代码生成：语句
# ═══════════════════════════════════════════════════════════════════════════

# ── gen_stmts：语句*（碰到 '}' 或源码末尾就回去）──────────────────
gen_stmts:
    addi sp, sp, -4
    sw   ra, 0(sp)
gen_stmts_loop:
    jal  ra, skip_ws
    jal  ra, peek
    beqz a0, gen_stmts_done
    li   t0, 125                 # '}'
    beq  a0, t0, gen_stmts_done
    li   t0, 59                  # ';'：空语句，跳过
    beq  a0, t0, gen_stmts_skip
    jal  ra, gen_stmt
    j    gen_stmts_loop
gen_stmts_skip:
    jal  ra, adv
    j    gen_stmts_loop
gen_stmts_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── gen_brace_block：吃掉 '{' … '}' 并编译里面的语句 ─────────────
gen_brace_block:
    addi sp, sp, -4
    sw   ra, 0(sp)
    jal  ra, skip_ws
    li   a0, 123                 # '{'
    jal  ra, match
    jal  ra, gen_stmts
    li   a0, 125                 # '}'
    jal  ra, match
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── gen_stmt：一条语句 ─────────────────────────────────────────
#   帧（16 字节）：sp+12 = ra，sp+4 / sp+8 = 标签编号。
#   ⚠ 变量名跨调用只能放 s2（辅助函数不碰 s0–s11）；sp+4/sp+8 谁用谁清楚。
gen_stmt:
    addi sp, sp, -16
    sw   ra, 12(sp)
    jal  ra, skip_ws
    jal  ra, peek
    beqz a0, gen_stmt_done
    li   t0, 123                 # '{'
    beq  a0, t0, gen_stmt_block
    li   t0, 105                 # 'i'：int 或 if
    beq  a0, t0, gen_stmt_i
    li   t0, 119                 # 'w'：while
    beq  a0, t0, gen_stmt_w
    j    gen_stmt_plain
gen_stmt_i:
    la   a0, kw_int
    jal  ra, check_kw
    bnez a0, gen_stmt_decl
    la   a0, kw_if
    jal  ra, check_kw
    bnez a0, gen_stmt_if
    j    gen_stmt_plain
gen_stmt_w:
    la   a0, kw_while
    jal  ra, check_kw
    bnez a0, gen_stmt_while
    j    gen_stmt_plain

# ── int 名字 = 表达式 ; ──
gen_stmt_decl:
    la   a0, kw_int
    jal  ra, eat_word
    jal  ra, skip_ws
    jal  ra, peek
    addi s2, a0, 0               # s2 = 变量名
    jal  ra, adv
    jal  ra, skip_ws
    jal  ra, getc                # 吃掉 '='
    jal  ra, gen_expr
    jal  ra, eat_semi
    j    gen_stmt_store

# ── { 语句* } ──
gen_stmt_block:
    jal  ra, gen_brace_block
    j    gen_stmt_done

# ── if (条件) { … } [else { … }] ──
gen_stmt_if:
    la   a0, kw_if
    jal  ra, eat_word
    jal  ra, skip_ws
    li   a0, 40                  # '('
    jal  ra, match
    jal  ra, gen_cond            # 条件 → a0 = 0/1
    jal  ra, next_label
    sw   a0, 4(sp)               # L<假>
    jal  ra, emit_beqz
    li   a0, 41                  # ')'
    jal  ra, match
    jal  ra, gen_brace_block     # then 分支
    jal  ra, skip_ws
    la   a0, kw_else
    jal  ra, check_kw
    beqz a0, gen_stmt_if_plain
    la   a0, kw_else
    jal  ra, eat_word
    jal  ra, next_label
    sw   a0, 8(sp)               # L<结束>
    jal  ra, emit_j
    lw   a0, 4(sp)
    jal  ra, emit_label_def      # L<假>:
    jal  ra, gen_brace_block     # else 分支
    lw   a0, 8(sp)
    jal  ra, emit_label_def      # L<结束>:
    j    gen_stmt_done
gen_stmt_if_plain:
    lw   a0, 4(sp)
    jal  ra, emit_label_def      # L<假>:
    j    gen_stmt_done

# ── while (条件) { … } ──
gen_stmt_while:
    la   a0, kw_while
    jal  ra, eat_word
    jal  ra, skip_ws
    li   a0, 40                  # '('
    jal  ra, match
    jal  ra, next_label
    sw   a0, 4(sp)               # L<头>
    jal  ra, emit_label_def
    jal  ra, gen_cond
    jal  ra, next_label
    sw   a0, 8(sp)               # L<出>
    jal  ra, emit_beqz
    li   a0, 41                  # ')'
    jal  ra, match
    jal  ra, gen_brace_block
    lw   a0, 4(sp)
    jal  ra, emit_j              # j L<头>
    lw   a0, 8(sp)
    jal  ra, emit_label_def      # L<出>:
    j    gen_stmt_done

# ── 赋值 / 表达式语句 ──
gen_stmt_plain:
    jal  ra, skip_ws
    jal  ra, peek
    addi s2, a0, 0
    addi t0, a0, -97
    li   t1, 0
    blt  t0, t1, gen_stmt_expr
    li   t1, 26
    bge  t0, t1, gen_stmt_expr
    la   t0, src
    la   t1, cur
    lw   t1, 0(t1)
    add  t0, t0, t1
    addi t0, t0, 1
gen_stmt_scan:
    lbu  t2, 0(t0)
    beqz t2, gen_stmt_expr
    li   t3, 32
    beq  t2, t3, gen_stmt_scan_next
    li   t3, 10
    beq  t2, t3, gen_stmt_scan_next
    li   t3, 13
    beq  t2, t3, gen_stmt_scan_next
    li   t3, 9
    beq  t2, t3, gen_stmt_scan_next
    li   t3, 61                  # '='
    bne  t2, t3, gen_stmt_expr
    jal  ra, adv                 # 吃掉变量名
    jal  ra, skip_ws
    jal  ra, getc                # 吃掉 '='
    jal  ra, gen_expr
    jal  ra, eat_semi
    j    gen_stmt_store
gen_stmt_scan_next:
    addi t0, t0, 1
    j    gen_stmt_scan
gen_stmt_expr:
    jal  ra, gen_expr
    jal  ra, eat_semi
    j    gen_stmt_done
gen_stmt_store:
    addi a0, s2, 0
    jal  ra, emit_la_var
    la   a0, str_sw
    jal  ra, emit_str
gen_stmt_done:
    lw   ra, 12(sp)
    addi sp, sp, 16
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  代码生成：条件（算成 0/1 放进 a0）
# ═══════════════════════════════════════════════════════════════════════════

# ── match_cmp：试吃一个比较符 → 0=`<` 1=`<=` 2=`>` 3=`>=` 4=`==` 5=`!=`
#   都没有就返回 -1（不吃任何字符）。先试两个字符的，再试一个字符的。
match_cmp:
    addi sp, sp, -4
    sw   ra, 0(sp)
    jal  ra, skip_ws
    jal  ra, peek
    li   t0, 60                  # '<'
    beq  a0, t0, match_cmp_lt
    li   t0, 62                  # '>'
    beq  a0, t0, match_cmp_gt
    li   t0, 61                  # '='
    beq  a0, t0, match_cmp_eq
    li   t0, 33                  # '!'
    beq  a0, t0, match_cmp_ne
    li   a0, -1
    j    match_cmp_done
match_cmp_lt:
    jal  ra, adv
    jal  ra, peek
    li   t0, 61
    beq  a0, t0, match_cmp_le
    li   a0, 0
    j    match_cmp_done
match_cmp_le:
    jal  ra, adv
    li   a0, 1
    j    match_cmp_done
match_cmp_gt:
    jal  ra, adv
    jal  ra, peek
    li   t0, 61
    beq  a0, t0, match_cmp_ge
    li   a0, 2
    j    match_cmp_done
match_cmp_ge:
    jal  ra, adv
    li   a0, 3
    j    match_cmp_done
match_cmp_eq:
    jal  ra, adv                 # 吃掉第一个 '='
    jal  ra, getc                # 吃掉第二个 '='
    li   a0, 4
    j    match_cmp_done
match_cmp_ne:
    jal  ra, adv                 # 吃掉 '!'
    jal  ra, getc                # 吃掉 '='
    li   a0, 5
match_cmp_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── gen_cond：条件 → a0 = 1（真）/ 0（假）──────────────────────
#   有比较符：左值压栈 → 右值 → 弹回 a1（a1=左、a0=右）→ 比较指令
#   没有比较符：表达式自己的值就是条件（非 0 即真），不用吐额外指令
gen_cond:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, gen_expr            # 左边
    jal  ra, match_cmp
    li   t0, -1
    beq  a0, t0, gen_cond_done
    sw   a0, 4(sp)               # 运算符编号（右边一算就会被冲掉）
    la   a0, str_push
    jal  ra, emit_str
    jal  ra, gen_expr            # 右边
    la   a0, str_pop
    jal  ra, emit_str
    lw   a0, 4(sp)
    li   t0, 0
    beq  a0, t0, gen_cond_lt
    li   t0, 1
    beq  a0, t0, gen_cond_le
    li   t0, 2
    beq  a0, t0, gen_cond_gt
    li   t0, 3
    beq  a0, t0, gen_cond_ge
    li   t0, 4
    beq  a0, t0, gen_cond_eq
    la   a0, str_sne             # 只剩 '!='
    j    gen_cond_emit
gen_cond_lt:
    la   a0, str_slt_l           # 左 < 右
    j    gen_cond_emit
gen_cond_le:
    la   a0, str_sle             # 左 <= 右 = !(右 < 左)
    j    gen_cond_emit
gen_cond_gt:
    la   a0, str_slt_r           # 左 > 右 = 右 < 左
    j    gen_cond_emit
gen_cond_ge:
    la   a0, str_sge             # 左 >= 右 = !(左 < 右)
    j    gen_cond_emit
gen_cond_eq:
    la   a0, str_seq             # 左 == 右 = (左 - 右) == 0
gen_cond_emit:
    jal  ra, emit_str
gen_cond_done:
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ═══════════════════════════════════════════════════════════════════════════
#  代码生成：表达式（和 4-8 一字不差）
# ═══════════════════════════════════════════════════════════════════════════

# ── gen_expr：expr := term (('+' | '-') term)* ──────────────────
gen_expr:
    addi sp, sp, -4
    sw   ra, 0(sp)
    jal  ra, gen_term
gen_expr_loop:
    jal  ra, skip_ws
    jal  ra, peek
    li   t0, 43                 # '+'
    beq  a0, t0, gen_expr_add
    li   t0, 45                 # '-'
    beq  a0, t0, gen_expr_sub
    j    gen_expr_done
gen_expr_add:
    la   a0, str_push
    jal  ra, emit_str
    jal  ra, getc               # 吃掉 '+'
    jal  ra, gen_term
    la   a0, str_pop
    jal  ra, emit_str
    la   a0, str_add
    jal  ra, emit_str
    j    gen_expr_loop
gen_expr_sub:
    la   a0, str_push
    jal  ra, emit_str
    jal  ra, getc               # 吃掉 '-'
    jal  ra, gen_term
    la   a0, str_pop
    jal  ra, emit_str
    la   a0, str_sub
    jal  ra, emit_str
    j    gen_expr_loop
gen_expr_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── gen_term：term := unary (('*' | '/') unary)* ────────────────
gen_term:
    addi sp, sp, -4
    sw   ra, 0(sp)
    jal  ra, gen_unary
gen_term_loop:
    jal  ra, skip_ws
    jal  ra, peek
    li   t0, 42                 # '*'
    beq  a0, t0, gen_term_mul
    li   t0, 47                 # '/'
    beq  a0, t0, gen_term_div
    j    gen_term_done
gen_term_mul:
    la   a0, str_push
    jal  ra, emit_str
    jal  ra, getc
    jal  ra, gen_unary
    la   a0, str_pop
    jal  ra, emit_str
    la   a0, str_mul
    jal  ra, emit_str
    j    gen_term_loop
gen_term_div:
    la   a0, str_push
    jal  ra, emit_str
    jal  ra, getc
    jal  ra, gen_unary
    la   a0, str_pop
    jal  ra, emit_str
    la   a0, str_div
    jal  ra, emit_str
    j    gen_term_loop
gen_term_done:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# ── gen_unary：unary := '-' unary | '(' expr ')' | 数字 | 名字 ───
#   ⚠ 变量名放本函数的帧里（sp+4），不能放 s2：调用方 gen_stmt 正用 s2
#     记着"这条语句要写回哪个变量"。
gen_unary:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, skip_ws
    jal  ra, peek
    li   t0, 40                 # '('
    beq  a0, t0, gen_unary_paren
    li   t0, 45                 # '-'
    beq  a0, t0, gen_unary_neg
    li   t0, 48                 # '0'
    blt  a0, t0, gen_unary_name
    li   t0, 58                 # '9' + 1
    blt  a0, t0, gen_unary_num
gen_unary_name:
    li   t0, 97                 # 'a'
    blt  a0, t0, gen_unary_done
    li   t0, 123                # 'z' + 1
    bge  a0, t0, gen_unary_done
    sw   a0, 4(sp)
    jal  ra, adv
    lw   a0, 4(sp)
    jal  ra, emit_la_var
    la   a0, str_lw
    jal  ra, emit_str
    j    gen_unary_done
gen_unary_num:
    jal  ra, getnum
    sw   a0, 4(sp)
    la   a0, str_li
    jal  ra, emit_str
    lw   a0, 4(sp)
    jal  ra, emit_num
    la   a0, str_nl
    jal  ra, emit_str
    j    gen_unary_done
gen_unary_paren:
    jal  ra, getc               # 吃掉 '('
    jal  ra, gen_expr
    jal  ra, getc               # 吃掉 ')'
    j    gen_unary_done
gen_unary_neg:
    jal  ra, getc               # 吃掉 '-'
    jal  ra, gen_unary
    la   a0, str_neg
    jal  ra, emit_str
gen_unary_done:
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret
