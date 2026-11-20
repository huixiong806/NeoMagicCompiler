# 3-2 参考解：把输入字符分成"数字 / 字母 / 其他"三类，各数一遍，按三行输出
#
# 约定（叙事里讲过）：
#   输入端口 0xF0000004，读回 0 表示结束；输出端口 0xF0000000
#   '0' 是 48，'9' 是 57；'A' 是 65，'Z' 是 90；'a' 是 97，'z' 是 122
#   三个计数：x8 = 数字，x9 = 字母，x18 = 其他
#   结果同时写进 0x2000 / 0x2004 / 0x2008（判定会看），并按三行打印
#
# 判断的写法很讲究（也是这一关最容易错的地方）：
#   只问"比谁小"最省心 —— `blt a, b, 标签` = "a 比 b 小就跳走"。
#   判断"在 48..57 之间"就是：比 48 小 → 走开；比 57 大 → 走开；剩下的就是数字。
#   问"比 57 大"要写成 `blt 57, 字符, 走开`（把两个操作数换个位置）。

li   x1, 0xF0000004      # 输入端口
addi x8, x0, 0           # 数字个数
addi x9, x0, 0           # 字母个数
addi x18, x0, 0          # 其他字符个数

read_loop:
lw   x5, 0(x1)           # 取下一个字符
beq  x5, x0, counts      # 0 = 队列空了
li   x6, 48              # '0'
blt  x5, x6, other       # 比 '0' 小 → 其他
li   x6, 57              # '9'
blt  x6, x5, not_digit   # '9' 比字符小 → 字符比 '9' 大 → 不是数字
addi x8, x8, 1
j    read_loop
not_digit:
li   x6, 65              # 'A'
blt  x5, x6, other       # 比 'A' 小（'9' 和 'A' 之间还夹着符号）→ 其他
li   x6, 90              # 'Z'
blt  x6, x5, not_upper   # 比 'Z' 大 → 可能小写，也可能更靠后
addi x9, x9, 1
j    read_loop
not_upper:
li   x6, 97              # 'a'
blt  x5, x6, other       # 'Z' 和 'a' 之间夹着 `[ \ ] ^ _ \`` → 其他
li   x6, 122             # 'z'
blt  x6, x5, other       # 比 'z' 大 → 其他
addi x9, x9, 1
j    read_loop

other:
addi x18, x18, 1
j    read_loop

counts:
li   x19, 0x2000
sw   x8, 0(x19)          # 0x2000 = 数字个数
sw   x9, 4(x19)          # 0x2004 = 字母个数
sw   x18, 8(x19)         # 0x2008 = 其他字符个数
li   x21, 0xF0000000     # 显示端口
addi x20, x0, 10         # 换行符

addi x10, x8, 0          # 第一行：数字
jal  x1, print_int
sb   x20, 0(x21)         # 换行
addi x10, x9, 0          # 第二行：字母
jal  x1, print_int
sb   x20, 0(x21)
addi x10, x18, 0         # 第三行：其他
jal  x1, print_int
sb   x20, 0(x21)

li   x10, 0
li   x17, 0
ecall

# ── 子程序：把 x10 里的非负整数逐位打印出来（借来 0x2020 那一小段当缓冲区） ──
print_int:
li   x3, 0x2020          # 缓冲区末尾
li   x4, 10              # 除数
addi x5, x3, 0
addi x6, x10, 0
bne  x6, x0, to_digits
addi x6, x0, 48          # 特例：0 本身也要打印一个 '0'
addi x5, x5, -1
sb   x6, 0(x5)
j    emit

to_digits:
remu x7, x6, x4          # 最低位数字
addi x7, x7, 48          # 数字 → 字符
addi x5, x5, -1
sb   x7, 0(x5)
divu x6, x6, x4
bne  x6, x0, to_digits

emit:
lb   x6, 0(x5)           # 从高位往低位，一个字符一个字符送出去
beq  x6, x0, print_done
sb   x6, 0(x21)          # 直接写显示端口：这一关的输出很多，这样更省
addi x5, x5, 1
j    emit
print_done:
jalr x0, 0(x1)
