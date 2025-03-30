// References: [1] http://g4jnt.com/WSPR_Coding_Process.pdf

#![no_std]

#[derive(Debug, PartialEq)]
pub enum Error {
    InvalidPower,
    InvalidGrid,
    InvalidCallsign,
}

// A 32-bit shift register that shifts bits into the least significant bit,
// performs a bitwise AND with a constant, counts the number of set bits
// returning a 0 if even, 1 if odd.
struct ShiftRegister {
    value: u32,
    and_const: u32,
}

impl ShiftRegister {
    fn new(and_const: u32) -> Self {
        Self {
            value: 0,
            and_const,
        }
    }

    fn shift(&mut self, bit: u32) -> u8 {
        self.value = (self.value << 1) | bit;
        let ones = (self.value & self.and_const).count_ones();
        let parity = ones & 0x01; // is parity odd
        parity as u8
    }
}

// Typically we'd just use a Vec here, but this crate is designed for [no_std]
// use, we'll implement this by hand.
struct Buffer {
    buffer: [u8; 162],
    index: usize,
}

impl Buffer {
    fn new() -> Self {
        Self {
            buffer: [0u8; 162],
            index: 0,
        }
    }

    fn push(&mut self, bit: u8) {
        self.buffer[self.index] = bit;
        self.index += 1;
    }

    fn interleave(&mut self) {
        let mut interleaved = [0u8; 162];
        let mut p = 0;
        for i in 0u8..255 {
            let j = i.reverse_bits() as usize;
            if j < 162 {
                interleaved[j] = self.buffer[p];
                p += 1;
                if p == 162 {
                    break;
                }
            }
        }

        self.buffer = interleaved;
    }

    fn sync(&mut self) {
        const SYNC: [u8; 162] = [
            1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1,
            0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0,
            0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0,
            0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0,
            0, 1, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0,
            1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0,
            0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1,
            0, 0, 0, 1, 1, 0, 0, 0,
        ];

        for i in 0..162 {
            self.buffer[i] = SYNC[i] + 2 * self.buffer[i];
        }
    }

    fn release(self) -> [u8; 162] {
        self.buffer
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

    // Fill an array with spaces and copying the callsign into the appropriate
    // offset.
    let stop = start + length;
    let mut padded = [b' '; 6];
    padded[start..stop].copy_from_slice(&callsign);
    let callsign = padded;

    // Ensure the 3rd character in the padded callsign is a digit.
    if !(callsign[2] as char).is_digit(10) {
        return Err(Error::InvalidCallsign);
    }

    // The code below is an looping version of the algorithm described
    // in `The WSPR Coding Process`[1] by G4JNT.
    //
    // N0 = 0
    // N1 = N0 * 0  + [Ch 1]
    // N2 = N1 * 36 + [Ch 2]
    // N3 = N2 * 10 + [Ch 3]
    // N4 = N3 * 27 + [Ch 4] – 10
    // N5 = N4 * 27 + [Ch 5] – 10
    // N6 = N5 * 27 + [Ch 6] – 10

    let scalars = [0u32, 36, 10, 27, 27, 27];
    let subtracts = [0u32, 0, 0, 10, 10, 10];

    let mut n = 0;
    for (index, &c) in callsign.iter().enumerate() {
        n = n * scalars[index] + encode_callsign_char(c)? as u32
            - subtracts[index];
    }

    Ok(n)
}

fn encode_grid_char(c: u8) -> Result<u8, Error> {
    return match (c as char).to_ascii_uppercase() {
        'A'..='R' => Ok((c as u8) - b'A'),
        '0'..='9' => Ok((c as u8) - b'0'),
        _ => Err(Error::InvalidGrid),
    };
}

fn encode_grid(grid: &str) -> Result<u16, Error> {
    let grid = grid.as_bytes();

    if grid.len() != 4 {
        return Err(Error::InvalidGrid);
    }

    let first = encode_grid_char(grid[0])? as u16;
    let second = encode_grid_char(grid[1])? as u16;
    let third = encode_grid_char(grid[2])? as u16;
    let fourth = encode_grid_char(grid[3])? as u16;

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

/// Encodes a callsign, a four character Maidenhead grid square, and a power
/// level (in dBm) into 162 symbols each with a range of 0-3. These symbols
/// may then be transmitting using 4 tone frequency shift keying. Each tone
/// is separated by 1.46Hz and is transmitted for 0.683s at a time, for a total
/// transmission time of 110.64s.
pub fn encode(
    callsign: &str,
    grid: &str,
    power: u8,
) -> Result<[u8; 162], Error> {
    let callsign = encode_callsign(callsign)?;
    let grid = encode_grid(grid)?;
    let power = encode_power(power)?;

    let mut reg0 = ShiftRegister::new(0xF2D05351);
    let mut reg1 = ShiftRegister::new(0xE4613C47);

    let mut buffer = Buffer::new();

    for i in (0..28).rev() {
        let bit = (callsign >> i) & 0x01;
        buffer.push(reg0.shift(bit));
        buffer.push(reg1.shift(bit));
    }

    for i in (0..15).rev() {
        let bit = (grid as u32 >> i) & 0x01;
        buffer.push(reg0.shift(bit));
        buffer.push(reg1.shift(bit));
    }

    for i in (0..7).rev() {
        let bit = (power as u32 >> i) & 0x01;
        buffer.push(reg0.shift(bit));
        buffer.push(reg1.shift(bit));
    }

    for _ in (0..31).rev() {
        let bit = 0;
        buffer.push(reg0.shift(bit));
        buffer.push(reg1.shift(bit));
    }

    buffer.interleave();
    buffer.sync();
    Ok(buffer.release())
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
                3, 3, 0, 0, 2, 2, 0, 0, 1, 2, 0, 0, 1, 1, 1, 0, 2, 0, 3, 0, 0,
                3, 0, 1, 1, 3, 3, 0, 0, 0, 0, 2, 0, 2, 1, 0, 0, 3, 2, 1, 2, 0,
                2, 0, 0, 0, 3, 0, 1, 3, 0, 2, 3, 1, 2, 3, 0, 2, 0, 3, 3, 0, 1,
                2, 2, 0, 0, 1, 3, 2, 1, 0, 3, 2, 3, 2, 1, 0, 0, 3, 2, 2, 1, 2,
                1, 1, 0, 0, 0, 1, 1, 0, 3, 2, 3, 2, 2, 2, 3, 0, 2, 2, 0, 0, 1,
                2, 2, 1, 2, 0, 1, 3, 1, 2, 3, 3, 0, 0, 1, 1, 2, 3, 2, 2, 0, 3,
                1, 3, 2, 2, 0, 2, 0, 3, 0, 3, 2, 0, 1, 1, 2, 2, 0, 0, 2, 2, 2,
                1, 3, 2, 3, 2, 3, 1, 2, 0, 0, 3, 1, 2, 2, 2
            ])
        );

        assert_eq!(
            encode("N6AB", "CM87", 0),
            Ok([
                3, 1, 0, 0, 2, 2, 0, 2, 1, 0, 2, 0, 1, 3, 3, 0, 2, 0, 3, 0, 0,
                1, 2, 1, 3, 1, 1, 2, 0, 2, 0, 0, 0, 2, 3, 2, 0, 1, 0, 3, 2, 0,
                0, 0, 0, 2, 1, 0, 1, 3, 2, 0, 3, 1, 2, 1, 2, 0, 2, 3, 3, 0, 3,
                0, 2, 2, 2, 3, 3, 0, 3, 2, 3, 2, 3, 2, 3, 0, 2, 1, 2, 2, 1, 0,
                1, 3, 2, 2, 0, 1, 1, 0, 1, 2, 1, 0, 2, 2, 1, 0, 0, 2, 2, 0, 1,
                0, 2, 3, 0, 2, 1, 1, 1, 0, 3, 3, 2, 0, 3, 1, 0, 3, 2, 0, 0, 3,
                3, 1, 2, 2, 2, 2, 2, 3, 0, 1, 2, 0, 1, 1, 0, 2, 2, 0, 2, 2, 2,
                3, 3, 2, 1, 2, 1, 3, 0, 0, 0, 3, 1, 0, 2, 2
            ])
        );

        assert_eq!(
            encode("G1ABC", "IO83", 37),
            Ok([
                3, 3, 0, 0, 0, 2, 0, 0, 1, 0, 2, 0, 1, 1, 3, 2, 2, 2, 3, 2, 2,
                1, 0, 1, 1, 3, 1, 2, 2, 2, 0, 0, 0, 0, 3, 0, 0, 1, 0, 3, 0, 2,
                2, 2, 0, 2, 3, 2, 1, 3, 2, 2, 3, 3, 0, 1, 0, 0, 0, 1, 3, 2, 3,
                2, 2, 2, 0, 1, 1, 2, 3, 0, 3, 0, 1, 0, 3, 0, 0, 1, 2, 2, 3, 2,
                3, 3, 0, 0, 2, 3, 1, 2, 1, 0, 1, 2, 2, 2, 1, 0, 2, 0, 2, 2, 3,
                2, 0, 1, 0, 0, 3, 1, 1, 2, 3, 3, 2, 2, 1, 1, 2, 1, 2, 0, 0, 1,
                3, 3, 2, 0, 0, 2, 2, 1, 2, 3, 2, 0, 1, 1, 2, 2, 2, 2, 2, 0, 2,
                3, 3, 2, 1, 2, 1, 3, 0, 2, 2, 3, 3, 2, 2, 0
            ])
        );

        assert_eq!(
            encode("KA1BCD", "AA00", 33),
            Ok([
                3, 3, 2, 2, 0, 2, 0, 2, 3, 2, 0, 2, 1, 1, 1, 0, 0, 2, 1, 0, 2,
                3, 2, 1, 1, 1, 1, 0, 0, 2, 0, 2, 2, 0, 3, 2, 2, 3, 2, 3, 2, 2,
                2, 0, 2, 0, 3, 0, 3, 1, 0, 2, 3, 1, 0, 3, 2, 2, 0, 1, 3, 2, 1,
                2, 0, 2, 0, 3, 3, 0, 3, 2, 1, 2, 1, 0, 3, 0, 2, 3, 0, 0, 3, 0,
                3, 3, 2, 0, 2, 1, 1, 0, 3, 0, 3, 2, 2, 0, 3, 2, 0, 0, 2, 0, 3,
                2, 0, 1, 2, 2, 1, 3, 1, 2, 1, 3, 2, 0, 1, 1, 2, 3, 0, 0, 2, 1,
                3, 3, 2, 0, 2, 2, 2, 3, 0, 1, 2, 2, 1, 1, 0, 2, 0, 0, 0, 0, 2,
                3, 1, 2, 1, 2, 3, 3, 2, 2, 2, 3, 1, 2, 0, 2
            ])
        );
    }
}
