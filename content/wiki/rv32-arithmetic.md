# RV32IM 算术与搬运指令

所有三操作数算术指令都遵循**「目标在最左，输入在右侧」**的统一格式：

| 指令 | 格式 | 语义 | 示例 |
|------|------|------|------|
| `add` | `add rd, rs1, rs2` | `rd = rs1 + rs2` | `add t2, t0, t1` |
| `addi` | `addi rd, rs1, imm` | `rd = rs1 + imm`（12 位有符号常量 `-2048..2047`） | `addi t1, t0, 7` |
| `sub` | `sub rd, rs1, rs2` | `rd = rs1 - rs2`（顺序敏感） | `sub t2, t0, t1` |
| `mv` | `mv rd, rs` | `rd = rs`（伪指令，等价于 `addi rd, rs, 0`） | `mv t3, t0` |
| `mul` | `mul rd, rs1, rs2` | `rd = rs1 × rs2`（取低 32 位） | `mul t4, t2, t1` |
| `div` | `div rd, rs1, rs2` | `rd = rs1 / rs2`（向零截断，如 `-17 / 5 = -3`） | `div t2, t0, t1` |
| `rem` | `rem rd, rs1, rs2` | `rd = rs1 % rs2`（余数符号跟随被除数） | `rem t3, t0, t1` |

## 实用技巧
- 构造小常量：`addi t1, zero, 42`
- 寄存器清零：`addi t1, zero, 0` 或 `mv t1, zero`
- 寄存器取相反数：`sub t1, zero, t0`
