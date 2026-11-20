// 2-11 参考解：埃氏筛，返回不超过 n 的素数个数
int solve(int n) {
    if (n < 2) {
        return 0;
    }
    int mark[256];
    for (int i = 0; i <= n; i++) {
        mark[i] = 0;
    }
    int count = 0;
    for (int i = 2; i <= n; i++) {
        if (mark[i] == 0) {
            count++;
            for (int j = i * 2; j <= n; j += i) {
                mark[j] = 1;
            }
        }
    }
    return count;
}
