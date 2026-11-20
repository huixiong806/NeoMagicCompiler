# ═══════════════════════════════════════════════════════════════════════════
# 4-10 参考解：星咒 → 星之语 的编译器（函数与调用这一层）
#
# 输入：星咒源码。从输入端口 0xF0000004 逐字符读，读到 0 表示源码结束。
# 输出：星之语汇编文本。逐字符写到显示端口（a7=1 的 ecall），用 '\n' 分行。
#
# 支持的星咒子集（本关只要这一小套）：
#   函数      int 名字() { return 表达式; }        （无参函数，可以有任意多个）
#   表达式    整数、名字()（无参调用）、- 一元负号、括号
#             + - * / %                            （乘除模比加减紧）
#   本关不涉及变量与分支，所以生成的目标代码里没有 .data，也没有标签跳转
#   （只有函数自己的结束标签）。
#
# 生成的目标代码（这一关按文本比对，格式就是下面这套）：
#   .text                  ← 整份输出只吐一次，在第一行
#   名字:                   ← 每个函数一个标签，顺序和源码一致
#   addi sp, sp, -16        ← 序言：给 ra 腾出位置
#   sw ra, 12(sp)           ←         并把 ra 存进去（调用别人会覆盖 ra）
#   ……函数体……
#   L<结束标签>:
#   lw ra, 12(sp)           ← 收尾：把 ra 取回来
#   addi sp, sp, 16
#   ret
#
#   return 表达式;  → 值算进 a0，再 j 到本函数的收尾段
#   名字()          → jal ra, 名字
#   数字            → li a0, 数字
#   一元负号        → sub a0, x0, a0
#   二元运算        → 左边算进 a0 → 压栈 → 右边算进 a0 → 弹回 a1 → 运算
#                   （右边可能是一次调用，调用会把 a0 冲掉，所以左边必须压栈）
#
#   结束标签从 1 开始按函数出现顺序发：第一个函数 L1，第二个 L2……
#   main 也用 ret 结束，绝不用 ecall。
#
# 寄存器约定（本程序自己定的）：
#   a0-a2 / t0-t6 随用随扔；凡是跨过一次 jal 还要用的中间结果，
#   一律放自己的栈帧里；每个非叶子的子程序都在栈上保存 ra。
# ═══════════════════════════════════════════════════════════════════════════

# ── 编译器自己的数据（放在 0x2000 之后的安全区）──────────────────────────
        .data
        .org 0x2000
CH:       .word 0              # 当前还没消费的字符（0 = 输入结束）
FIRSTF:   .word 0              # 有没有输出过 ".text"
FNEND:    .word 0              # 当前函数的"结束标签"编号
LABNO:    .word 0              # 标签计数器
IDBUF:    .space 16            # 最近读到的名字

# ── 生成模板（都是 NUL 结尾的字符串，用 emit_str 打出去）──────────────────
T_PRE:    .asciz ".text\n"
T_PRO:    .asciz "addi sp, sp, -16\nsw ra, 12(sp)\n"
T_EPI:    .asciz "lw ra, 12(sp)\naddi sp, sp, 16\nret\n"
T_COLON:  .asciz ":\n"
T_PUSH:   .asciz "addi sp, sp, -4\nsw a0, 0(sp)\n"
T_POP:    .asciz "lw a1, 0(sp)\naddi sp, sp, 4\n"
T_LI:     .asciz "li a0, "
T_NEG:    .asciz "sub a0, x0, a0\n"
T_ADD:    .asciz "add a0, a1, a0\n"
T_SUB:    .asciz "sub a0, a1, a0\n"
T_MUL:    .asciz "mul a0, a1, a0\n"
T_DIV:    .asciz "div a0, a1, a0\n"
T_REM:    .asciz "rem a0, a1, a0\n"
T_JAL:    .asciz "jal ra, "
T_JL:     .asciz "j L"
T_LAB:    .asciz "L"
T_NL:     .asciz "\n"

# ── 关键字 ────────────────────────────────────────────────────────────────
K_INT:    .asciz "int"
K_RET:    .asciz "return"

        .text

# ─────────────────────────────────────────────────────────────────────────
#  主循环：反复读一个 "int 名字() { return 表达式; }"，直到输入结束
# ─────────────────────────────────────────────────────────────────────────
        jal  ra, advance                   # 预读第一个字符

top:
        jal  ra, skip_ws
        la   t0, CH
        lw   a0, 0(t0)
        beqz a0, finish                # 源码读完了
        jal  ra, id_take               # 读一个词
        la   a0, IDBUF
        la   a1, K_INT
        jal  ra, str_eq
        beqz a0, finish                # 不是 int 开头，就没有更多函数了
        jal  ra, compile_func
        j    top

finish:
        li   a0, 0
        li   a7, 0
        ecall                          # 编译器自己结束

# ─────────────────────────────────────────────────────────────────────────
#  compile_func：编译一个函数定义
# ─────────────────────────────────────────────────────────────────────────
compile_func:
        addi sp, sp, -8
        sw   ra, 4(sp)
        # 注意：main 上面的主循环已经把 "int" 这个词吃掉并核对过了，
        # 所以这里直接读函数名。
        jal  ra, id_take                   # 函数名 → IDBUF

        # 第一次真的遇到函数时，先把 ".text" 吐出去
        la   t0, FIRSTF
        lw   t1, 0(t0)
        bnez t1, cf_nopre
        li   t1, 1
        sw   t1, 0(t0)
        la   a0, T_PRE
        jal  ra, emit_str
cf_nopre:
        la   a0, IDBUF                 # "函数名" + ":"
        jal  ra, emit_str
        la   a0, T_COLON
        jal  ra, emit_str
        la   a0, T_PRO                 # 序言：腾出 16 字节并保存 ra
        jal  ra, emit_str

        jal  ra, next_label                # 给这个函数分配结束标签
        la   t0, FNEND
        sw   a0, 0(t0)

        li   a0, '('                   # 无参函数：()
        jal  ra, expect_ch
        li   a0, ')'
        jal  ra, expect_ch
        li   a0, '{'
        jal  ra, expect_ch

cf_body:
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, cf_end                # 源码突然结束
        li   t2, '}'
        beq  t1, t2, cf_close
        jal  ra, compile_stmt
        j    cf_body
cf_close:
        jal  ra, advance                   # 吃掉 '}'

cf_end:
        la   t0, FNEND
        lw   a0, 0(t0)
        jal  ra, emit_label                # 输出 "L<结束>:"
        la   a0, T_EPI                 # 收尾：恢复 ra、收回栈、ret
        jal  ra, emit_str
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_stmt：本关的函数体里只有 return
# ─────────────────────────────────────────────────────────────────────────
compile_stmt:
        addi sp, sp, -8
        sw   ra, 4(sp)
        jal  ra, id_take                   # 取一个词
        la   a0, IDBUF
        la   a1, K_RET
        jal  ra, str_eq
        bnez a0, cs_ret
        jal  ra, advance                   # 不是 return：吃掉一个字符往前走
        j    cs_done
cs_ret:
        jal  ra, compile_add               # 返回值算进 a0
        la   t0, FNEND
        lw   a0, 0(t0)
        jal  ra, emit_j2                   # j 到函数尾的收尾段
        li   a0, ';'
        jal  ra, expect_ch
cs_done:
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_add：加减（左结合）；本关它是表达式的最外层
# ─────────────────────────────────────────────────────────────────────────
compile_add:
        addi sp, sp, -16
        sw   ra, 12(sp)
        jal  ra, compile_mul
ad_loop:
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '+'
        beq  t1, t2, ad_add
        li   t2, '-'
        beq  t1, t2, ad_sub
        j    ad_done
ad_add:
        jal  ra, advance
        la   t0, T_ADD
        j    ad_bin
ad_sub:
        jal  ra, advance
        la   t0, T_SUB
ad_bin:
        addi sp, sp, -8
        sw   t0, 0(sp)
        la   a0, T_PUSH                # 左边先压栈
        jal  ra, emit_str
        jal  ra, compile_mul
        la   a0, T_POP                 # 右边算完，弹回 a1
        jal  ra, emit_str
        lw   a0, 0(sp)
        addi sp, sp, 8
        jal  ra, emit_str
        j    ad_loop
ad_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_mul：乘除模（比加减紧）
# ─────────────────────────────────────────────────────────────────────────
compile_mul:
        addi sp, sp, -16
        sw   ra, 12(sp)
        jal  ra, compile_atom
cm_loop:
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '*'
        beq  t1, t2, cm_mul
        li   t2, '/'
        beq  t1, t2, cm_div
        li   t2, '%'
        beq  t1, t2, cm_rem
        j    cm_done
cm_mul:
        jal  ra, advance
        la   t0, T_MUL
        j    cm_bin
cm_div:
        jal  ra, advance
        la   t0, T_DIV
        j    cm_bin
cm_rem:
        jal  ra, advance
        la   t0, T_REM
cm_bin:
        addi sp, sp, -8
        sw   t0, 0(sp)
        la   a0, T_PUSH
        jal  ra, emit_str
        jal  ra, compile_atom
        la   a0, T_POP
        jal  ra, emit_str
        lw   a0, 0(sp)
        addi sp, sp, 8
        jal  ra, emit_str
        j    cm_loop
cm_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_atom：数、名字() 调用、- 负号、括号
# ─────────────────────────────────────────────────────────────────────────
compile_atom:
        addi sp, sp, -16
        sw   ra, 12(sp)
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, ca_zero               # 源码没了：当成 0
        li   t2, '('
        beq  t1, t2, ca_paren
        li   t2, '-'
        beq  t1, t2, ca_neg
        li   t2, '0'
        blt  t1, t2, ca_alpha
        li   t2, '9'
        bge  t2, t1, ca_num
ca_alpha:
        li   t2, '_'
        beq  t1, t2, ca_name
        li   t2, 'a'
        blt  t1, t2, ca_up
        li   t2, 'z'
        bge  t2, t1, ca_name
ca_up:
        li   t2, 'A'
        blt  t1, t2, ca_zero
        li   t2, 'Z'
        blt  t2, t1, ca_zero
        j    ca_name
ca_num:
        jal  ra, parse_num
        jal  ra, emit_li                   # li a0, <数>
        j    ca_done
ca_paren:
        jal  ra, advance                   # 吃掉 '('
        jal  ra, compile_add
        li   a0, ')'
        jal  ra, expect_ch
        j    ca_done
ca_neg:
        jal  ra, advance                   # 吃掉 '-'
        jal  ra, compile_atom
        la   a0, T_NEG
        jal  ra, emit_str
        j    ca_done
ca_name:
        jal  ra, id_take                   # 函数名 → IDBUF
        li   a0, '('
        jal  ra, expect_ch                 # 无参调用：吃后面的 ')'
        li   a0, ')'
        jal  ra, expect_ch
        la   a0, T_JAL
        jal  ra, emit_str
        la   a0, IDBUF                 # jal ra, 名字
        jal  ra, emit_str
        la   a0, T_NL
        jal  ra, emit_str
        j    ca_done
ca_zero:
        li   a0, 0
        jal  ra, emit_li
ca_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# ═════════════════════════════════════════════════════════════════════════
#  生成端的小工具
# ═════════════════════════════════════════════════════════════════════════

# emit_li：li a0, <a0>
emit_li:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        la   a0, T_LI
        jal  ra, emit_str
        lw   a0, 0(sp)
        jal  ra, emit_int
        la   a0, T_NL
        jal  ra, emit_str
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# emit_label：L<a0>:
emit_label:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        la   a0, T_LAB
        jal  ra, emit_str
        lw   a0, 0(sp)
        jal  ra, emit_int
        la   a0, T_COLON
        jal  ra, emit_str
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# emit_j2：j L<a0>
emit_j2:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        la   a0, T_JL
        jal  ra, emit_str
        lw   a0, 0(sp)
        jal  ra, emit_int
        la   a0, T_NL
        jal  ra, emit_str
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# emit_int：把 a0（十进制、可负）打出去
emit_int:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        bgez a0, ei_pos
        li   a0, 45                    # 减号的编码
        jal  ra, emit_char
        lw   a0, 0(sp)
        neg  a0, a0
ei_pos:
        li   t0, 10
        blt  a0, t0, ei_small
        sw   a0, 0(sp)
        li   t0, 10
        div  a0, a0, t0                # 先递归打出高位
        jal  ra, emit_int
        lw   a0, 0(sp)
        li   t0, 10
        rem  a0, a0, t0                # 再打最后一位
ei_small:
        addi a0, a0, 48                # '0'
        jal  ra, emit_char
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# emit_str：把 a0 指向的 NUL 结尾字符串打出去（只用 t3、a0、a7）
emit_str:
        mv   t3, a0
es_loop:
        lbu  a0, 0(t3)
        beqz a0, es_done
        li   a7, 1
        ecall
        addi t3, t3, 1
        j    es_loop
es_done:
        ret

# emit_char：把 a0 的低 8 位当字符打出去
emit_char:
        li   a7, 1
        ecall
        ret

# next_label：发一个新标签号（从 1 开始）
next_label:
        la   t0, LABNO
        lw   t1, 0(t0)
        addi t1, t1, 1
        sw   t1, 0(t0)
        mv   a0, t1
        ret

# ═════════════════════════════════════════════════════════════════════════
#  读源码的小工具
# ═════════════════════════════════════════════════════════════════════════

# advance：从输入端口读下一个字符放进 CH（读空得到 0，就是输入结束）
# 只用 t0、t1
advance:
        li   t0, 0xF0000004
        lw   t1, 0(t0)
        la   t0, CH
        sw   t1, 0(t0)
        ret

# skip_ws：跳过空白（空格、制表、换行、回车）
skip_ws:
        addi sp, sp, -8
        sw   ra, 4(sp)
sw_loop:
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, sw_done
        li   t2, 32
        beq  t1, t2, sw_step
        li   t2, 9
        beq  t1, t2, sw_step
        li   t2, 10
        beq  t1, t2, sw_step
        li   t2, 13
        beq  t1, t2, sw_step
        j    sw_done
sw_step:
        jal  ra, advance
        j    sw_loop
sw_done:
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# expect_ch：跳过空白；如果 CH 正好是 a0 给的那个字符，就吃掉它
expect_ch:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        lw   t2, 0(sp)
        bne  t1, t2, ec_done
        jal  ra, advance
ec_done:
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# id_take：读一个词到 IDBUF（字母/数字/下划线，最多 15 个字符）
id_take:
        addi sp, sp, -8
        sw   ra, 4(sp)
        jal  ra, skip_ws
        la   t3, IDBUF
        li   t4, 0                     # 已经收了几个字符
        li   t5, 15
ir_loop:
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, ir_done
        bge  t4, t5, ir_done
        li   t2, '_'
        beq  t1, t2, ir_take
        li   t2, '0'
        blt  t1, t2, ir_alpha
        li   t2, '9'
        bge  t2, t1, ir_take
ir_alpha:
        li   t2, 'a'
        blt  t1, t2, ir_upper
        li   t2, 'z'
        bge  t2, t1, ir_take
ir_upper:
        li   t2, 'A'
        blt  t1, t2, ir_done
        li   t2, 'Z'
        blt  t2, t1, ir_done
ir_take:
        sb   t1, 0(t3)
        addi t3, t3, 1
        addi t4, t4, 1
        jal  ra, advance
        j    ir_loop
ir_done:
        sb   x0, 0(t3)
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# parse_num：CH 是数字，读出这个十进制整数 → a0（t3 在 advance 里不会被碰）
parse_num:
        addi sp, sp, -8
        sw   ra, 4(sp)
        li   t3, 0
pn_loop:
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '0'
        blt  t1, t2, pn_done
        li   t2, '9'
        blt  t2, t1, pn_done
        li   t2, 10
        mul  t3, t3, t2
        addi t1, t1, -48
        add  t3, t3, t1
        jal  ra, advance
        j    pn_loop
pn_done:
        mv   a0, t3
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# str_eq：a0 和 a1 两个 NUL 结尾字符串相等 → a0 = 1，否则 0
# 只改 t0、t1、a0、a1
str_eq:
se_loop:
        lbu  t0, 0(a0)
        lbu  t1, 0(a1)
        bne  t0, t1, se_no
        beqz t0, se_yes
        addi a0, a0, 1
        addi a1, a1, 1
        j    se_loop
se_yes:
        li   a0, 1
        ret
se_no:
        li   a0, 0
        ret
