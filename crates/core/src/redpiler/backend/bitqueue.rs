#[derive(Default, Debug, Clone, Copy)]
pub struct BitQueue(u128);

impl BitQueue {
    pub const MAX_BITS: usize = 128;
    pub const MAX_NIBBLES: usize = 32;

    pub fn new_bits(length: u8, value: bool) -> BitQueue {
        let mut bq = BitQueue::default();
        if value {
            for i in 0..length {
                bq.set_bit(i, value);
            }
        }
        bq
    }

    pub fn new_nibbles(length: u8, value: u8) -> BitQueue {
        let mut bq = BitQueue::default();
        if value != 0 {
            for i in 0..length {
                bq.set_nibble(i, value);
            }
        }
        bq
    }

    pub fn set_bit(&mut self, location: u8, value: bool) {
        let mask = 1 << location;
        if value {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }

    pub fn get_bit(&self, location: u8) -> bool {
        ((self.0 >> location) & 0b1) == 1
    }

    pub fn shift_bit(&mut self) {
        self.0 >>= 1;
    }

    pub fn all_bits_same(&self, length: u8) -> bool {
        let ones: u32 = self.0.count_ones();
        ones == 0 || ones == length as u32
    }

    pub fn set_nibble(&mut self, location: u8, value: u8) {
        let mask = 0b1111 << ((location & 0b1111) * 4);
        let val_shifted = ((value & 0b1111) as u128) << (location * 4);
        self.0 = (self.0 & !mask) | val_shifted;
    }

    pub fn get_nibble(&self, location: u8) -> u8 {
        let pos = (location & 0b1111) * 4;
        ((self.0 >> pos) & 0b1111) as u8
    }

    pub fn shift_nibble(&mut self) {
        self.0 >>= 4;
    }

    pub fn all_nibbles_same(&self, length: u8) -> bool {
        let ref_nibble = self.0 & 0b1111;
        for i in 1..(length as usize) {
            let nibble = self.0 >> ((i & 0b1111) * 4);
            if nibble != ref_nibble {
                return false;
            }
        }
        true
    }
}
