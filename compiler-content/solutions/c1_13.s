# 1-13 参考解：调用 square 子程序，把 x6 变成它的平方，结果放进 x10
#
# 脚手架已经把参数从 x1（它同时是 ra）挪到了 x6，所以这里只要：
#   1. 用 jal 调用 square（jal 会顺手把返回地址写进 x1）
#   2. 子程序返回后，把 x6 里的结果搬进 x10
addi x6, x1, 0
jal  x1, square
addi x10, x6, 0
ecall
square:
mul  x6, x6, x6
jalr x0, 0(x1)
