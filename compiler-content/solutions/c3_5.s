# 3-5 参考解：用栈判断括号是否匹配，输出 ok / bad
#
# 约定（叙事里讲过）：
#   输入端口 0xF0000004，读回 0 表示结束
#   栈顶指针 x20 = 0x3000，**向上增长**（压栈：存一个字，然后 +4；弹栈：先 -4，再取）
#   只往栈里存"期望的右括号"：看到 `(` 就压 `)`，看到 `[` 就压 `]`，看到 `{` 就压 `}`
#   配对成功 = 结束时**栈正好空着**，而且途中没有出现过"对不上"
#
# 寄存器用法（这一关的关键就是别把 x28 用掉）：
#   x1  = 输入端口 0xF0000004
#   x5  = 当前读到的字符
#   x6  = 临时比较用的常数（注意：它是"被打来打去"的，别指望它留住东西）
#   x7  = 要压进栈的值 / 记录地址
#   x28 = **当前这个右括弧"期望配到的"左括号**（一进右括号分支就定下来，之后只读不写）
#   x18 = 1 表示"到目前为止一切正常"，0 表示已经坏了
#   x19 = 栈底 0x3000
#   x20 = 栈顶指针（等于栈底就是空栈）
#
# 踩过的坑（留个记录）：一开始把"期望值"和"从栈里弹出的值"都放在 x6 里，
# 结果左括号分派那几条 `li x6, ...` 把期望值冲掉了，比较永远成立 →
# 连 `{([{()}]})` 这种完全配平的串都被判成 bad。两个值必须用两个寄存器。

li   x1, 0xF0000004      # 输入端口
li   x20, 0x3000         # 自家的栈顶指针：栈底从 0x3000 开始，往上长
li   x19, 0x3000         # 栈底（用来判断"栈是不是空的"）
addi x18, x0, 1          # x18 = 1 表示"到目前为止一切正常"

bracket_loop:
lw   x5, 0(x1)           # 取下一个字符
beq  x5, x0, brackets_end

li   x6, 40              # '('
beq  x5, x6, open_paren
li   x6, 91              # '['
beq  x5, x6, open_square
li   x6, 123             # '{'
beq  x5, x6, open_curly

# ── 右括号：先认出来，再定下"它期望配到的左括号" ──
li   x6, 41              # ')'
beq  x5, x6, close_paren
li   x6, 93              # ']'
beq  x5, x6, close_square
li   x6, 125             # '}'
beq  x5, x6, close_curly
j    bracket_loop        # 别的字符（空格、换行）直接跳过

# ══ 左括号：压入"期望的右括号" ══
open_paren:
li   x7, 41              # 期望后面来一个 ')'
j    bracket_push
open_square:
li   x7, 93              # 期望后面来一个 ']'
j    bracket_push
open_curly:
li   x7, 125             # 期望后面来一个 '}'
bracket_push:
sw   x7, 0(x20)          # 存进栈顶这一格
addi x20, x20, 4         # 栈顶往上挪一格
j    bracket_loop

# ══ 右括号：先看栈空不空，再弹出来和 x28 比 ══
close_curly:             # 注意：判断顺序要和前面认字符的顺序岔开，
li   x28, 123            # 这里先定下"它期望配到的左括号是 '{'"
j    do_pop
close_square:
li   x28, 91             # 期望配到 '['
j    do_pop
close_paren:
li   x28, 40             # 期望配到 '('

do_pop:
beq  x20, x19, bracket_mismatch   # 栈空 → 这个右括号没有左括号配它
addi x20, x20, -4        # 先退到栈顶那一格（弹栈）
lw   x6, 0(x20)          # 取出当时压进去的"期望的右括号"
beq  x6, x5, bracket_loop   # 和当前字符（x5）一比，对了就继续
j    bracket_mismatch

# ══ 对不上：记下"不正常"，但接着把输入读完再收尾 ══
bracket_mismatch:
addi x18, x0, 0
j    bracket_loop

# ══ 收尾：两条都要满足才算匹配 ══
#    1) 途中没有对不上（x18 = 1）
#    2) 栈正好空着（x20 还等于栈底 x19）
brackets_end:
bne  x20, x19, bracket_bad   # 栈里还留着东西（比如只来了一堆左括号）
beq  x18, x0, bracket_bad
li   x6, 1                   # 匹配
j    bracket_report
bracket_bad:
addi x6, x0, 0               # 不匹配
bracket_report:
li   x7, 0x2000              # 结果写进 0x2000
sw   x6, 0(x7)
beq  x6, x0, report_bad
li   x9, 0x2010              # 指向 "ok"
j    emit
report_bad:
li   x9, 0x2018              # 指向 "bad"

emit:
li   x17, 1              # x17（a7） = 1：输出一个字符
emit_loop:
lb   x5, 0(x9)           # 逐个取字符，遇到 0 就停
beq  x5, x0, emit_done
addi x10, x5, 0
ecall
addi x9, x9, 1
j    emit_loop
emit_done:
li   x10, 10             # 收尾补一个换行
ecall
li   x10, 0
li   x17, 0
ecall

# ── 数据：要输出的两个词，末尾都用 0 收尾 ──
#    注意：0x2000 已经用来放结果了，所以字符串放到 0x2010 / 0x2018
.data
.org 0x2010
.ascii "ok"
.byte 0
.org 0x2018
.ascii "bad"
.byte 0
