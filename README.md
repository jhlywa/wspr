### WSPR

`wspr` is a Rust crate for encoding a callsign, a four character Maidenhead
grid square, and a power level (in dBm) into the 162 symbols needed for a WSPR
transmission. Each resulting symbol is in the range 0-3 and may be transmitted
using 4 tone FSK.

Each tone is separated by 1.464Hz and is 683ms in length.

Only Type 1 WSPR messages are supported.

### no_std

The `wspr` crate is `no_std` by default.

### Optional Features

The `wspr` crate provides the following optional Cargo features:

- `defmt`: Implements `defmt::Format` for `wspr::Error`

### Example

```rust

if let Ok(symbols) = wspr::encode("KA1BCD", "FM17", 37) {
    // 20m WSPR dial frequency in KHz
    let dial = 14095.6;

    // WSPR transmit frequencies are 1.5KHz above the dial frequency
    let offset = 1.5;

    // Tone spacing in KHz
    let spacing = 0.001464;

    for symbol in symbols.iter() {
        let frequency = dial + offset + (spacing * symbol as f32);
        // A notional WSPR transmission
        // set_frequency(frequency);
        // key_down()
        // sleep_ms(683);
    }
    // key_up();
}
```
