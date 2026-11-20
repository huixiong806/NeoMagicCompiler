# 1-11 参考解：把 0x1000 起前 n 个字加起来，结果放 x10
#
# x6 是"当前要读哪个格子"的指针（初始由脚手架给成 0x1000），
# 所以不能直接加在 x6 上取下一个字 —— 借 x7 算地址，x6 自己往后挪。
li   x6, 0x1000
addi x10, x0, 0
addi x5, x0, 0
loop:
lw   x7, 0(x6)
add  x10, x10, x7
addi x6, x6, 4
addi x5, x5, 1
blt  x5, x1, loop
ecall
