# 任务：t0=总数, t1=每箱容量 → t2=整箱数, t3=余数, t4=已装箱总数
div t2, t0, t1
rem t3, t0, t1
mul t4, t2, t1
ecall
