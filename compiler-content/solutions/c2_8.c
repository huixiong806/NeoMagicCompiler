// 2-8 参考解：递归求斐波那契（fib(0)=0, fib(1)=1）
int fib(int n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int solve(int n) {
    return fib(n);
}
