use rustavm::decoder::Decoder;
use rustavm::ivf::IvfReader;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Note: This example expects test.ivf to be in avm/out/
    let input_path = "../avm/out/test.ivf";
    let input_file = match File::open(input_path) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("Error: Could not open {input_path}. Please run avmenc to generate it first.");
            return Ok(());
        }
    };
    
    let mut ivf_reader = IvfReader::new(BufReader::new(input_file))?;
    let mut decoder = Decoder::new()?;

    println!("Decoding {input_path}...");
    let mut frame_count = 0;
    while let Some(frame) = ivf_reader.next_frame()? {
        decoder.decode(&frame.data)?;
        for img in decoder.get_frames() {
            frame_count += 1;
            println!("Frame {}: {}x{}, format: {:x}", frame_count, img.width(), img.height(), img.format());
        }
    }
    println!("Total frames decoded: {frame_count}");
    Ok(())
}
