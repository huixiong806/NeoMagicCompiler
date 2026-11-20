// 2-5 参考解：通过指针交换两个格子里的数
void solve(int* a, int* b) {
    int t = *a;
    *a = *b;
    *b = t;
}
