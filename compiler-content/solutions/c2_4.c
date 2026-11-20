// 2-4 参考解：返回最大值的下标（并列时取最小的下标）
int solve(int* a, int n) {
    int best = 0;
    for (int i = 1; i < n; i++) {
        if (a[i] > a[best]) {
            best = i;
        }
    }
    return best;
}
