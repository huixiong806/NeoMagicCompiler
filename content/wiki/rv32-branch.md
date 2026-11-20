# RV32IM 分支、标签与循环

CPU 通过 **PC（程序计数器）** 记录下一条要执行的指令。默认情况下 PC 顺序往下走；使用**标签**和**分支/跳转指令**可以改变 PC 的去向：

| 指令 | 格式 | 语义 |
|------|------|------|
| `blt` | `blt rs1, rs2, label` | 若 `rs1 < rs2`（有符号），跳到 `label` |
| `bge` | `bge rs1, rs2, label` | 若 `rs1 >= rs2`（有符号），跳到 `label` |
| `beq` | `beq rs1, rs2, label` | 若 `rs1 == rs2`，跳到 `label` |
| `bne` | `bne rs1, rs2, label` | 若 `rs1 != rs2`，跳到 `label` |
| `j` | `j label` | 无条件跳转到 `label` |

## 两条黄金模式

### 1. 二选一分支（if / else）
```asm
    blt t0, t1, else_branch
    # if 块
    j   done          # 别忘了跳过 else 块！
else_branch:
    # else 块
done:
```

### 2. 前置判断循环（while）
```asm
loop:
    blt t0, t2, done  # 条件不满足先跳出（保证 0 次时也能安全跳过）
    # 循环体
    j   loop          # 往回跳 = 循环！
done:
```
