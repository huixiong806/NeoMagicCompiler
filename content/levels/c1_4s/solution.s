# 挑战：仅用 t0、t1 与 add/sub 就地交换 t0 和 t1
add t0, t0, t1
sub t1, t0, t1
sub t0, t0, t1
ecall
