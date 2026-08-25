//! Bit-array helpers ported from `src/mapbits.c`.

const MS_ARRAY_BIT: usize = u32::BITS as usize;

pub fn get_bit_array_size(numbits: usize) -> usize {
    numbits.div_ceil(MS_ARRAY_BIT)
}

pub fn alloc_bit_array(numbits: usize) -> Vec<u32> {
    vec![0; get_bit_array_size(numbits)]
}

pub fn get_bit(array: &[u32], index: usize) -> bool {
    let word = index / MS_ARRAY_BIT;
    let bit = index % MS_ARRAY_BIT;
    (array[word] & (1u32 << bit)) != 0
}

pub fn get_next_bit(array: &[u32], mut i: usize, size: usize) -> Option<usize> {
    while i < size {
        let b = array[i / MS_ARRAY_BIT];
        if b != 0 && (b >> (i % MS_ARRAY_BIT)) != 0 {
            if (b & (1u32 << (i % MS_ARRAY_BIT))) != 0 {
                return Some(i);
            }
            i += 1;
        } else {
            i += MS_ARRAY_BIT - (i % MS_ARRAY_BIT);
        }
    }
    None
}

pub fn set_bit(array: &mut [u32], index: usize, value: bool) {
    let word = index / MS_ARRAY_BIT;
    let bit = index % MS_ARRAY_BIT;
    if value {
        array[word] |= 1u32 << bit;
    } else {
        array[word] &= !(1u32 << bit);
    }
}

pub fn set_all_bits(array: &mut [u32], value: bool) {
    array.fill(if value { u32::MAX } else { 0 });
}

pub fn flip_bit(array: &mut [u32], index: usize) {
    let word = index / MS_ARRAY_BIT;
    let bit = index % MS_ARRAY_BIT;
    array[word] ^= 1u32 << bit;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_bit_get_set_flip() {
        let mut bits = alloc_bit_array(70);
        assert!(!get_bit(&bits, 0));
        assert!(!get_bit(&bits, 69));

        set_bit(&mut bits, 0, true);
        set_bit(&mut bits, 69, true);
        assert!(get_bit(&bits, 0));
        assert!(get_bit(&bits, 69));

        flip_bit(&mut bits, 69);
        assert!(!get_bit(&bits, 69));

        set_bit(&mut bits, 0, false);
        assert!(!get_bit(&bits, 0));
    }

    #[test]
    fn finds_next_set_bit() {
        let mut bits = alloc_bit_array(96);
        set_bit(&mut bits, 2, true);
        set_bit(&mut bits, 65, true);
        set_bit(&mut bits, 95, true);

        assert_eq!(get_next_bit(&bits, 0, 96), Some(2));
        assert_eq!(get_next_bit(&bits, 3, 96), Some(65));
        assert_eq!(get_next_bit(&bits, 66, 96), Some(95));
        assert_eq!(get_next_bit(&bits, 96, 96), None);
    }

    #[test]
    fn sets_all_bits() {
        let mut bits = alloc_bit_array(33);
        set_all_bits(&mut bits, true);
        assert!(get_bit(&bits, 0));
        assert!(get_bit(&bits, 32));

        set_all_bits(&mut bits, false);
        assert!(!get_bit(&bits, 0));
        assert!(!get_bit(&bits, 32));
    }
}
