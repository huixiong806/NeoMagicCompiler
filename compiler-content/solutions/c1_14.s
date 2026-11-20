# 1-14 参考解：在 f 里做两层调用 square(square(x))，用栈保住 ra
#
# 关键点：jal 会把返回地址写进 x1，第二次调用会盖掉第一次的返回地址。
# 所以在 f 里先把 x1 压栈，两次调用都做完之后再把 x1 捞回来，
# 这样才能照着它回到主线（否则会跳错地方，甚至死循环）。
li   x6, 0x1000
lw   x6, 0(x6)
jal  x1, f
addi x10, x6, 0
ecall

f:
addi sp, sp, -4
sw   x1, 0(sp)
jal  x1, square
jal  x1, square
lw   x1, 0(sp)
addi sp, sp, 4
jalr x0, 0(x1)

square:
mul  x6, x6, x6
jalr x0, 0(x1)
