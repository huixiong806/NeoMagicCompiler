# 4-4 参考解：把一行表达式（非负整数、+ - * /、括号）解析成后缀（RPN）token 流
#
# 输入：0xF0000004 按字节读字符（读到 0 = 输入结束）；
#       字符集只有 0-9、+ - * / ( )、空白，数字是不带符号的非负整数。
# 输出：一个 token 一行，数字 -> NUM:值，运算符 -> OP:符号，括号只当路标不输出。
#
# 结构是教科书式的递归下降，三层对应三档优先级：
#   parse_expr   （管 + -）：先要一个 term，然后只要还看到 + 或 -，就再要一个 term
#   parse_term   （管 * /）：先要一个 factor，然后只要还看到 * 或 /，就再要一个 factor
#   parse_factor （数字 或 '(' expr ')'）
# 每个函数都"先取一个低一层的整体，再循环吃掉本层的运算符"，
# 所以加法那一层根本看不到乘号：它只看到 1 和 2*3 两个整体 —— 优先级是结构逼出来的。
#
# 有一个最容易踩的坑，在这里写清楚：**当前字符 t0 是谁读的？**
#   t0 是"向前看一格"的那个字符，由**上一层**负责准备好：
#     main 先读一次，parse_expr 才看得见第一个 token；
#     每一层吃掉自己那一档的运算符之后，也立刻读下一个字符。
#   哪一层忘了读，t0 就会停在 0（= 输入结束），整棵树当场变成空的 ——
#   "程序正常跑完了，却一个字都没输出"通常就是这个原因。
#
# 后缀 token 是"看完一整棵子树才吐出来"：数字读完了吐 NUM:，左右子树都算完才吐 OP:。
# 到了 4-5 这件事会反过来（先吐前缀再递归），那时打印出来的就是 AST 本身。
#
# 寄存器分工（全程序统一，方便对照）：
#   t0 = 当前字符   t1 = 临时   t2 = 数字累加值   t3 = 本层的运算符
#   t4/t5 = 打印用临时   t6 = 循环计数   s0/s1 = 打印时要跨过子调用的备份
#
# 还有一个**千万别再犯**的坑，写在最前面（它真的在这份参考解里发生过）：
#   曾经有一版 read_char 除了端口以外还看一个"回退槽"：某一层读到不属于自己的字符时，
#   把它存进槽里，上一层再调 read_char 时先从槽里取。看上去很聪明，其实是错的：
#   上层手里的 t0 **本来就是那个字符**，它调 read_char 只是想往前挪一格，
#   结果又从容里读回同一个字符 —— 位置一步没动，循环永远出不去。
#   （症状：输入 3+5 会打出 `3(+ )(+ )(+ )…` 直到指令数超限。）
#   正确的分工只有一句：**t0 就是"现在手上的字符"**，谁读到的就归谁看，
#   不需要第二个副本，也不需要把字符塞回输入队列。

    .text
main:
    jal  ra, read_char               # 先把第一个字符读进来（"向前看"的那一格）
    jal  ra, parse_expr              # 解析整个表达式（根）
    # 收工：退出码 0，然后 ecall 结束程序
    li   a0, 0
    li   a7, 0
    ecall
    li   a7, 0
    ecall

# ───────────────────────── 字符输入：读一个字符 ─────────────────────────
#
# read_char：取下一个非空白字符到 t0。
#   读到 0 表示输入结束；空格、\n、\r、\t 一律跳过。
read_char:
read_char_loop:
    li   t0, 0xF0000004
    lbu  t0, 0(t0)                   # 用 lbu 读"一个字节"；lw 会一次吃掉 4 个字节
read_char_check:
    beq  t0, x0, read_char_done      # 0 = 输入结束，原样返回
    li   t1, 32
    beq  t0, t1, read_char_loop      # 空格
    li   t1, 10
    beq  t0, t1, read_char_loop      # \n
    li   t1, 13
    beq  t0, t1, read_char_loop      # \r
    li   t1, 9
    beq  t0, t1, read_char_loop      # \t
read_char_done:
    ret

# ───────────────────────── 输出 ─────────────────────────

# emit_op：把 a0 里的运算符按 "OP:x" 输出，再换行
#
# 注意：这个函数自己会 jal 到别的辅助函数，所以必须**把 ra 存在栈上**。
# 不存 ra 就去 jal，ra 会被盖成"回到自己函数中间"，返回时就会原地打转
# —— 表现为"输出看起来对了，但程序永远不结束"。
emit_op:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   s0, 4(sp)
    mv   s0, a0                      # 把运算符存进 s0（emit_cstr 会盖掉 t4）
    la   a0, s_op                    # 前缀 "OP:"（长度字节 + 内容）
    jal  ra, emit_cstr
    mv   a0, s0
    li   a7, 1
    ecall
    jal  ra, put_nl
    lw   s0, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 12
    ret

# emit_num：把 a0 里的非负整数按 "NUM:值" 输出，再换行
emit_num:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   s1, 4(sp)
    mv   s1, a0
    la   a0, s_num                   # 前缀 "NUM:"（长度字节 + 内容）
    jal  ra, emit_cstr
    mv   a0, s1
    jal  ra, emit_dec
    mv   a0, s1
    jal  ra, put_nl
    lw   s1, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 12
    ret

# emit_dec：把 a0 里的非负整数按十进制输出（各位先压栈，再反向弹出）
# 这是个**叶子函数**（不 jal 任何东西），所以不必存 ra。
emit_dec:
    mv   t4, a0
    li   t6, 0                       # 已经压了几位
emit_dec_loop:
    li   t1, 10
    rem  t3, t4, t1                  # 最低位
    addi t3, t3, 48
    div  t4, t4, t1
    addi sp, sp, -1
    sb   t3, 0(sp)                   # 压栈
    addi t6, t6, 1
    bne  t4, x0, emit_dec_loop
emit_dec_out:
    lbu  a0, 0(sp)                   # 反向弹出 = 最高位先出
    addi sp, sp, 1
    li   a7, 1
    ecall
    addi t6, t6, -1
    bne  t6, x0, emit_dec_out
    ret

# put_nl：输出一个 '\n'（叶子函数，可以放心 jal）
put_nl:
    li   a0, 10
    li   a7, 1
    ecall
    ret

# emit_cstr：a0 = 字符串表里的地址，第 0 个字节是长度，后面跟着内容（叶子函数）
emit_cstr:
    lbu  t6, 0(a0)                   # t6 = 长度
    addi t4, a0, 1                   # t4 = 内容起点
emit_cstr_loop:
    # 长度字节数的是**内容字节数**，所以这里先判长度再打印：
    # 写成"先打印再自减"就会多吐一个字符。
    beq  t6, x0, emit_cstr_end
    lbu  a0, 0(t4)
    li   a7, 1
    ecall
    addi t4, t4, 1
    addi t6, t6, -1
    j    emit_cstr_loop
emit_cstr_end:
    ret

# ───────────────────────── 递归下降：三层 ─────────────────────────

# parse_expr：expression = term { ('+' | '-') term }
# 本层的运算符放在 t3 上跨过 `jal parse_term` —— parse_term 会用栈帧把它原样还回来（见下）。
parse_expr:
    addi sp, sp, -4
    sw   ra, 0(sp)
    jal  ra, parse_term
pe_loop:
    li   t1, 43                      # '+'
    beq  t0, t1, pe_take
    li   t1, 45                      # '-'
    beq  t0, t1, pe_take
    j    pe_end
pe_take:
    mv   t3, t0                      # 先记下运算符
    jal  ra, read_char               # 吃掉它，再读一个
    jal  ra, parse_term              # 右子树
    mv   a0, t3                      # 两棵子树都完成了，这才吐运算符
    jal  ra, emit_op
    j    pe_loop
pe_end:
    # t0 里那个"不是本层运算符"的字符**就这么留着**：它就是上一层要的向前看格。
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

# parse_term：term = factor { ('*' | '/') factor }
#
# ⚠ 这一关的第三个坑就在这个栈帧里：调用者（parse_expr）把**它自己的运算符**放在 t3 上，
#   托付给 parse_term 一起跨过子调用。所以本帧必须给两样东西**两个格子**：
#     4(sp) = 调用者的 t3（进来先存，返回前还回去）
#     8(sp) = 本层自己的运算符
#   两样塞进同一个格子的话，本层一吃到 '*'，就把调用者的 '+' 覆盖掉了 ——
#   上一层随后印出来的就是 '*'（症状：`1+2*3` 打出 OP:* OP:*，少了一个 OP:+）。
parse_term:
    addi sp, sp, -12
    sw   ra, 0(sp)
    sw   t3, 4(sp)                   # 把调用者的 t3 收好
    jal  ra, parse_factor
pt_loop:
    li   t1, 42                      # '*'
    beq  t0, t1, pt_take
    li   t1, 47                      # '/'
    beq  t0, t1, pt_take
    j    pt_end
pt_take:
    sw   t0, 8(sp)                   # 本层的运算符存在**另一个**格子里
    jal  ra, read_char
    jal  ra, parse_factor
    lw   t3, 8(sp)
    mv   a0, t3
    jal  ra, emit_op
    j    pt_loop
pt_end:
    # t0 原样留给上一层；t3 还回调用者的值
    lw   t3, 4(sp)
    lw   ra, 0(sp)
    addi sp, sp, 12
    ret

# parse_factor：factor = 数字 | '(' expression ')'
parse_factor:
    addi sp, sp, -4
    sw   ra, 0(sp)
    li   t1, 48
    blt  t0, t1, pf_lparen           # 比 '0' 小 -> 只可能是 '('
    li   t1, 57
    blt  t1, t0, pf_lparen           # 比 '9' 大 -> 也只可能是 '('
    j    pf_number
pf_lparen:
    li   t1, 40                      # '('
    bne  t0, t1, pf_end              # 既不是数字也不是 '('：当作表达式结束
    jal  ra, read_char               # 吃掉 '(' —— 括号里的世界从最高层重新开始
    jal  ra, parse_expr
    li   t1, 41                      # 期待 ')'
    bne  t0, t1, pf_end
    jal  ra, read_char               # 吃掉 ')'
    j    pf_end
pf_number:
    li   t2, 0                       # 数字累加值
pf_num_loop:
    li   t1, 48
    blt  t0, t1, pf_num_done
    li   t1, 57
    blt  t1, t0, pf_num_done
    li   t1, 10                      # t2 = t2 * 10 + (t0 - '0')
    mul  t2, t2, t1
    addi t4, t0, -48
    add  t2, t2, t4
    jal  ra, read_char
    j    pf_num_loop
pf_num_done:
    mv   a0, t2
    jal  ra, emit_num                # 数字被完整吃掉的那一刻吐 NUM:
    j    pf_end
pf_end:
    lw   ra, 0(sp)
    addi sp, sp, 4
    ret

    .data
# 两个前缀字符串，格式统一：**第一个字节是长度**，后面才是内容，最后那个 0 只是给人看的
# （emit_cstr 只看长度，看不到 0；所以长度写错就会印出结尾的 0）。
# 运算符本身（+ - * /）不放在表里 —— emit_num/emit_op 拿到的是字符，直接 ecall 就行。
s_num:
    .byte 4                          # ★ "NUM:" 是 4 个字节。写成 5 就会把结尾那个 0 也印出去，
                                     #   输出变成 "NUM:" + \0 + 数字（这个坑真踩过）。
    .ascii "NUM:"
    .byte 0
s_op:
    .byte 3                          # "OP:" 是 3 个字节
    .ascii "OP:"
    .byte 0
