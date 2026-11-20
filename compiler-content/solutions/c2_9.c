// 2-9 参考解：把 n 个符文的能量（power × count）加起来
struct rune {
    int power;
    int count;
};

int solve(struct rune* list, int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += list[i].power * list[i].count;
    }
    return total;
}
