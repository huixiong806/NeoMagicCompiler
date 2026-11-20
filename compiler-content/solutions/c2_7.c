// 2-7 参考解：递归求阶乘
int fact(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * fact(n - 1);
}

int solve(int n) {
    return fact(n);
}
