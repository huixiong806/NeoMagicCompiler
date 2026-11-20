# 巩固任务：计算 t2 = 3 * t0 - t1 + 5（不要修改 t0 和 t1）
add  t3, t0, t0
add  t3, t3, t0
sub  t3, t3, t1
addi t2, t3, 5
ecall
