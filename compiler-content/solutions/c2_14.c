// 2-14 参考解：能量配比
//
// 规则（叙事里写清楚了）：
//   1. 每种元素先分 1 单位「底价」；若 budget < n（连底价都不够）返回 0；
//   2. rest = budget - n 是"还能再分的量"；
//   3. 从第 0 种开始依次尽量多分：第 i 种最多还能再加 recipe[i] 单位
//      （也就是它总上限是 recipe[i] + 1）；rest 够就全给它，不够就把剩下的全给它；
//   4. 全部处理完后 rest 恰好为 0 → 配平返回 1，否则返回 0。
//
// 等价的一句话：把每种元素的**上限**加起来（sum(recipe[i] + 1)），
// 正好等于预算才配平 —— 少了装不满、多了没处放。
int solve(int* recipe, int n, int budget) {
    if (budget < n) {
        return 0;
    }
    int rest = budget - n;
    for (int i = 0; i < n; i++) {
        if (rest == 0) {
            break;
        }
        if (recipe[i] <= rest) {
            rest = rest - recipe[i];
        } else {
            rest = 0;
        }
    }
    if (rest == 0) {
        return 1;
    }
    return 0;
}
