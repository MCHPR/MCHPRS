#[derive(Default, Debug, Clone, Copy)]
pub struct BitQueue([u64; 4]);

impl BitQueue {
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
        let mask = 1 << (location & 0b111111);
        if value {
            self.0[location as usize >> 6] |= mask;
        } else {
            self.0[location as usize >> 6] &= !mask;
        }
    }

    pub fn peek_bit(&self, location: u8) -> bool {
        let index = (location as usize) >> 6;
        let pos = location & 0b111111;
        ((self.0[index] >> pos) & 0b1) == 1
    }

    pub fn pop_bit(&mut self) -> bool {
        let mut overflow = false;
        for num in self.0.iter_mut().rev() {
            let new_overflow = *num & 1 == 1;
            *num >>= 1;
            *num |= (overflow as u64) << 63;
            overflow = new_overflow;
        }
        overflow
    }

    pub fn all_bits_same(&self, length: u8) -> bool {
        let ones: u32 = self.0.iter().map(|x| x.count_ones()).sum();
        ones == 0 || ones == length as u32
    }

    pub fn set_nibble(&mut self, location: u8, value: u8) {
        let mask = 0b1111 << ((location & 0b1111) * 4);
        let val_shifted = (value & 0b1111) << ((location & 0b1111) * 4);
        let index = (location as usize) >> 4;
        self.0[index] = (self.0[index] & !mask) | val_shifted as u64;
    }

    pub fn peek_nibble(&self, location: u8) -> u8 {
        let index = (location as usize) >> 4;
        let pos = (location & 0b1111) * 4;
        ((self.0[index] >> pos) & 0b1111) as u8
    }

    pub fn pop_nibble(&mut self) -> u8 {
        let mut overflow = 0;
        for num in self.0.iter_mut().rev() {
            let new_overflow = (*num & 0b1111) as u8;
            *num >>= 4;
            *num |= (overflow as u64) << 60;
            overflow = new_overflow;
        }
        overflow
    }

    pub fn all_nibbles_same(&self, length: u8) -> bool {
        let ref_nibble = self.0[0] & 0b1111;
        for i in 1..(length as usize) {
            let nibble = self.0[i >> 4] >> ((i & 0b1111) * 4);
            if nibble != ref_nibble {
                return false;
            }
        }
        true
    }
}
