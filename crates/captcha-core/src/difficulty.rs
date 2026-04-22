/// 计算字节切片的前导零比特数。
/// 用于判定 Argon2 哈希是否满足难度要求。
pub fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut count = 0u32;
    for &byte in hash {
        if byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros();
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zeros() {
        assert_eq!(leading_zero_bits(&[0, 0, 0, 0]), 32);
    }

    #[test]
    fn first_byte_nonzero() {
        assert_eq!(leading_zero_bits(&[0x80]), 0);
        assert_eq!(leading_zero_bits(&[0x40]), 1);
        assert_eq!(leading_zero_bits(&[0x0F]), 4);
        assert_eq!(leading_zero_bits(&[0x01]), 7);
    }

    #[test]
    fn mixed_bytes() {
        assert_eq!(leading_zero_bits(&[0x00, 0x01]), 15);
        assert_eq!(leading_zero_bits(&[0x00, 0x00, 0x08]), 20);
        assert_eq!(leading_zero_bits(&[0x00, 0x00, 0x00, 0x01]), 31);
    }

    #[test]
    fn empty_slice() {
        assert_eq!(leading_zero_bits(&[]), 0);
    }

    #[test]
    fn boundary_values() {
        assert_eq!(leading_zero_bits(&[0x00, 0x80]), 8);
        assert_eq!(leading_zero_bits(&[0x00, 0x00, 0x80]), 16);
    }
}
