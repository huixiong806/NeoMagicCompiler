# ═══════════════════════════════════════════════════════════════════════════
# 4-12 参考解：同一个编译器，用来编译墙上那段"封印程序"
#
# 说明：这一关的参考解就是 4-11 那台编译器（逐字相同），没有新增任何能力。
# 终章要证明的事情不是"再写一台机器"，而是"这台机器能编译伊塔尔留下的那段"：
# 那段程序里有 if（不带 else）与 ==，正好把 4-9 学的分支也一起用上了。
#
# 输入：星咒源码。从输入端口 0xF0000004 逐字符读，读到 0 表示源码结束。
# 输出：星之语汇编文本。逐字符写到显示端口（a7=1 的 ecall），用 '\n' 分行。
#
# 支持的星咒子集（4-7 ~ 4-10 教过的那一套）：
#   函数      int 名字() { 语句… }          （无参函数；不做递归）
#   语句      int 名字 [= 表达式] ;         （声明，初值可有可无）
#             名字 = 表达式 ;               （赋值）
#             if (表达式) 语句 [else 语句]
#             while (表达式) 语句
#             { 语句… }
#             return 表达式 ;
#   表达式    整数、变量、名字()（无参调用）、- 一元负号、括号
#             + - * / %                     （乘除模比加减紧）
#             < > <= >= == !=               （一个表达式里最多一个，用来做条件）
#
# 生成的目标代码（沿用 4-7 ~ 4-9 那套写法；本关的判定会把它交给真汇编器
# 执行，所以必须真的能跑）：
#   .text                  ← 先吐代码
#   名字:                   ← 每个函数一个标签
#   addi sp, sp, -16        ← 先给 ra 腾出位置（调用别人会覆盖 ra）
#   sw ra, 12(sp)
#   ……函数体……
#   L<这个函数的结束标签>:
#   lw ra, 12(sp)
#   addi sp, sp, 16
#   ret                     ← main 也用 ret 结束（返回地址是判定器给的）
#   .data                   ← 变量声明补在最后
#   变量名: .word 0
#
#   为什么 .data 在最后？因为我们是"一边读一边吐"的单遍编译器：
#   只有整个程序读完，才知道一共出现过哪些变量。
#
#   return 表达式;  → 把值算进 a0，再 j 到函数尾的收尾段
#   a()             → jal ra, a
#   变量            → la a1, 名字 之后 lw a0, 0(a1) / sw a0, 0(a1)
#   二元运算        → 左边算完压栈、右边算完弹回 a1：
#                     addi sp, sp, -4 / sw a0, 0(sp)   ……算右边……
#                     lw a1, 0(sp) / addi sp, sp, 4 / add a0, a1, a0
#                   （右边可能是一次调用，调用会把 a0 冲掉，所以左边必须压栈）
# ═══════════════════════════════════════════════════════════════════════════
#
# 寄存器约定（本程序自己定的）：
#   a0-a2 / t0-t6 随用随扔；凡是跨过一次 jal 还要用的中间结果，
#   一律放自己的栈帧里；每个非叶子的子程序都在栈上保存 ra。
# ═══════════════════════════════════════════════════════════════════════════

# ── 编译器自己的数据（放在 0x2000 之后的安全区）──────────────────────────
        .data
        .org 0x2000
CH:       .word 0              # 当前还没消费的字符（0 = 输入结束）
PEEKD:    .word 0              # IDBUF 里是不是已经暂存了一个词
FIRSTF:   .word 0              # 有没有输出过开头那一行
FNEND:    .word 0              # 当前函数的"结束标签"编号
LABNO:    .word 0              # 标签计数器
NVARS:    .word 0              # 已经登记了几个变量
IDBUF:    .space 16            # 最近读到的名字
CURVAR:   .space 16            # 正在赋值/声明的那个变量名
VARNAMES: .space 256           # 变量名表：每项 16 字节，最多 16 个变量

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
T_SLT:    .asciz "slt a0, a1, a0\n"
T_SGT:    .asciz "slt a0, a0, a1\n"
T_SLE:    .asciz "slt a0, a0, a1\nxori a0, a0, 1\n"
T_SGE:    .asciz "slt a0, a1, a0\nxori a0, a0, 1\n"
T_SEQ:    .asciz "sub a0, a1, a0\nseqz a0, a0\n"
T_SNE:    .asciz "sub a0, a1, a0\nsnez a0, a0\n"
T_JAL:    .asciz "jal ra, "
T_LA1:    .asciz "la a1, "
T_LW:     .asciz "lw a0, 0(a1)\n"
T_SW:     .asciz "sw a0, 0(a1)\n"
T_DATAS:  .asciz ".data\n"
T_ZERO:   .asciz ": .word 0\n"
T_BEQZ:   .asciz "beqz a0, L"
T_JL:     .asciz "j L"
T_LAB:    .asciz "L"
T_NL:     .asciz "\n"

# ── 关键字（拿来和读到的词比较）───────────────────────────────────────────
K_INT:    .asciz "int"
K_IF:     .asciz "if"
K_ELSE:   .asciz "else"
K_WHILE:  .asciz "while"
K_RET:    .asciz "return"

        .text

# ─────────────────────────────────────────────────────────────────────────
#  主循环：反复读一个 "int 名字() { … }"，直到输入结束
# ─────────────────────────────────────────────────────────────────────────
        jal  ra, advance                   # 预读第一个字符

top:
        jal  ra, skip_ws
        la   t0, CH
        lw   a0, 0(t0)
        beqz a0, finish                # 源码读完了
        jal  ra, id_peek                   # 先看一眼下一个词
        la   a0, IDBUF
        la   a1, K_INT
        jal  ra, str_eq
        beqz a0, finish                # 不是 int 开头，就没有更多函数了
        jal  ra, compile_func
        j    top

finish:
        jal  ra, emit_decls                # 最后补上 .data 里的变量声明
        li   a0, 0
        li   a7, 0
        ecall                          # 编译器自己结束

# ─────────────────────────────────────────────────────────────────────────
#  compile_func：编译一个函数定义
# ─────────────────────────────────────────────────────────────────────────
compile_func:
        addi sp, sp, -8
        sw   ra, 4(sp)
        jal  ra, id_take                   # 吃掉 "int"
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
        la   t0, PEEKD                 # 有暂存的词就先处理它
        lw   t1, 0(t0)
        bnez t1, cf_stmt
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, cf_end                # 源码突然结束
        li   t2, '}'
        beq  t1, t2, cf_close
cf_stmt:
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
#  compile_stmt：编译一条语句
# ─────────────────────────────────────────────────────────────────────────
compile_stmt:
        addi sp, sp, -16
        sw   ra, 12(sp)
        la   t0, PEEKD
        lw   t1, 0(t0)
        bnez t1, cs_disp               # 已经有暂存的词，直接分派
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '{'
        beq  t1, t2, cs_block

cs_disp:
        jal  ra, id_take                   # 取一个词 → IDBUF
        la   a0, IDBUF
        la   a1, K_RET
        jal  ra, str_eq
        bnez a0, cs_ret
        la   a0, IDBUF
        la   a1, K_IF
        jal  ra, str_eq
        bnez a0, cs_if
        la   a0, IDBUF
        la   a1, K_WHILE
        jal  ra, str_eq
        bnez a0, cs_while
        la   a0, IDBUF
        la   a1, K_INT
        jal  ra, str_eq
        bnez a0, cs_decl
        j    cs_assign                  # 剩下的情况：当成"变量 = 表达式 ;"

# ── return 表达式 ; ──
cs_ret:
        jal  ra, compile_rel               # 返回值算进 a0
        la   t0, FNEND
        lw   a0, 0(t0)
        jal  ra, emit_j2                   # j 到函数尾的收尾段
        li   a0, ';'
        jal  ra, expect_ch
        j    cs_done

# ── int 名字 [= 表达式] ; ──
cs_decl:
        jal  ra, id_take                   # 变量名 → IDBUF
        la   a0, CURVAR                # 这个名字后面还要用（表达式里会读别的词），
        la   a1, IDBUF                 # 先抄一份到 CURVAR
        jal  ra, str_copy
        la   a0, IDBUF
        jal  ra, var_add                   # 登记变量（登记过就什么都不做）
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '='
        bne  t1, t2, cs_decl_end
        jal  ra, advance                   # 吃掉 '='
        jal  ra, compile_rel               # 初值算进 a0
        jal  ra, emit_store                # 存进 CURVAR 那个变量
cs_decl_end:
        li   a0, ';'
        jal  ra, expect_ch
        j    cs_done

# ── 变量 = 表达式 ; ──
cs_assign:
        la   a0, CURVAR                # 变量名 → CURVAR
        la   a1, IDBUF
        jal  ra, str_copy
        la   a0, IDBUF
        jal  ra, var_lookup                # 没登记过的变量就不认
        bltz a0, cs_bad
        li   a0, '='
        jal  ra, expect_ch
        jal  ra, compile_rel
        jal  ra, emit_store
        li   a0, ';'
        jal  ra, expect_ch
        j    cs_done
cs_bad:
        jal  ra, advance                   # 不认识的语句：吃掉一个字符，保证往前走
        j    cs_done

# ── { 语句… } ──
cs_block:
        jal  ra, advance                   # 吃掉 '{'
cs_blk:
        jal  ra, skip_ws
        la   t0, PEEKD
        lw   t1, 0(t0)
        bnez t1, cs_blk_stmt
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, cs_done               # 源码突然结束
        li   t2, '}'
        beq  t1, t2, cs_blk_end
cs_blk_stmt:
        jal  ra, compile_stmt
        j    cs_blk
cs_blk_end:
        jal  ra, advance                   # 吃掉 '}'
        j    cs_done

# ── if (表达式) 语句 [else 语句] ──
cs_if:
        li   a0, '('
        jal  ra, expect_ch
        jal  ra, compile_rel               # 条件算进 a0
        li   a0, ')'
        jal  ra, expect_ch
        jal  ra, next_label                # "条件不成立时"跳到的标签
        addi sp, sp, -8
        sw   a0, 0(sp)
        jal  ra, emit_beqz                 # beqz a0, L<这个标签>
        jal  ra, compile_stmt              # then 分支
        jal  ra, id_peek                   # 后面跟的是不是 else？
        la   a0, IDBUF
        la   a1, K_ELSE
        jal  ra, str_eq
        beqz a0, cs_if_plain
        jal  ra, id_take                   # 吃掉 "else"
        jal  ra, next_label                # then 做完要跳到这儿（if 整个结束）
        sw   a0, 4(sp)
        lw   a0, 4(sp)
        jal  ra, emit_j2
        lw   a0, 0(sp)
        jal  ra, emit_label                # L<条件不成立>:
        jal  ra, compile_stmt              # else 分支
        lw   a0, 4(sp)
        jal  ra, emit_label                # L<if 结束>:
        j    cs_if_done
cs_if_plain:
        lw   a0, 0(sp)
        jal  ra, emit_label                # L<条件不成立>: 就在 then 之后
cs_if_done:
        addi sp, sp, 8
        j    cs_done

# ── while (表达式) 语句 ──
cs_while:
        jal  ra, next_label                # 循环头
        addi sp, sp, -8
        sw   a0, 0(sp)
        jal  ra, emit_label                # L<循环头>:
        li   a0, '('
        jal  ra, expect_ch
        jal  ra, compile_rel               # 条件算进 a0
        li   a0, ')'
        jal  ra, expect_ch
        jal  ra, next_label                # 循环出口
        sw   a0, 4(sp)
        jal  ra, emit_beqz                 # beqz a0, L<出口>
        jal  ra, compile_stmt              # 循环体
        lw   a0, 0(sp)
        jal  ra, emit_j2                   # 回到循环头
        lw   a0, 4(sp)
        jal  ra, emit_label                # L<出口>:
        addi sp, sp, 8

cs_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_rel：关系表达式（整个表达式里最多一个关系运算符）
#  先算左边；后面如果跟着 < > <= >= == !=，就把左边压栈、算右边、
#  弹回 a1 做比较；结果 0/1 放 a0（正好给 beqz 用）。
# ─────────────────────────────────────────────────────────────────────────
compile_rel:
        addi sp, sp, -16
        sw   ra, 12(sp)
        jal  ra, compile_add
cr_try:
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '<'
        beq  t1, t2, cr_lt
        li   t2, '>'
        beq  t1, t2, cr_gt
        li   t2, '='
        beq  t1, t2, cr_eq
        li   t2, '!'
        beq  t1, t2, cr_ne
        j    cr_done
cr_lt:
        jal  ra, advance
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '='
        beq  t1, t2, cr_le
        la   t0, T_SLT
        j    cr_go
cr_le:
        jal  ra, advance
        la   t0, T_SLE
        j    cr_go
cr_gt:
        jal  ra, advance
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '='
        beq  t1, t2, cr_ge
        la   t0, T_SGT
        j    cr_go
cr_ge:
        jal  ra, advance
        la   t0, T_SGE
        j    cr_go
cr_eq:
        jal  ra, advance                   # 吃掉第一个 '='
        li   a0, '='
        jal  ra, expect_ch                 # '==' 一定是成对的
        la   t0, T_SEQ
        j    cr_go
cr_ne:
        jal  ra, advance                   # 吃掉 '!'
        li   a0, '='
        jal  ra, expect_ch
        la   t0, T_SNE
cr_go:
        addi sp, sp, -8
        sw   t0, 0(sp)                 # 运算模板地址先存好（右边一算完就会被冲掉）
        la   a0, T_PUSH
        jal  ra, emit_str
        jal  ra, compile_add
        la   a0, T_POP
        jal  ra, emit_str
        lw   a0, 0(sp)
        addi sp, sp, 8
        jal  ra, emit_str                  # 输出比较指令
cr_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# ─────────────────────────────────────────────────────────────────────────
#  compile_add：加减（左结合）
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
#  compile_atom：数、变量、名字()、- 负号、括号
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
        jal  ra, compile_rel
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
        jal  ra, id_take                   # 名字 → IDBUF
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        li   t2, '('
        beq  t1, t2, ca_call
        la   a0, IDBUF                 # 不是调用：当变量读出来
        jal  ra, var_lookup
        bltz a0, ca_zero
        la   a0, T_LA1                 # la a1, 名字
        jal  ra, emit_str
        la   a0, IDBUF
        jal  ra, emit_str
        la   a0, T_NL
        jal  ra, emit_str
        la   a0, T_LW                  # lw a0, 0(a1)
        jal  ra, emit_str
        j    ca_done
ca_call:
        jal  ra, advance                   # 吃掉 '('
        li   a0, ')'
        jal  ra, expect_ch                 # 无参：立刻补上 ')'
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

# ─────────────────────────────────────────────────────────────────────────
#  emit_decls：把 ".data" 和"变量名: .word 0"补在最后
# ─────────────────────────────────────────────────────────────────────────
emit_decls:
        addi sp, sp, -16
        sw   ra, 12(sp)
        la   t0, NVARS
        lw   t1, 0(t0)
        beqz t1, ed_done               # 一个变量都没有：不用吐 .data
        sw   t1, 4(sp)                 # 变量个数
        sw   x0, 0(sp)                 # 下标
        la   a0, T_DATAS
        jal  ra, emit_str
ed_loop:
        lw   t2, 0(sp)
        lw   t3, 4(sp)
        bge  t2, t3, ed_done
        slli t3, t2, 4                 # 每个名字占 16 字节
        la   t4, VARNAMES
        add  t4, t4, t3
        mv   a0, t4
        jal  ra, emit_str                  # 变量名
        la   a0, T_ZERO
        jal  ra, emit_str                  # ": .word 0" 加换行
        lw   t2, 0(sp)
        addi t2, t2, 1
        sw   t2, 0(sp)
        j    ed_loop
ed_done:
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

# emit_store：把 a0 存进 CURVAR 那个变量（la a1, 名字 / sw a0, 0(a1)）
emit_store:
        addi sp, sp, -8
        sw   ra, 4(sp)
        la   a0, T_LA1
        jal  ra, emit_str
        la   a0, CURVAR
        jal  ra, emit_str
        la   a0, T_NL
        jal  ra, emit_str
        la   a0, T_SW
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

# emit_beqz：beqz a0, L<a0>
emit_beqz:
        addi sp, sp, -8
        sw   ra, 4(sp)
        sw   a0, 0(sp)
        la   a0, T_BEQZ
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

# id_read_raw：真读一个词到 IDBUF（字母/数字/下划线，最多 15 个字符）
id_read_raw:
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

# id_take：取一个词到 IDBUF（已经暂存过就直接用）
id_take:
        addi sp, sp, -8
        sw   ra, 4(sp)
        la   t0, PEEKD
        lw   t1, 0(t0)
        beqz t1, it_raw
        sw   x0, 0(t0)                 # 清掉暂存标志
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret
it_raw:
        jal  ra, id_read_raw
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# id_peek：看一眼下一个词，但不消费（已经暂存过就直接返回）
id_peek:
        addi sp, sp, -8
        sw   ra, 4(sp)
        la   t0, PEEKD
        lw   t1, 0(t0)
        bnez t1, ip_done
        la   t0, IDBUF
        sb   x0, 0(t0)                 # 先清空暂存的词
        jal  ra, skip_ws
        la   t0, CH
        lw   t1, 0(t0)
        beqz t1, ip_done
        mv   a0, t1
        jal  ra, is_word_ch                # 不是词的首字符就什么都不暂存
        beqz a0, ip_done
        jal  ra, id_read_raw
        la   t0, PEEKD
        li   t1, 1
        sw   t1, 0(t0)
ip_done:
        lw   ra, 4(sp)
        addi sp, sp, 8
        ret

# is_word_ch：a0 是字母/数字/下划线 → 1，否则 0（只用 t0、a0）
is_word_ch:
        li   t0, '_'
        beq  a0, t0, iw_yes
        li   t0, '0'
        blt  a0, t0, iw_alpha
        li   t0, '9'
        bge  t0, a0, iw_yes
iw_alpha:
        li   t0, 'a'
        blt  a0, t0, iw_upper
        li   t0, 'z'
        bge  t0, a0, iw_yes
iw_upper:
        li   t0, 'A'
        blt  a0, t0, iw_no
        li   t0, 'Z'
        blt  t0, a0, iw_no
iw_yes:
        li   a0, 1
        ret
iw_no:
        li   a0, 0
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

# ═════════════════════════════════════════════════════════════════════════
#  变量名表
# ═════════════════════════════════════════════════════════════════════════

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

# str_copy：把 a1 处的字符串抄到 a0（含结尾的 0）
str_copy:
sc_loop:
        lbu  t0, 0(a1)
        sb   t0, 0(a0)
        beqz t0, sc_done
        addi a0, a0, 1
        addi a1, a1, 1
        j    sc_loop
sc_done:
        ret

# var_lookup：a0 = 名字地址 → a0 = 槽号；没登记过 → -1
var_lookup:
        addi sp, sp, -16
        sw   ra, 12(sp)
        sw   a0, 8(sp)
        la   t0, NVARS
        lw   t1, 0(t0)
        sw   t1, 4(sp)                 # 变量个数
        sw   x0, 0(sp)                 # 下标
vl_loop:
        lw   t2, 0(sp)
        lw   t3, 4(sp)
        bge  t2, t3, vl_none
        slli t3, t2, 4                 # 每个名字占 16 字节
        la   t4, VARNAMES
        add  t4, t4, t3
        lw   a0, 8(sp)
        mv   a1, t4
        jal  ra, str_eq
        bnez a0, vl_found
        lw   t2, 0(sp)
        addi t2, t2, 1
        sw   t2, 0(sp)
        j    vl_loop
vl_found:
        lw   a0, 0(sp)
        j    vl_done
vl_none:
        li   a0, -1
vl_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret

# var_add：a0 = 名字地址 → 登记它（已经登记过就什么都不做）
var_add:
        addi sp, sp, -16
        sw   ra, 12(sp)
        sw   a0, 8(sp)
        jal  ra, var_lookup
        bgez a0, va_done               # 已经登记过
        la   t0, NVARS
        lw   t1, 0(t0)
        li   t2, 16
        bge  t1, t2, va_done           # 表满了（不会发生）
        slli t2, t1, 4
        la   t3, VARNAMES
        add  t3, t3, t2
        mv   a0, t3
        lw   a1, 8(sp)
        jal  ra, str_copy                  # 抄名字
        la   t0, NVARS
        lw   t1, 0(t0)
        addi t1, t1, 1
        sw   t1, 0(t0)
va_done:
        lw   ra, 12(sp)
        addi sp, sp, 16
        ret
