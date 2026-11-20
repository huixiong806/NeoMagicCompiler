// 2-12 参考解：冒泡排序（原地升序）
void solve(int* a, int n) {
    for (int end = n - 1; end > 0; end--) {
        for (int i = 0; i < end; i++) {
            if (a[i] > a[i + 1]) {
                int t = a[i];
                a[i] = a[i + 1];
                a[i + 1] = t;
            }
        }
    }
}
