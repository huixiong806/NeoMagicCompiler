// 2-3 参考解：1 + 2 + ... + n（n <= 0 时返回 0）
int solve(int n) {
    int sum = 0;
    int i = 1;
    while (i <= n) {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}
