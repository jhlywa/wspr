# WSPR

WSPR is a Rust library for encoding a callsign, a four character Maidenhead
grid square, and a power level (in dBm) into the 162 symbols needed for a WSPR
transmission. Each resulting symbol in the range 0-3 and may be transmitted
uses 4 tone FSK.

Each tone is separated by 1.464Hz and is 683ms in length.

Only Type 1 WSPR messages are supported.

# Example

```rust

if let Ok(symbols) = wspr::encode("KA1BCD", "FM17", 37) {
    // 20m WSPR dial frequency in KHz
    let dial = 14095.6;

    // WSPR transmit frequencies are 1.5KHz above the dial frequency
    let offset = 1.5;

    for symbol in symbols.iter() {
        let frequency = dial + offset + (0.001464 * symbol);
        // A notional WSPR transmission
        // set_frequency(frequency);
        // enable_tx();
        // sleep_ms(683);
    }
    // disable_tx();
}
```
