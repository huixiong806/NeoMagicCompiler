# 1-10 参考解：求 1 + 2 + ... + n（n 在 x1），结果放 x10
#
# x5 当计数器（从 1 数到 n），x10 当总和。
# 注意先比较再累加：n = 0 时一下就跳出，总和保持 0。
addi x10, x0, 0
addi x5, x0, 1
loop:
blt  x1, x5, done
add  x10, x10, x5
addi x5, x5, 1
j    loop
done:
ecall
