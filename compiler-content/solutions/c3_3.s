# 3-3 参考解：读入空格分隔的十进制整数（可以带负号），输出它们的总和
#
# 约定（叙事里讲过）：
#   输入端口 0xF0000004，读回 0 表示结束
#   拼数三步：跳过分隔符 → 看有没有 '-' → 逐位拼 value = value * 10 + 这一位
#   读到一个非数字字符就收手，并且**不把它吃掉**（它可能正是下一个数的负号或数字）
#   总和写进 0x2000，并用 a7 = 2 按十进制打印

li   x20, 0xF0000004     # 输入端口
li   x21, 0xF0000000     # 显示端口
addi x18, x0, 0          # x18 = 总和

sum_loop:
jal  x1, read_int        # 读一个整数到 x10
beq  x10, x0, report     # 读回 0（队列空了）就收工
add  x18, x18, x10
j    sum_loop

report:
li   x9, 0x2000
sw   x18, 0(x9)          # 总和存进 0x2000
addi x10, x18, 0         # 待打印的数放 x10（a0）
li   x17, 2              # x17（a7） = 2：按十进制打印 x10
ecall

li   x10, 0
li   x17, 0
ecall

# ── 子程序：读一个十进制整数（可带负号），结果放 x10 ──
read_int:
addi x11, x0, 0          # x11 = 正在拼的值
addi x12, x0, 1          # x12 = 符号系数

skip_sep:
lw   x13, 0(x20)
li   x14, 32             # 空格
beq  x13, x14, skip_sep
li   x14, 10             # 换行
beq  x13, x14, skip_sep
li   x14, 13             # 回车
beq  x13, x14, skip_sep

li   x14, 45             # '-' 是 45
bne  x13, x14, digit_loop
addi x12, x0, -1         # 是负号：先记下"最后要取负"
lw   x13, 0(x20)         # 真正的第一位数字还在后面

digit_loop:
li   x14, 48             # '0'
blt  x13, x14, read_done # 比 '0' 小 → 这个数到头了
li   x14, 57             # '9'
blt  x14, x13, read_done # 比 '9' 大 → 也到头了
li   x15, 10
mul  x11, x11, x15       # 值 = 值 * 10
addi x13, x13, -48       # 字符 → 数字
add  x11, x11, x13
lw   x13, 0(x20)         # 取下一个字符
j    digit_loop

read_done:
mul  x10, x11, x12       # 正数或负数
jalr x0, 0(x1)
