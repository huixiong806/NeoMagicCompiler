# 任务：计算 t2 = t0 - t1，并交换 t0 与 t1（可借用 t3）
sub t2, t0, t1
mv  t3, t0
mv  t0, t1
mv  t1, t3
ecall
