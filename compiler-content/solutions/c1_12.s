# 1-12 参考解：同 1-11，但循环体里多放一条 ebreak
#
# ebreak 只是"在这里停一下"，调试面板可以观察这一刻的 x10、x5、x6；
# 判定时程序会继续跑到 ecall，所以 x10 的终值仍然是正确的总和。
li   x6, 0x1000
addi x10, x0, 0
addi x5, x0, 0
loop:
lw   x7, 0(x6)
add  x10, x10, x7
ebreak
addi x6, x6, 4
addi x5, x5, 1
blt  x5, x1, loop
ecall
