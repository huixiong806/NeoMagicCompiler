# 1-3 参考解：x3 = x1 - x2，并把 x1 与 x2 互换
# 思路：借一个中间格子 x5 暂存，两个数才换得动
sub  x3, x1, x2
addi x5, x1, 0
addi x1, x2, 0
addi x2, x5, 0
ecall
