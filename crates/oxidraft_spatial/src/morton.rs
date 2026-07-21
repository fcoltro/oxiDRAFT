//! Morton (Z-order) codes: interleaving two 32-bit coordinates into one 64-bit
//! key so that spatially nearby points get nearby codes.

/// The Morton code of `(x, y)` — the bits of `x` and `y` interleaved.
pub fn morton_code(x: u32, y: u32) -> u64 {
    interleave(x as u64) | (interleave(y as u64) << 1)
}

fn interleave(mut n: u64) -> u64 {
    n &= 0x0000_0000_FFFF_FFFF;
    n = (n | (n << 16)) & 0x0000_FFFF_0000_FFFF;
    n = (n | (n << 8)) & 0x00FF_00FF_00FF_00FF;
    n = (n | (n << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
    n = (n | (n << 2)) & 0x3333_3333_3333_3333;
    n = (n | (n << 1)) & 0x5555_5555_5555_5555;
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn morton_origin() {
        assert_eq!(morton_code(0, 0), 0);
    }
    #[test]
    fn morton_1_0() {
        assert_eq!(morton_code(1, 0), 1);
    }
    #[test]
    fn morton_0_1() {
        assert_eq!(morton_code(0, 1), 2);
    }
    #[test]
    fn morton_1_1() {
        assert_eq!(morton_code(1, 1), 3);
    }
}
