# 任务：比较 t0 与 t1，将较大值存入 t2
blt t0, t1, use_t1
mv  t2, t0
j   done
use_t1:
mv  t2, t1
done:
ecall
