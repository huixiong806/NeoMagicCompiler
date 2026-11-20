# 1-6 参考解：把 1、2、3 写进 0x1000、0x1004、0x1008
li   x1, 0x1000
li   x2, 1
sw   x2, 0(x1)
li   x2, 2
sw   x2, 4(x1)
li   x2, 3
sw   x2, 8(x1)
ecall
