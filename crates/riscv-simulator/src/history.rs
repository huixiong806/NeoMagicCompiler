//! 执行历史：每条指令执行前记录"将被覆盖的旧值"，支持单步倒退与任意回看。
//!
//! 设计见 `documents/架构.md` §7「历史回溯的设计」：
//! 只记录"被覆盖的旧值"（而不是快照整台机器）—— 99% 的指令只改 1 个寄存器 + PC，
//! 所以一条记录很轻（几十字节）。容量满后环形淘汰最老记录，
//! `base_tick` 记下最老可回退点。

use std::collections::VecDeque;

/// 一条历史记录：执行第 `tick` 条指令**之前**的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryEntry {
    /// 指令序号（从 1 开始；执行前 PC 所在的那条指令的序号）
    pub tick: u64,
    /// 执行前的 PC
    pub old_pc: u32,
    /// 被覆盖的寄存器 (索引, 旧值)；`None` 表示没改寄存器
    pub old_reg: Option<(u8, u32)>,
    /// 被覆盖的内存字节 (地址, 旧值)；`None` 表示没写内存
    pub old_mem: Option<(u32, u8)>,
}

/// 环形历史缓冲。
#[derive(Debug, Clone)]
pub struct History {
    entries: VecDeque<HistoryEntry>,
    capacity: usize,
    /// 已被淘汰的最老 tick（`entries` 的第一条对应 tick = base_tick + 1）
    base_tick: u64,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self { entries: VecDeque::with_capacity(capacity.min(4096)), capacity, base_tick: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 最老可回退到的 tick 号（= 已被淘汰的最后一个 tick）。
    pub fn base_tick(&self) -> u64 {
        self.base_tick
    }

    /// 记录一条（执行**前**调用）。
    pub fn push(&mut self, entry: HistoryEntry) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() == self.capacity {
            if let Some(dropped) = self.entries.pop_front() {
                self.base_tick = dropped.tick;
            }
        }
        self.entries.push_back(entry);
    }

    /// 弹出最近一条（倒退一步）。
    pub fn pop(&mut self) -> Option<HistoryEntry> {
        let e = self.entries.pop_back();
        if e.is_none() {
            // 历史空了：下一次记录从当前 base 之后继续
        }
        e
    }

    /// 弹出最老的若干条，直到剩余记录中最老的一条 tick `>= tick`。
    ///
    /// 语义：调用后仍可以回退到 `tick`（即 `front().tick == tick`）。
    pub fn pop_until(&mut self, tick: u64) -> usize {
        let mut n = 0;
        while let Some(front) = self.entries.front() {
            if front.tick < tick {
                if let Some(dropped) = self.entries.pop_front() {
                    self.base_tick = self.base_tick.max(dropped.tick);
                }
                n += 1;
            } else {
                break;
            }
        }
        n
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.base_tick = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tick: u64) -> HistoryEntry {
        HistoryEntry { tick, old_pc: 0, old_reg: None, old_mem: None }
    }

    #[test]
    fn push_and_pop_lifo() {
        let mut h = History::new(4);
        h.push(entry(1));
        h.push(entry(2));
        assert_eq!(h.pop().unwrap().tick, 2);
        assert_eq!(h.pop().unwrap().tick, 1);
        assert!(h.pop().is_none());
    }

    #[test]
    fn capacity_evicts_oldest_and_tracks_base() {
        let mut h = History::new(2);
        h.push(entry(1));
        h.push(entry(2));
        h.push(entry(3));
        assert_eq!(h.len(), 2);
        assert_eq!(h.base_tick(), 1, "最老的可回退点是 tick 1");
        assert_eq!(h.pop().unwrap().tick, 3);
        assert_eq!(h.pop().unwrap().tick, 2);
        assert!(h.pop().is_none(), "tick 1 已被淘汰");
    }

    #[test]
    fn pop_until_removes_everything_before_target() {
        let mut h = History::new(8);
        for t in 1..=5 {
            h.push(entry(t));
        }
        assert_eq!(h.pop_until(3), 2, "tick 1、2 被弹出，tick 3 保留以便回退到它");
        assert_eq!(h.len(), 3, "剩下 tick 3、4、5");
        assert_eq!(h.pop().unwrap().tick, 5);
        assert_eq!(h.pop().unwrap().tick, 4);
        assert_eq!(h.pop().unwrap().tick, 3, "tick 3 仍然可以回退");
    }
}
