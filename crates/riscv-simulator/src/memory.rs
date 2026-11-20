//! 内存模型：稀疏 RAM + MMIO 地址空间。
//!
//! - RAM 是一段从 0 开始的连续地址空间（默认 64 KiB），未写过的字节读回 0；
//! - MMIO 地址在 RAM 之外，读写走「输入队列 / 输出记录」模型；
//! - 其他地址 → 段错误。
//!
//! 内存按字节存储，便于 `lbu`/`sb` 与字符串关卡。

use std::collections::{BTreeMap, HashMap};

use crate::config::MmioDirection;

/// RAM 末端（含），也是默认 RAM 大小。
pub const MEMORY_TOP: u32 = 0x1_0000;

/// 显示端口（写一个字符）。
pub const MMIO_DISPLAY: u32 = 0xF000_0000;
/// 输入端口（读一个字符）。
pub const MMIO_INPUT: u32 = 0xF000_0004;
/// 魔力计（只读：已消耗周期数）。
pub const MMIO_MANA: u32 = 0xF000_0008;

/// 内存访问错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemError {
    #[error("段错误：地址 0x{addr:08X} 不在可访问范围内（RAM 0x0–0x{top:08X}，或 MMIO 区）")]
    SegFault { addr: u32, top: u32 },
    #[error("访问跨越了地址空间边界：0x{addr:08X} + {size} 字节")]
    OutOfBounds { addr: u32, size: u32 },
}

/// 一段内存区域（地址 → 字节）。
pub type MemoryImage = BTreeMap<u32, u8>;

/// 内存子系统。
#[derive(Debug, Clone)]
pub struct Memory {
    /// RAM 字节数组
    ram: Vec<u8>,
    /// MMIO 映射表
    mmio_map: HashMap<u32, MmioDirection>,
    /// MMIO 输入队列（地址 → FIFO）
    pub(crate) input_queues: HashMap<u32, std::collections::VecDeque<u32>>,
    /// MMIO 输出记录（地址 → append-only 序列）
    pub(crate) output_log: HashMap<u32, Vec<u32>>,
    /// 上一次读 MMIO 时队列是否为空
    pub(crate) last_read_was_empty: bool,
}

impl Memory {
    /// 新建内存，指定 RAM 字节数。
    pub fn new(ram_size: u32) -> Self {
        Self {
            ram: vec![0u8; ram_size as usize],
            mmio_map: HashMap::new(),
            input_queues: HashMap::new(),
            output_log: HashMap::new(),
            last_read_was_empty: false,
        }
    }

    pub fn ram_size(&self) -> u32 {
        self.ram.len() as u32
    }

    /// 注册 MMIO 地址。
    pub fn map_mmio(&mut self, addr: u32, direction: MmioDirection) {
        self.mmio_map.insert(addr, direction);
    }

    /// 该地址是否为 MMIO。
    pub fn is_mmio(&self, addr: u32) -> bool {
        self.mmio_map.contains_key(&addr)
    }

    /// 该地址是否在 RAM 内。
    pub fn in_ram(&self, addr: u32) -> bool {
        addr < self.ram.len() as u32
    }

    /// 读一个字节。MMIO 输入地址走队列；输出地址与未映射地址返回 0。
    pub fn read_byte(&mut self, addr: u32) -> u8 {
        if let Some(dir) = self.mmio_map.get(&addr).copied() {
            return match dir {
                MmioDirection::Input => {
                    let q = self.input_queues.entry(addr).or_default();
                    match q.pop_front() {
                        Some(v) => {
                            self.last_read_was_empty = false;
                            (v & 0xFF) as u8
                        }
                        None => {
                            self.last_read_was_empty = true;
                            0
                        }
                    }
                }
                MmioDirection::Output => 0,
            };
        }
        if (addr as usize) < self.ram.len() {
            self.ram[addr as usize]
        } else {
            0
        }
    }

    /// 读一个字（小端）。
    ///
    /// **MMIO 例外**：如果基地址是映射过的外设端口，就只取**一个**队列元素、
    /// 按低 8 位返回，不去碰后面的地址。
    ///
    /// 为什么必须这样：输入端口只映射了 `0xF0000004` 这一个地址，
    /// `addr+1`…`addr+3` 都没映射，会落到 RAM 读出 0。以前这里连着调 4 次
    /// `read_byte`，行为**碰巧一样**（后 3 次读到 0），但"取了 4 个字节"这个直觉是错的。
    /// 现在把宽度问题显式化：**从外设读就是取走一个元素**，
    /// 所以 `lw` / `lh` / `lbu` / `lb` 从输入端口读**都是一个字符**（有测试钉住）。
    pub fn read_word(&mut self, addr: u32) -> u32 {
        if let Some(v) = self.read_mmio_element(addr) {
            return v & 0xFF;
        }
        let b0 = self.read_byte(addr) as u32;
        let b1 = self.read_byte(addr + 1) as u32;
        let b2 = self.read_byte(addr + 2) as u32;
        let b3 = self.read_byte(addr + 3) as u32;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }

    /// 读半个字（小端）。MMIO 端口同 [`Self::read_word`]：只消费一个元素。
    pub fn read_half(&mut self, addr: u32) -> u16 {
        if let Some(v) = self.read_mmio_element(addr) {
            return (v & 0xFF) as u16;
        }
        let b0 = self.read_byte(addr) as u16;
        let b1 = self.read_byte(addr + 1) as u16;
        b0 | (b1 << 8)
    }

    /// 若 `addr` 是映射过的外设端口，取走一个元素；否则 `None`（调用方按 RAM 走）。
    fn read_mmio_element(&mut self, addr: u32) -> Option<u32> {
        let dir = self.mmio_map.get(&addr).copied()?;
        Some(match dir {
            MmioDirection::Input => match self.input_queues.entry(addr).or_default().pop_front() {
                Some(v) => {
                    self.last_read_was_empty = false;
                    v
                }
                None => {
                    self.last_read_was_empty = true;
                    0
                }
            },
            MmioDirection::Output => 0,
        })
    }

    /// 写一个字节。MMIO 输出地址记录输出；输入地址与非法地址按调用方约定处理。
    pub fn write_byte(&mut self, addr: u32, value: u8) {
        if let Some(dir) = self.mmio_map.get(&addr).copied() {
            match dir {
                MmioDirection::Output => {
                    self.output_log.entry(addr).or_default().push(value as u32);
                }
                MmioDirection::Input => { /* 写输入端口无效果 */ }
            }
            return;
        }
        if (addr as usize) < self.ram.len() {
            self.ram[addr as usize] = value;
        }
    }

    /// 写一个字（小端）。
    pub fn write_word(&mut self, addr: u32, value: u32) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr + 1, ((value >> 8) & 0xFF) as u8);
        self.write_byte(addr + 2, ((value >> 16) & 0xFF) as u8);
        self.write_byte(addr + 3, ((value >> 24) & 0xFF) as u8);
    }

    /// 写半个字（小端）。
    pub fn write_half(&mut self, addr: u32, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr + 1, ((value >> 8) & 0xFF) as u8);
    }

    /// 把数据写入 RAM（越界部分抛错）。
    pub fn load_image(&mut self, image: &MemoryImage) -> Result<(), MemError> {
        for (&addr, &byte) in image {
            if !self.in_ram(addr) {
                return Err(MemError::SegFault { addr, top: self.ram_size() - 1 });
            }
            self.ram[addr as usize] = byte;
        }
        Ok(())
    }

    /// 把二进制数据写入 RAM。
    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) -> Result<(), MemError> {
        for (i, &b) in data.iter().enumerate() {
            let a = addr + i as u32;
            if !self.in_ram(a) {
                return Err(MemError::SegFault { addr: a, top: self.ram_size() - 1 });
            }
            self.ram[a as usize] = b;
        }
        Ok(())
    }

    /// 读一段字节。
    pub fn read_bytes(&self, addr: u32, len: u32) -> Vec<u8> {
        (0..len)
            .map(|i| {
                let a = addr.wrapping_add(i) as usize;
                if a < self.ram.len() { self.ram[a] } else { 0 }
            })
            .collect()
    }

    /// 当前 RAM 快照（稀疏：只保留非零字节）。
    pub fn snapshot_nonzero(&self) -> MemoryImage {
        let mut out = BTreeMap::new();
        for (i, &b) in self.ram.iter().enumerate() {
            if b != 0 {
                out.insert(i as u32, b);
            }
        }
        out
    }

    /// 清空全部 RAM。
    pub fn clear_ram(&mut self) {
        self.ram.iter_mut().for_each(|b| *b = 0);
    }

    /// 写入 RAM 原始字节（不做边界错误的静默截断，用于引导代码）。
    pub(crate) fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    // ── MMIO 队列 ──

    /// 向 MMIO 输入队列追加数据。
    pub fn push_mmio(&mut self, addr: u32, values: &[u32]) {
        let q = self.input_queues.entry(addr).or_default();
        for &v in values {
            q.push_back(v);
        }
    }

    /// 清空全部 MMIO 队列与输出记录。
    pub fn clear_mmio(&mut self) {
        self.input_queues.clear();
        self.output_log.clear();
        self.last_read_was_empty = false;
    }

    /// 某地址的输出记录。
    pub fn mmio_output(&self, addr: u32) -> &[u32] {
        self.output_log.get(&addr).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// 某地址的输出记录（按字节，低 8 位）。
    pub fn mmio_output_bytes(&self, addr: u32) -> Vec<u8> {
        self.mmio_output(addr).iter().map(|v| (v & 0xFF) as u8).collect()
    }

    /// 全部 MMIO 输出记录。
    pub fn all_mmio_outputs(&self) -> HashMap<u32, Vec<u32>> {
        self.output_log.clone()
    }

    /// 恢复 MMIO 输出记录（`reset()` 时保留输出历史用）。
    pub fn restore_outputs(&mut self, outputs: HashMap<u32, Vec<u32>>) {
        self.output_log = outputs;
    }

    /// 导出输入队列（`reset()` 时保留待读数据用）。
    pub fn export_inputs(&self) -> Vec<(u32, Vec<u32>)> {
        self.input_queues.iter().map(|(&a, q)| (a, q.iter().copied().collect())).collect()
    }

    /// 恢复输入队列（覆盖式）。
    pub fn restore_inputs(&mut self, queues: Vec<(u32, Vec<u32>)>) {
        self.input_queues.clear();
        for (addr, values) in queues {
            self.push_mmio(addr, &values);
        }
    }

    /// 某地址输入队列剩余长度。
    pub fn mmio_input_len(&self, addr: u32) -> usize {
        self.input_queues.get(&addr).map(|q| q.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_reads_default_to_zero_and_roundtrip() {
        let mut m = Memory::new(0x1000);
        assert_eq!(m.read_byte(0x100), 0);
        m.write_byte(0x100, 0xAB);
        assert_eq!(m.read_byte(0x100), 0xAB);
        m.write_word(0x200, 0xDEAD_BEEF);
        assert_eq!(m.read_word(0x200), 0xDEAD_BEEF);
        assert_eq!(m.read_byte(0x200), 0xEF, "小端序");
        assert_eq!(m.read_byte(0x203), 0xDE);
    }

    #[test]
    fn signed_half_and_byte_reads() {
        let mut m = Memory::new(0x100);
        m.write_byte(0x10, 0xFF);
        assert_eq!(m.read_byte(0x10) as i8, -1);
        m.write_half(0x20, 0xFFFF);
        assert_eq!(m.read_half(0x20) as i16, -1);
    }

    #[test]
    fn mmio_input_is_fifo_and_reports_empty() {
        let mut m = Memory::new(0x1000);
        m.map_mmio(MMIO_INPUT, MmioDirection::Input);
        m.push_mmio(MMIO_INPUT, &[10, 20]);
        assert_eq!(m.read_byte(MMIO_INPUT), 10);
        assert!(!m.last_read_was_empty);
        assert_eq!(m.read_byte(MMIO_INPUT), 20);
        assert_eq!(m.read_byte(MMIO_INPUT), 0);
        assert!(m.last_read_was_empty);
    }

    #[test]
    fn mmio_output_is_logged() {
        let mut m = Memory::new(0x1000);
        m.map_mmio(MMIO_DISPLAY, MmioDirection::Output);
        m.write_word(MMIO_DISPLAY, b'A' as u32);
        m.write_word(MMIO_DISPLAY, b'B' as u32);
        assert_eq!(m.mmio_output_bytes(MMIO_DISPLAY), b"AB".to_vec());
    }

    /// 一次 `lw` 从输入端口只消费**一个**元素（不是 4 个、也不是 0.25 个）。
    ///
    /// 这条以前是错的：`read_word` 连着读 4 个地址，但外设只映射了基地址，
    /// 于是后面三个地址落到 RAM 读出 0 —— 结果"低字节对、但只吃掉一个字符"，
    /// 而且没人说得清到底吃几个。写关卡的人按"读 4 字节"推算就会踩空。
    #[test]
    fn mmio_input_word_read_consumes_exactly_one_element() {
        let mut m = Memory::new(0x1000);
        m.map_mmio(MMIO_INPUT, MmioDirection::Input);
        m.push_mmio(MMIO_INPUT, &[65, 66, 67]);

        assert_eq!(m.read_word(MMIO_INPUT), 65, "低字节应该是下一个字符");
        assert_eq!(m.mmio_input_len(MMIO_INPUT), 2, "一次读只该消费一个元素");
        assert_eq!(m.read_half(MMIO_INPUT), 66);
        assert_eq!(m.mmio_input_len(MMIO_INPUT), 1);
        assert_eq!(m.read_word(MMIO_INPUT), 67);
        assert_eq!(m.mmio_input_len(MMIO_INPUT), 0);

        // 读空：返回 0 并标记"读空了"（运行时据此暂停）
        assert_eq!(m.read_word(MMIO_INPUT), 0);
        assert!(m.last_read_was_empty);
    }

    /// 外设端口后面的地址不该被卷进来：RAM 里的数据要原样读出来。
    #[test]
    fn ram_reads_are_unaffected_by_the_mmio_fix() {
        let mut m = Memory::new(0x1000);
        m.map_mmio(MMIO_INPUT, MmioDirection::Input);
        m.write_word(0x200, 0xDEAD_BEEF);
        assert_eq!(m.read_word(0x200), 0xDEAD_BEEF);
        assert_eq!(m.read_half(0x200), 0xBEEF);
    }

    #[test]
    fn snapshot_keeps_only_nonzero() {
        let mut m = Memory::new(0x100);
        m.write_byte(0x10, 1);
        m.write_byte(0x20, 2);
        let snap = m.snapshot_nonzero();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap.get(&0x10), Some(&1));
    }
}
