# 挑战：不用乘除取余，用减法循环求正整数 t0 与 t1 的最大公约数存入 t2
loop:
blt t0, t1, b_big
blt t1, t0, a_big
mv  t2, t0
j   done
a_big:
sub t0, t0, t1
j   loop
b_big:
sub t1, t1, t0
j   loop
done:
ecall
