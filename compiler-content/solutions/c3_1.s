# 3-1 参考解：读字符流，统计并输出字符个数（含换行符，不含结束标记）
#
# 约定（叙事里讲过）：
#   输入端口在 0xF0000004，lw 读一个字符；队列空了会读回 0，表示输入结束
#   '0' 是 48，'\n' 是 10
#   统计结果先写进 0x2000（判定会看这个数），再用 a7 = 2 以十进制打印
li   x1, 0xF0000004      # x1 = 输入端口地址
addi x8, x0, 0           # x8 = 计数器：已经读进来多少个字符

read_loop:
lw   x5, 0(x1)           # 取下一个字符
beq  x5, x0, done_read   # 读到 0 → 队列空了，结束
addi x8, x8, 1           # 否则计数器 +1（\n 也是字符，照数不误）
j    read_loop

done_read:
li   x9, 0x2000
sw   x8, 0(x9)           # 把字符个数存进 0x2000

# ── 下面把数字打印出来 ──
li   x3, 0x2020          # 缓冲区末尾（数字从后往前生成）
li   x4, 10              # 除数：10
addi x5, x3, 0           # x5 = 写入位置，从末尾往前挪
addi x6, x8, 0           # x6 = 待转换的值

to_digits:
remu x7, x6, x4          # 取最低位数字
addi x7, x7, 48          # 数字 → 字符（'0' 是 48）
addi x5, x5, -1          # 写入位置往前挪一格
sb   x7, 0(x5)
divu x6, x6, x4
bne  x6, x0, to_digits

li   x7, 10              # 末尾补一个换行
sb   x7, 0(x3)

# ── 从 x5 打印到 x3（含末尾的换行） ──
print_loop:
lb   x6, 0(x5)           # 读下一个字符（正数，可以直接和 0 比）
beq  x6, x0, finish
addi x10, x6, 0          # 待输出的字符放 x10（a0）
li   x17, 1              # x17（a7） = 1：输出一个字符
ecall
addi x5, x5, 1
j    print_loop

finish:
li   x10, 0              # 退出码
li   x17, 0
ecall
