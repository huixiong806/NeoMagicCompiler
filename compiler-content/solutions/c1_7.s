# 1-7 参考解：x1 == x2 时 x3 = 1，否则 x3 = 0
#
# 思路：先"猜"它们相等（x3 = 1），再用 bne 问一句"真的相等吗"；
# 不等就把 x3 改回 0。这就是"先猜一个，再推翻它"。
addi x3, x0, 1
bne  x1, x2, not_equal
j    done
not_equal:
addi x3, x0, 0
done:
ecall
