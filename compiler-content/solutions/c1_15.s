# 1-15 参考解：按字符读入整数，累加矿石量到读到 0 为止，与门槛比较后输出 '1' / '0'
#
# 约定（叙事里讲过）：
#   输入端口在 0xF0000004，读一个字符；读空会读回 0
#   '0' 的编码是 48，' ' 是 32，'-' 是 45
#   第一个数是门槛（写进 0x1004），后面是矿石量，读到 0 结束
#   总和也写进 0x1004；总和 >= 门槛把 '1'（49）写进显示端口，否则写 '0'（48）
li   x20, 0xF0000004
li   x21, 0x1004
jal  x1, read_int
addi x8, x10, 0
sw   x8, 0(x21)

addi x9, x0, 0
sum_loop:
jal  x1, read_int
beq  x10, x0, judge
add  x9, x9, x10
j    sum_loop

judge:
sw   x9, 0(x21)
blt  x9, x8, not_enough
li   x18, 49
j    output
not_enough:
li   x18, 48
output:
li   x19, 0xF0000000
sw   x18, 0(x19)
li   a7, 0
ecall

# ── 子程序：从输入端口读一个十进制整数（可带负号），结果放 x10 ──
read_int:
addi x11, x0, 0
addi x12, x0, 1
skip_blank:
lw   x13, 0(x20)
addi x14, x0, 32
beq  x13, x14, skip_blank
addi x14, x0, 10
beq  x13, x14, skip_blank
addi x14, x0, 13
beq  x13, x14, skip_blank
addi x14, x0, 45
bne  x13, x14, digit_loop
addi x12, x0, -1
lw   x13, 0(x20)
digit_loop:
addi x14, x0, 48
blt  x13, x14, read_done
addi x14, x0, 57
blt  x14, x13, read_done
addi x11, x11, 0
li   x15, 10
mul  x11, x11, x15
addi x11, x11, -48
add  x11, x11, x13
lw   x13, 0(x20)
j    digit_loop
read_done:
mul  x10, x11, x12
jalr x0, 0(x1)
