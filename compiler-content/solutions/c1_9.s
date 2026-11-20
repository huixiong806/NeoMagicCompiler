# 1-9 参考解：在 ok 之后补一条 j，跳过那枚"反转印"
#
# 初始代码的前三行是闸刻好的：
#   li x1, 2
#   j ok            ← 已经跳过了两个陷阱行
#   addi x1, x0, 99 ← 这是第二枚陷阱，落在 ok 后面，滑动就会踩中
# 所以玩家必须在 ok: 之后、它之前再写一条 j，直接跳到 finish。
li x1, 2
j ok
addi x1, x0, 99
ok:
j finish
finish:
addi x1, x0, 7
ecall
