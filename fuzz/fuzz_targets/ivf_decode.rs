#![no_main]

use libfuzzer_sys::fuzz_target;
use rustavm::decoder::Decoder;
use rustavm::ivf::IvfReader;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Fuzz the IVF parser: exercise header validation and frame size cap (R1, R2).
    let cursor = Cursor::new(data);
    let mut reader = match IvfReader::new(cursor) {
        Ok(r) => r,
        Err(_) => return, // malformed header — expected
    };

    // Fuzz the decoder: feed parsed frames into the AVM decoder.
    // NOTE: The C codec may abort() on severely malformed OBU sequences.
    // Run with ASAN/UBSAN to catch memory errors in libavm itself.
    let mut decoder = match Decoder::new() {
        Ok(d) => d,
        Err(_) => return,
    };

    while let Ok(Some(frame)) = reader.next_frame() {
        if decoder.decode(&frame.data).is_err() {
            break;
        }
        // Drain decoded frames to exercise Frame/plane accessors.
        for img in decoder.get_frames() {
            let _ = img.width();
            let _ = img.height();
            let _ = img.bit_depth();
            let _ = img.format();
            let _ = img.bytes_per_sample();
            for i in 0..3 {
                let _ = img.plane(i);
                let _ = img.plane_view(i);
                if let Some(rows) = img.rows(i) {
                    for row in rows {
                        std::hint::black_box(row);
                    }
                }
            }
        }
    }

    // Flush to exercise B-frame drain path.
    let _ = decoder.flush();
    for img in decoder.get_frames() {
        let _ = img.width();
        for i in 0..3 {
            let _ = img.plane(i);
            if let Some(rows) = img.rows(i) {
                for row in rows {
                    std::hint::black_box(row);
                }
            }
        }
    }
});
