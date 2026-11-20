# 任务：用循环计算 1 + 2 + ... + t0，结果存入 t1（t0 = 0 时为 0）
addi t1, zero, 0
addi t2, zero, 1
loop:
blt  t0, t2, done
add  t1, t1, t2
addi t2, t2, 1
j    loop
done:
ecall
