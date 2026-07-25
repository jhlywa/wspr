// ## References
// - [1] http://g4jnt.com/WSPR_Coding_Process.pdf
// - [2] https://www.wsprnet.org/drupal/sites/wsprnet.org/files/si570wspr.pdf
// - [3] https://ocw.mit.edu/courses/6-02-introduction-to-eecs-ii-digital-communication-systems-fall-2012/6bb326417ac947837cce79d334c8ac1c_MIT6_02F12_chap07.pdf
// - [4] https://www.cs.princeton.edu/courses/archive/spring19/cos463/lectures/L09-viterbi.pdf
//
// TODO: try shifting bits in from the MSB of the shift register

#![cfg_attr(not(test), no_std)]

const SYMBOLS: usize = 162;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Error {
    InvalidPower,
    InvalidGrid,
    InvalidCallsign,
}

// Typically we'd just use a Vec here, but this crate is designed for no_std,
// so we'll implement this by hand.
struct BitVec {
    bits: [u8; SYMBOLS],
    index: usize,
}

impl BitVec {
    fn new() -> Self {
        Self {
            bits: [0u8; SYMBOLS],
            index: 0,
        }
    }

    fn push(&mut self, bit: u8) {
        self.bits[self.index] = bit;
        self.index += 1;
    }

    fn interleave(&mut self) {
        let mut interleaved = [0u8; SYMBOLS];
        let mut p = 0;
        for i in 0u8..255 {
            let j = i.reverse_bits() as usize;
            if j < SYMBOLS {
                interleaved[j] = self.bits[p];
                p += 1;
                if p == SYMBOLS {
                    break;
                }
            }
        }

        self.bits = interleaved;
    }

    // Converts each bit to a symbol between 0-3 using a preshared sync word.
    fn sync(&mut self) {
        const SYNC: [u8; SYMBOLS] = [
            1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0,
            0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 0,
            0, 0, 1, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 1, 0, 1, 1,
            0, 0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0,
            0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0, 0,
        ];

        for i in 0..SYNC.len() {
            self.bits[i] = SYNC[i] + 2 * self.bits[i];
        }
    }

    fn inner(self) -> [u8; SYMBOLS] {
        self.bits
    }
}

// The WSPR convolutional encoder uses an internal 32-bit shift register which:
//
//   - shifts a bit (k) into the least significant bit of the register
//   - performs a bitwise AND with two polynomials (two 32-bit constants g0 and g1)
//   - sums the number of set bits in each AND result
//   - returns the a pair (n0, n1) indicating the parity of each term (0 = even, 1 = odd)
//
// The encoder has the following properties:
//
//    k = 1 : one bit input
//    n = 2 : two bits output
//    K = 32: constraint length of 32 (the number of stages in the shift register)
//
//    Rate r = k/n = 1/2
//
//  See [3] and [4] for more info on convolutional encoders.
//
struct ConvolutionalEncoder {
    g0: u32,
    g1: u32,
    register: u32,
}

impl ConvolutionalEncoder {
    fn new(g0: u32, g1: u32) -> Self {
        Self {
            g0,
            g1,
            register: 0,
        }
    }

    // Produce 2 encoded bits (n0, n1) for every input bit (k).
    fn encode_bit(&mut self, bit: u32) -> (u8, u8) {
        self.register = (self.register << 1) | bit;
        let n0 = (self.register & self.g0).count_ones() & 1;
        let n1 = (self.register & self.g1).count_ones() & 1;
        (n0 as u8, n1 as u8)
    }
}

// Return the base-36 value (0-35) for a single character '0-9A-Z', 36 for
// spaces, or an error if any other characters are encountered.
fn encode_callsign_char(c: u8) -> Result<u32, Error> {
    let c = c as char;
    if c == ' ' {
        Ok(36)
    } else {
        match c.to_digit(36) {
            Some(d) => Ok(d),
            None => return Err(Error::InvalidCallsign),
        }
    }
}

fn encode_callsign(callsign: &str) -> Result<u32, Error> {
    let callsign = callsign.as_bytes();

    // Verify callsign is the appropriate length
    let length = callsign.len();
    if !(3..=6).contains(&length) {
        return Err(Error::InvalidCallsign);
    }

    // Pad the callsign with spaces to make it a total of 6 ASCII characters,
    // using the following scheme:
    //
    // "K1A"    => " K1A  "  (len 3)
    // "K1AB"   => " K1AB "  (len 4)
    // "KA1B"   => "KA1B  "  (len 4)
    // "K1ABC"  => " K1ABC"  (len 5)
    // "KA1BC"  => "KA1BC "  (len 5)
    // "KA1BCD" => "KA1BCD"  (len 6)

    // Determine the starting index of the first non-space character.
    let start = match length {
        0..=4 => {
            if (callsign[2] as char).is_digit(10) {
                0
            } else {
                1
            }
        }
        5 => {
            if (callsign[1] as char).is_digit(10) {
                1
            } else {
                0
            }
        }
        _ => 0,
    };

    // Fill an array with spaces and copying the callsign into the appropriate offset.
    let stop = start + length;
    let mut padded = [b' '; 6];
    padded[start..stop].copy_from_slice(&callsign);
    let callsign = padded;

    // Ensure the 3rd character in the padded callsign is a digit.
    if !(callsign[2] as char).is_digit(10) {
        return Err(Error::InvalidCallsign);
    }

    // The code below is an looping version of the algorithm described in
    // `The WSPR Coding Process`[1] by G4JNT.
    //
    // N0 = 0
    // N1 = N0 * 0  + callsign[0]
    // N2 = N1 * 36 + callsign[1]
    // N3 = N2 * 10 + callsign[2]
    // N4 = N3 * 27 + callsign[3] – 10
    // N5 = N4 * 27 + callsign[4] – 10
    // N6 = N5 * 27 + callsign[5] – 10

    let scalars = [0u32, 36, 10, 27, 27, 27];
    let subtracts = [0u32, 0, 0, 10, 10, 10];

    let mut n = 0;
    for (index, &c) in callsign.iter().enumerate() {
        n = n * scalars[index] + encode_callsign_char(c)? as u32 - subtracts[index];
    }

    Ok(n)
}

fn encode_grid_char(c: u8) -> Result<u16, Error> {
    let result = match (c as char).to_ascii_uppercase() {
        'A'..='R' => (c as u8) - b'A',
        '0'..='9' => (c as u8) - b'0',
        _ => return Err(Error::InvalidGrid),
    };

    Ok(result as u16)
}

fn encode_grid(grid: &str) -> Result<u16, Error> {
    let grid = grid.as_bytes();

    if grid.len() != 4 {
        return Err(Error::InvalidGrid);
    }

    let first = encode_grid_char(grid[0])?;
    let second = encode_grid_char(grid[1])?;
    let third = encode_grid_char(grid[2])?;
    let fourth = encode_grid_char(grid[3])?;

    let result = (179 - 10 * first - third) * 180 + 10 * second + fourth;
    Ok(result)
}

fn encode_power(power: u8) -> Result<u8, Error> {
    // Power is between 0 and 60 dBm, and can only end in 0, 3, or 7 (otherwise
    // it's invalid).
    //
    // Power in milliwatts can be calculated as 10 ^ (power / 10). For example:
    // 37dBm = 10 ^ 3.7 = 5011.87 mW.
    let rem = power % 10;

    if (0..=60).contains(&power) && (rem == 0 || rem == 3 || rem == 7) {
        Ok(power + 64)
    } else {
        Err(Error::InvalidPower)
    }
}

/// Encodes a callsign, a four character Maidenhead grid square, and a power level (in dBm) into 162
/// symbols each with a range of 0-3. These symbols may then be transmitting using 4 tone frequency
/// shift keying. Each tone is separated by 1.46Hz and is transmitted for 0.683s at a time, for a
/// total transmission time of 110.64s.
pub fn encode(callsign: &str, grid: &str, power: u8) -> Result<[u8; SYMBOLS], Error> {
    let mut buffer = BitVec::new();

    // Load the encoder with Layland-Lushbaugh polynomials
    let mut conv = ConvolutionalEncoder::new(0xF2D05351, 0xE4613C47);

    // Encode the callsign
    let callsign = encode_callsign(callsign)?;
    for i in (0..28).rev() {
        let bit = (callsign >> i) & 0x01;
        let n = conv.encode_bit(bit);
        buffer.push(n.0);
        buffer.push(n.1);
    }

    // Encode the grid
    let grid = encode_grid(grid)?;
    for i in (0..15).rev() {
        let bit = (grid as u32 >> i) & 0x01;
        let n = conv.encode_bit(bit);
        buffer.push(n.0);
        buffer.push(n.1);
    }

    // Encode the power
    let power = encode_power(power)?;
    for i in (0..7).rev() {
        let bit = (power as u32 >> i) & 0x01;
        let n = conv.encode_bit(bit);
        buffer.push(n.0);
        buffer.push(n.1);
    }

    // Drain the remaining bits by pumping 0's into the convolutional encoder
    for _ in (0..31).rev() {
        let bit = 0;
        let n = conv.encode_bit(bit);
        buffer.push(n.0);
        buffer.push(n.1);
    }

    // At the end of this process we've pumped 81 bits (28 + 15 + 7 + 31) through the encoder,
    // generating two bits for each, resulting in 162 bits.

    buffer.interleave();
    buffer.sync();
    Ok(buffer.inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_callsign() {
        assert_eq!(encode_callsign("  9   "), Ok(262374389));
        assert_eq!(encode_callsign("KA1BCD"), Ok(143706369));
    }

    #[test]
    fn test_encode_grid() {
        assert_eq!(encode_grid("AA00"), Ok(32220));
        assert_eq!(encode_grid("RR99"), Ok(179));

        assert_eq!(encode_grid("Z"), Err(Error::InvalidGrid));
        assert_eq!(encode_grid("ZZ"), Err(Error::InvalidGrid));
        assert_eq!(encode_grid("ZZ11"), Err(Error::InvalidGrid));
    }

    #[test]
    fn test_encode_power() {
        // Too much power!
        assert_eq!(encode_power(61), Err(Error::InvalidPower));

        for power in 0..=60 {
            let rem = power % 10;
            let result = encode_power(power);
            if rem == 0 || rem == 3 || rem == 7 {
                assert_eq!(result, Ok(power + 64));
            } else {
                // doesn't end in 0, 3 or 7
                assert_eq!(result, Err(Error::InvalidPower));
            }
        }
    }

    #[test]
    fn test_encode_wspr() {
        assert_eq!(
            encode("K1A", "FN34", 33),
            Ok([
                3, 3, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1, 1, 0, 2, 0, 3, 0, 0, 3, 0, 1, 1, 3, 3, 0,
                0, 0, 0, 2, 0, 2, 1, 0, 0, 3, 2, 1, 2, 0, 2, 0, 0, 0, 3, 0, 1, 3, 0, 2, 3, 1, 2, 3,
                0, 2, 0, 3, 3, 0, 1, 2, 2, 0, 0, 1, 3, 2, 1, 0, 3, 2, 3, 2, 1, 0, 0, 3, 2, 2, 1, 2,
                1, 1, 0, 0, 0, 1, 1, 0, 3, 2, 3, 2, 2, 2, 3, 0, 2, 2, 0, 0, 1, 2, 2, 1, 2, 0, 1, 3,
                1, 2, 3, 3, 0, 0, 1, 1, 2, 3, 2, 2, 0, 3, 1, 3, 2, 2, 0, 2, 0, 3, 0, 3, 2, 0, 1, 1,
                2, 2, 0, 0, 2, 2, 2, 1, 3, 2, 3, 2, 3, 1, 2, 0, 0, 3, 1, 2, 2, 2
            ])
        );

        assert_eq!(
            encode("N6AB", "CM87", 0),
            Ok([
                3, 1, 0, 0, 2, 2, 0, 2, 1, 0, 2, 0, 1, 3, 3, 0, 2, 0, 3, 0, 0, 1, 2, 1, 3, 1, 1, 2,
                0, 2, 0, 0, 0, 2, 3, 2, 0, 1, 0, 3, 2, 0, 0, 0, 0, 2, 1, 0, 1, 3, 2, 0, 3, 1, 2, 1,
                2, 0, 2, 3, 3, 0, 3, 0, 2, 2, 2, 3, 3, 0, 3, 2, 3, 2, 3, 2, 3, 0, 2, 1, 2, 2, 1, 0,
                1, 3, 2, 2, 0, 1, 1, 0, 1, 2, 1, 0, 2, 2, 1, 0, 0, 2, 2, 0, 1, 0, 2, 3, 0, 2, 1, 1,
                1, 0, 3, 3, 2, 0, 3, 1, 0, 3, 2, 0, 0, 3, 3, 1, 2, 2, 2, 2, 2, 3, 0, 1, 2, 0, 1, 1,
                0, 2, 2, 0, 2, 2, 2, 3, 3, 2, 1, 2, 1, 3, 0, 0, 0, 3, 1, 0, 2, 2
            ])
        );

        assert_eq!(
            encode("G1ABC", "IO83", 37),
            Ok([
                3, 3, 0, 0, 0, 2, 0, 0, 1, 0, 2, 0, 1, 1, 3, 2, 2, 2, 3, 2, 2, 1, 0, 1, 1, 3, 1, 2,
                2, 2, 0, 0, 0, 0, 3, 0, 0, 1, 0, 3, 0, 2, 2, 2, 0, 2, 3, 2, 1, 3, 2, 2, 3, 3, 0, 1,
                0, 0, 0, 1, 3, 2, 3, 2, 2, 2, 0, 1, 1, 2, 3, 0, 3, 0, 1, 0, 3, 0, 0, 1, 2, 2, 3, 2,
                3, 3, 0, 0, 2, 3, 1, 2, 1, 0, 1, 2, 2, 2, 1, 0, 2, 0, 2, 2, 3, 2, 0, 1, 0, 0, 3, 1,
                1, 2, 3, 3, 2, 2, 1, 1, 2, 1, 2, 0, 0, 1, 3, 3, 2, 0, 0, 2, 2, 1, 2, 3, 2, 0, 1, 1,
                2, 2, 2, 2, 2, 0, 2, 3, 3, 2, 1, 2, 1, 3, 0, 2, 2, 3, 3, 2, 2, 0
            ])
        );

        assert_eq!(
            encode("KA1BCD", "AA00", 33),
            Ok([
                3, 3, 2, 2, 0, 2, 0, 2, 3, 2, 0, 2, 1, 1, 1, 0, 0, 2, 1, 0, 2, 3, 2, 1, 1, 1, 1, 0,
                0, 2, 0, 2, 2, 0, 3, 2, 2, 3, 2, 3, 2, 2, 2, 0, 2, 0, 3, 0, 3, 1, 0, 2, 3, 1, 0, 3,
                2, 2, 0, 1, 3, 2, 1, 2, 0, 2, 0, 3, 3, 0, 3, 2, 1, 2, 1, 0, 3, 0, 2, 3, 0, 0, 3, 0,
                3, 3, 2, 0, 2, 1, 1, 0, 3, 0, 3, 2, 2, 0, 3, 2, 0, 0, 2, 0, 3, 2, 0, 1, 2, 2, 1, 3,
                1, 2, 1, 3, 2, 0, 1, 1, 2, 3, 0, 0, 2, 1, 3, 3, 2, 0, 2, 2, 2, 3, 0, 1, 2, 2, 1, 1,
                0, 2, 0, 0, 0, 0, 2, 3, 1, 2, 1, 2, 3, 3, 2, 2, 2, 3, 1, 2, 0, 2
            ])
        );
    }
}
