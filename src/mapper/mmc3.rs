use crate::bus::Bus;
use crate::cartridge::Cartridge;
use crate::cpu::Interrupt;
use crate::debug;
use crate::mapper::Mapper;
use crate::ppu::MirroringMode;
#[cfg(not(target_arch = "wasm32"))]
use serde_derive::{Deserialize, Serialize};

#[derive(Debug)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Serialize))]
enum PrgRomBankMode {
    // prg rom is two switchable 8K banks and two fixed 8K banks on last two banks
    TwoSwitchTwoFix,
    // prg rom is one fixed 8K bank on the second last bank, two switchable 8K banks, and one
    // fixed 8K bank on the last bank
    FixTwoSwitchFix,
}

impl Default for PrgRomBankMode {
    fn default() -> Self {
        PrgRomBankMode::TwoSwitchTwoFix
    }
}

#[derive(Debug)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Serialize))]
enum ChrRomBankMode {
    // chr rom is two switchable 2K banks and four switchable 1K banks
    Two2KFour1K,
    // chr rom is four switchable 1K banks and two switchable 2K banks
    Four1KTwo2K,
}

impl Default for ChrRomBankMode {
    fn default() -> Self {
        ChrRomBankMode::Two2KFour1K
    }
}

#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Serialize))]
struct Registers {
    mirroring_mode: MirroringMode,
    prg_rom_bank_mode: PrgRomBankMode,
    chr_rom_bank_mode: ChrRomBankMode,
    prg_ram_writes_enabled: bool,
    prg_ram_enabled: bool,
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    bank_data: [u8; 8],
    current_bank: u8,
}

impl Registers {
    pub fn new() -> Self {
        Registers {
            mirroring_mode: MirroringMode::Vertical,
            prg_rom_bank_mode: PrgRomBankMode::default(),
            chr_rom_bank_mode: ChrRomBankMode::default(),
            prg_ram_writes_enabled: true,
            prg_ram_enabled: true,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            bank_data: [0; 8],
            current_bank: 0,
        }
    }

    pub fn write_bank_select(&mut self, val: u8) {
        self.prg_rom_bank_mode = if val & 0x40 == 0 {
            PrgRomBankMode::TwoSwitchTwoFix
        } else {
            PrgRomBankMode::FixTwoSwitchFix
        };
        debug!(
            "[MMC3] Write prg rom bank mode: {:?}.",
            self.prg_rom_bank_mode
        );

        self.chr_rom_bank_mode = if val & 0x80 == 0 {
            ChrRomBankMode::Two2KFour1K
        } else {
            ChrRomBankMode::Four1KTwo2K
        };
        debug!(
            "[MMC3] Write chr rom bank mode: {:?}.",
            self.chr_rom_bank_mode
        );

        self.current_bank = val & 0x07;
        debug!("[MMC3] Write current bank: {}.", self.current_bank);
    }

    pub fn write_bank_data(&mut self, val: u8) {
        self.bank_data[self.current_bank as usize] = val;
        debug!("[MMC3] Write bank data: {}.", val);
    }

    pub fn write_mirroring_mode(&mut self, val: u8) {
        self.mirroring_mode = if val & 0x01 == 0 {
            MirroringMode::Vertical
        } else {
            MirroringMode::Horizontal
        };
        debug!("[MMC3] Write mirroring mode: {:?}.", self.mirroring_mode);
    }

    pub fn write_prg_ram_protect(&mut self, val: u8) {
        self.prg_ram_writes_enabled = val & 0x40 == 0;
        self.prg_ram_enabled = val & 0x80 != 0;
    }

    pub fn get_chr_rom_address(&self, addr: usize) -> usize {
        match self.chr_rom_bank_mode {
            ChrRomBankMode::Two2KFour1K => match addr {
                0x0000..=0x07FF => (self.bank_data[0] as usize & !0x01) * 0x400 + addr,
                0x0800..=0x0FFF => (self.bank_data[1] as usize & !0x01) * 0x400 + addr - 0x0800,
                0x1000..=0x13FF => (self.bank_data[2] as usize) * 0x400 + addr - 0x1000,
                0x1400..=0x17FF => (self.bank_data[3] as usize) * 0x400 + addr - 0x1400,
                0x1800..=0x1BFF => (self.bank_data[4] as usize) * 0x400 + addr - 0x1800,
                0x1C00..=0x1FFF => (self.bank_data[5] as usize) * 0x400 + addr - 0x1C00,
                _ => panic!("[MMC3] Invalid chr rom address."),
            },
            ChrRomBankMode::Four1KTwo2K => match addr {
                0x0000..=0x03FF => (self.bank_data[2] as usize) * 0x400 + addr,
                0x0400..=0x07FF => (self.bank_data[3] as usize) * 0x400 + addr - 0x0400,
                0x0800..=0x0BFF => (self.bank_data[4] as usize) * 0x400 + addr - 0x0800,
                0x0C00..=0x0FFF => (self.bank_data[5] as usize) * 0x400 + addr - 0x0C00,
                0x1000..=0x17FF => (self.bank_data[0] as usize & !0x01) * 0x400 + addr - 0x1000,
                0x1800..=0x1FFF => (self.bank_data[1] as usize & !0x01) * 0x400 + addr - 0x1800,
                _ => panic!("[MMC3] Invalid chr rom address."),
            },
        }
    }

    pub fn get_prg_rom_address(&self, addr: usize, prg_rom_banks: usize) -> usize {
        match self.prg_rom_bank_mode {
            PrgRomBankMode::TwoSwitchTwoFix => match addr {
                0x8000..=0x9FFF => (self.bank_data[6] as usize) * 0x2000 + addr - 0x8000,
                0xA000..=0xBFFF => (self.bank_data[7] as usize) * 0x2000 + addr - 0xA000,
                0xC000..=0xDFFF => (prg_rom_banks - 2) * 0x2000 + addr - 0xC000,
                0xE000..=0xFFFF => (prg_rom_banks - 1) * 0x2000 + addr - 0xE000,
                _ => panic!("[MMC3] Invalid prg rom address."),
            },
            PrgRomBankMode::FixTwoSwitchFix => match addr {
                0x8000..=0x9FFF => (prg_rom_banks - 2) * 0x2000 + addr - 0x8000,
                0xA000..=0xBFFF => (self.bank_data[7] as usize) * 0x2000 + addr - 0xA000,
                0xC000..=0xDFFF => (self.bank_data[6] as usize) * 0x2000 + addr - 0xC000,
                0xE000..=0xFFFF => (prg_rom_banks - 1) * 0x2000 + addr - 0xE000,
                _ => panic!("[MMC3] Invalid prg rom address."),
            },
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Registers::new()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), derive(Deserialize, Serialize))]
pub struct MMC3 {
    #[cfg_attr(
        not(target_arch = "wasm32"),
        serde(skip, default = "Cartridge::empty_cartridge")
    )]
    cartridge: Cartridge,
    r: Registers,
    #[cfg_attr(not(target_arch = "wasm32"), serde(skip))]
    bus: Option<Bus>,
}

impl MMC3 {
    pub fn new(cartridge: Cartridge) -> Self {
        MMC3 {
            cartridge,
            r: Registers::default(),
            bus: None,
        }
    }

    fn bus(&self) -> &Bus {
        self.bus.as_ref().expect("[MMC3] No bus attached.")
    }

    fn bus_mut(&mut self) -> &mut Bus {
        self.bus.as_mut().expect("[MMC3] No bus attached.")
    }
}

impl Mapper for MMC3 {
    fn read_byte(&self, addr: u16) -> u8 {
        let addr = addr as usize;
        match addr {
            0x0000..=0x1FFF => {
                let addr = self.r.get_chr_rom_address(addr);
                self.cartridge.read_chr_rom(addr)
            }
            0x6000..=0x7FFF if self.r.prg_ram_enabled => self.cartridge.read_prg_ram(addr - 0x6000),
            0x8000..=0xFFFF => {
                let prg_rom_banks = self.cartridge.prg_rom_len() / 0x2000;
                let addr = self.r.get_prg_rom_address(addr, prg_rom_banks);
                self.cartridge.read_prg_rom(addr)
            }
            _ => 0,
        }
    }

    fn write_byte(&mut self, addr: u16, val: u8) {
        let addr = addr as usize;
        match addr {
            0x0000..=0x1FFF => {
                let addr = self.r.get_chr_rom_address(addr);
                self.cartridge.write_chr_rom(addr, val);
            }
            0x6000..=0x7FFF if self.r.prg_ram_writes_enabled => {
                self.cartridge.write_prg_ram(addr - 0x6000, val)
            }
            0x8000..=0x9FFF if addr & 0x01 == 0 => self.r.write_bank_select(val),
            0x8000..=0x9FFF => self.r.write_bank_data(val),
            0xA000..=0xBFFF if addr & 0x01 == 0 => self.r.write_mirroring_mode(val),
            0xA000..=0xBFFF => self.r.write_prg_ram_protect(val),
            0xC000..=0xDFFF if addr & 0x01 == 0 => self.r.irq_latch = val,
            0xC000..=0xDFFF => self.r.irq_counter = self.r.irq_latch,
            0xE000..=0xFFFF if addr & 0x01 == 0 => self.r.irq_enabled = false,
            0xE000..=0xFFFF => self.r.irq_enabled = true,
            _ => {}
        }
    }

    fn chr_bank(&self, mut index: usize) -> *const u8 {
        index = match self.r.chr_rom_bank_mode {
            ChrRomBankMode::Two2KFour1K => match index {
                0 => self.r.bank_data[0] as usize & !0x01,
                1 => self.r.bank_data[0] as usize | 0x01,
                2 => self.r.bank_data[1] as usize & !0x01,
                3 => self.r.bank_data[1] as usize | 0x01,
                4 => self.r.bank_data[2] as usize,
                5 => self.r.bank_data[3] as usize,
                6 => self.r.bank_data[4] as usize,
                7 => self.r.bank_data[5] as usize,
                _ => panic!("Expected index < 8."),
            },
            ChrRomBankMode::Four1KTwo2K => match index {
                0 => self.r.bank_data[2] as usize,
                1 => self.r.bank_data[3] as usize,
                2 => self.r.bank_data[4] as usize,
                3 => self.r.bank_data[5] as usize,
                4 => self.r.bank_data[0] as usize & !0x01,
                5 => self.r.bank_data[0] as usize | 0x01,
                6 => self.r.bank_data[1] as usize & !0x01,
                7 => self.r.bank_data[1] as usize | 0x01,
                _ => panic!("Expected index < 8."),
            },
        };

        self.cartridge.chr_bank(index)
    }

    fn mirroring_mode(&self) -> MirroringMode {
        if self.cartridge.mirroring_mode == MirroringMode::None {
            MirroringMode::None
        } else {
            self.r.mirroring_mode
        }
    }

    fn attach_bus(&mut self, bus: Bus) {
        self.bus = Some(bus);
    }

    fn step(&mut self) {
        let ppu = self.bus().ppu();
        let cycle = ppu.cycle;
        let scanline = ppu.scanline;
        let rendering_enabled = ppu.r.show_sprites || ppu.r.show_background;

        if cycle != 260 || scanline >= 240 || !rendering_enabled {
            return;
        }

        if self.r.irq_counter == 0 {
            self.r.irq_counter = self.r.irq_latch;
        } else {
            self.r.irq_counter -= 1;
            if self.r.irq_counter == 0 && self.r.irq_enabled {
                debug!("[MM3] Triggered interrupt.");
                let cpu = self.bus_mut().cpu_mut();
                cpu.trigger_interrupt(Interrupt::IRQ);
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&self) -> bincode::Result<Option<Vec<u8>>> {
        self.cartridge.save()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load(&mut self, save_data: &[u8]) -> bincode::Result<()> {
        self.cartridge.load(save_data)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_state(&self) -> bincode::Result<(Vec<u8>, Vec<u8>)> {
        Ok((bincode::serialize(&self)?, self.cartridge.save_state()?))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_state(&mut self, mapper_data: &[u8], save_data: &[u8]) -> bincode::Result<()> {
        let mut saved_mapper = bincode::deserialize(mapper_data)?;
        std::mem::swap(self, &mut saved_mapper);
        std::mem::swap(&mut self.cartridge, &mut saved_mapper.cartridge);
        self.load(&save_data)?;
        Ok(())
    }
}
