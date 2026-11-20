# ═══════════════════════════════════════════════════════════════════════════
# 4-7 参考解：把常量表达式编译成星之语
#
# 玩家写的是一台**编译器**（程序模式）：
#   输入：星咒源码文本（从 0xF0000004 逐字符读，读到 0 表示输入结束）
#   输出：星之语汇编文本（逐字符写到显示端口 0xF0000000）
#
# 本关语法（只有常量、括号和 + - * /）：
#   expr  := term (('+' | '-') term)*
#   term  := unary (('*' | '/') unary)*
#   unary := '-' unary | '(' expr ')' | 数字
#
# 生成代码的约定（4-8 ~ 4-12 一路沿用，和 4-10/4-11/4-12 逐字一致）：
#   .text                 ← 整份输出只吐一次
#   main:                 ← 入口标签
#   ……表达式的代码……        ← 算完时结果在 a0 里，栈是平的
#   ret                   ← 用 ret 返回（不要 ecall：外面的人 jalr 进来、读 a0）
#
#   数字       li a0, N
#   一元负号   <操作数> 之后 sub a0, x0, a0
#   括号       只让里面先算，自己不吐指令
#   二元运算   左边算进 a0 →
#              addi sp, sp, -4
#              sw a0, 0(sp)         ← 左值压栈
#              右边算进 a0
#              lw a1, 0(sp)
#              addi sp, sp, 4       ← 弹回 a1
#              add a0, a1, a0       ← 运算（左 op 右）
#   指令一律不缩进、不带注释，一行一条，行尾都是 \n。
#
#   为什么左值要压栈、而不是换个寄存器放着？因为 4-10 起右边可能是一次
#   函数调用，调用会把 a0 冲掉；放在栈（记忆晶阵）上谁也碰不到。
#
# 记忆晶阵布局：
#   src/out/tinstr：源码、输出缓冲、emit_num 的十进制小缓冲
#   cur：源码游标；wc：当前位置的字符（0 = 源码到头）
# ═══════════════════════════════════════════════════════════════════════════

.data
src:        .space 1024
out:        .space 8192
tinstr:     .space 16
cur:        .word 0
wc:         .word 0
str_head:   .asciz ".text\nmain:\n"
str_li:     .asciz "li a0, "
str_nl:     .asciz "\n"
str_push:   .asciz "addi sp, sp, -4\nsw a0, 0(sp)\n"
str_pop:    .asciz "lw a1, 0(sp)\naddi sp, sp, 4\n"
str_add:    .asciz "add a0, a1, a0\n"
str_sub:    .asciz "sub a0, a1, a0\n"
str_mul:    .asciz "mul a0, a1, a0\n"
str_div:    .asciz "div a0, a1, a0\n"
str_neg:    .asciz "sub a0, x0, a0\n"
str_ret:    .asciz "ret\n"

.text
# ── 入口：读源码 → 编译 → 把生成的汇编文本送到显示端口 ─────────────
compile_entry:
    # 1) 把输入字符流读进 src（读到 0 表示输入结束）
    la   a1, src
    li   a2, 0xF0000004
read_loop:
    lw   a0, 0(a2)
    beqz a0, read_done
    sb   a0, 0(a1)
    addi a1, a1, 1
    j    read_loop
read_done:
    sb   x0, 0(a1)              # 末尾补 0，当结束标记

    # 2) 词法游标归零，把第一个字符读进 wc
    jal  ra, reset_lex

    # 3) 吐出编译器固定要写的那两行（.text / main:）
    la   a0, str_head
    jal  ra, emit_str

    # 4) 编译表达式（生成的代码算完时结果在 a0 里）
    jal  ra, gen_expr

    # 5) 收尾
    la   a0, str_ret
    jal  ra, emit_str

    # 6) 把 out 里的汇编文本逐字符写到显示端口
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
#  拼文本：emit_str / emit_num
# ═══════════════════════════════════════════════════════════════════════════

# ── emit_str：把 a0 指向的 NUL 结尾字符串接到 out 末尾（a0 会走到底）──
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
    sb   x0, 0(t0)              # 接完补 0，下次还能从头上找
    ret

# ── emit_num：把 a0 里的非负整数按十进制接到 out 末尾（a0 被破坏）──
#   数字从 tinstr 的尾巴往前填；tinstr 有 16 字节，够 11 位十进制。
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
    sb   x0, 0(t2)              # 结尾的 0 放在最后一格（不出界）
    addi t2, t2, -1             # 从倒数第二格往前填
    li   t3, 10
    bnez a0, emit_num_digits
    li   t4, 48                 # 值是 0 也得写一个 '0'
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
#  词法：peek（看）/ adv（吃掉一个字符）/ skip_ws / getc / getnum
#  约定：wc 里永远是"当前位置的字符"，0 表示源码到头。
#  ⚠ adv 只动 t0–t3，a0 不碰 —— getc、match 这些函数就靠这一点。
# ═══════════════════════════════════════════════════════════════════════════

# ── reset_lex：游标归零，并把第一个字符读进 wc ──────────────────
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
    sw   t3, 0(t0)              # wc 高 24 位一直是 0（.word 0 + 只写低字节）
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

# ── skip_ws：跳过空格 / 换行 / 回车 / 制表 ──────────────────────
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

# ── getc：跳过空白后，返回当前字符并**吃掉**它（源码到头返回 0）────
getc:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, skip_ws
    la   t6, wc
    lw   t5, 0(t6)              # t5 先拿着这个字符（adv 不碰 t5）
    beqz t5, getc_done
    jal  ra, adv
getc_done:
    addi a0, t5, 0
    lw   ra, 0(sp)
    addi sp, sp, 8
    ret

# ── getnum：把一串十进制数字读成一个整数（游标停在数字后面）────────
getnum:
    addi sp, sp, -8
    sw   ra, 0(sp)
    jal  ra, skip_ws
    li   t4, 0                  # 累加器（adv 碰不到 t4/t5）
    li   t5, 10
getnum_loop:
    la   t6, wc
    lw   t1, 0(t6)
    li   t0, 48                 # '0'
    blt  t1, t0, getnum_done
    li   t0, 58                 # '9' + 1
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
#  代码生成：表达式
#  每个 gen_* 都是"一边看一边吐"：它往 out 里拼的文本，就是生成程序的样子。
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
    la   a0, str_push           # 左边已经算完了：先压栈
    jal  ra, emit_str
    jal  ra, getc               # 吃掉 '+'
    jal  ra, gen_term           # 右边算进 a0
    la   a0, str_pop            # 弹回 a1
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

# ── gen_unary：unary := '-' unary | '(' expr ')' | 数字 ─────────
#   帧：sp+0 放 ra，sp+4 放本函数自己的暂存（数字字面量）
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
    blt  a0, t0, gen_unary_done
    li   t0, 58                 # '9' + 1
    bge  a0, t0, gen_unary_done
    # 数字：吐两行 —— "li a0, " + 数字 + 换行
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
    jal  ra, gen_expr           # 括号只是"让里面先算"，自己不吐指令
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
