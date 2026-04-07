use clap::Parser;
use rustavm::decoder::Decoder;
use rustavm::ivf::IvfReader;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};

#[derive(Parser)]
#[command(name = "avmdec", about = "AVM/AV2 video decoder — decodes IVF files to Y4M or raw YUV")]
struct Cli {
    /// Input IVF file
    input: String,

    /// Output file (Y4M or raw YUV)
    output: Option<String>,

    /// Compute MD5 checksum of decoded output
    #[arg(long)]
    md5: bool,

    /// Output raw video without Y4M container headers
    #[arg(long)]
    rawvideo: bool,
}

fn write_frame(
    img: &rustavm::decoder::Frame,
    out: &mut Option<BufWriter<File>>,
    md5_ctx: &mut md5::Context,
    compute_md5: bool,
    raw_video: bool,
) -> io::Result<()> {
    if !raw_video {
        let frame_header = "FRAME\n";
        if compute_md5 {
            md5_ctx.consume(frame_header.as_bytes());
        }
        if let Some(ref mut f) = out {
            f.write_all(frame_header.as_bytes())?;
        }
    }

    let bytes_per_sample = if img.bit_depth() > 8 || (img.format() & rustavm::AVM_IMG_FMT_HIGHBITDEPTH) != 0 { 2 } else { 1 };
    for i in 0..3 {
        if let Some(plane) = img.plane(i) {
            let stride = img.stride(i);
            let w = img.plane_width(i);
            let h = img.height_for_plane(i);

            if bytes_per_sample == 2 && img.bit_depth() == 8 {
                let mut row_buf = vec![0u8; w];
                for row_data in plane.chunks(stride).take(h) {
                    for (x, byte) in row_buf.iter_mut().enumerate().take(w) {
                        *byte = match row_data.get(x * 2) {
                            Some(&b) => b,
                            None => {
                                eprintln!("Warning: truncated plane data at row");
                                0
                            }
                        };
                    }
                    if compute_md5 {
                        md5_ctx.consume(&row_buf);
                    }
                    if let Some(ref mut f) = out {
                        f.write_all(&row_buf)?;
                    }
                }
            } else {
                let row_bytes = w * bytes_per_sample;
                for row_data in plane.chunks(stride).take(h) {
                    let row_slice = match row_data.get(..row_bytes) {
                        Some(s) => s,
                        None => {
                            eprintln!("Warning: truncated plane data at row");
                            break;
                        }
                    };
                    if compute_md5 {
                        md5_ctx.consume(row_slice);
                    }
                    if let Some(ref mut f) = out {
                        f.write_all(row_slice)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let input_path = &cli.input;
    let input_file = File::open(input_path)?;
    let mut ivf_reader = IvfReader::new(BufReader::new(input_file))?;

    println!("Input: {input_path}");
    println!("IVF Header: {}x{} @ {}/{} fps, FourCC: {:?}",
        ivf_reader.header().width, ivf_reader.header().height,
        ivf_reader.header().framerate_num, ivf_reader.header().framerate_den,
        ivf_reader.header().fourcc);

    let mut decoder = Decoder::new()?;
    let mut out_file: Option<BufWriter<File>> = if let Some(ref path) = cli.output {
        Some(BufWriter::with_capacity(1 << 20, File::create(path)?))
    } else {
        None
    };
    let compute_md5 = cli.md5;
    let raw_video = cli.rawvideo;

    let mut frame_count = 0;
    let mut md5_context = md5::Context::new();

    while let Some(frame) = ivf_reader.next_frame()? {
        if let Err(e) = decoder.decode(&frame.data) {
            eprintln!("\nDecode error at frame {frame_count}: {e}");
            break;
        }

        for img in decoder.get_frames() {
            if frame_count == 0 {
                println!("Frame 0: Format: {:x}, Bit Depth: {}, WxH: {}x{}", 
                    img.format(), img.bit_depth(), img.width(), img.height());

                if !raw_video {
                    // Generate Y4M file header matching C's logic
                    let colorspace: Cow<'static, str> = if img.monochrome() {
                        Cow::Borrowed(match img.bit_depth() {
                            8 => "Cmono",
                            9 => "Cmono9",
                            10 => "Cmono10",
                            12 => "Cmono12",
                            16 => "Cmono16",
                            _ => "Cmono",
                        })
                    } else if img.bit_depth() == 8 {
                        Cow::Borrowed(match img.format() {
                            rustavm::avm_img_fmt_AVM_IMG_FMT_I444 => "C444",
                            rustavm::avm_img_fmt_AVM_IMG_FMT_I422 => "C422",
                            _ => {
                                if img.chroma_sample_position() == rustavm::avm_chroma_sample_position_AVM_CSP_LEFT {
                                    "C420mpeg2 XYSCSS=420MPEG2"
                                } else if img.chroma_sample_position() == rustavm::avm_chroma_sample_position_AVM_CSP_TOPLEFT {
                                    "C420"
                                } else {
                                    "C420jpeg"
                                }
                            }
                        })
                    } else {
                        let template = match img.format() {
                            rustavm::avm_img_fmt_AVM_IMG_FMT_I44416 => "C444p{} XYSCSS=444P{}",
                            rustavm::avm_img_fmt_AVM_IMG_FMT_I42216 => "C422p{} XYSCSS=422P{}",
                            _ => "C420p{} XYSCSS=420P{}",
                        };
                        Cow::Owned(template.replace("{}", &img.bit_depth().to_string()))
                    };

                    let range = if img.color_range() == rustavm::avm_color_range_AVM_CR_FULL_RANGE {
                        " XCOLORRANGE=FULL"
                    } else {
                        ""
                    };

                    let header = format!("YUV4MPEG2 W{} H{} F{}:{} Ip {}{}\n", 
                        img.width(), img.height(), 
                        ivf_reader.header().framerate_num, ivf_reader.header().framerate_den,
                        colorspace, range);

                    if compute_md5 {
                        md5_context.consume(header.as_bytes());
                    }
                    if let Some(ref mut f) = out_file {
                        f.write_all(header.as_bytes())?;
                    }
                }
            }

            write_frame(&img, &mut out_file, &mut md5_context, compute_md5, raw_video)?;
            frame_count += 1;
            if frame_count % 10 == 0 {
                print!("\rDecoded {frame_count} frames");
                io::stdout().flush()?;
            }
        }
    }

    // Flush the decoder to retrieve any buffered frames (B-frame reordering).
    if let Err(e) = decoder.flush() {
        eprintln!("Flush error: {e}");
    }
    for img in decoder.get_frames() {
        write_frame(&img, &mut out_file, &mut md5_context, compute_md5, raw_video)?;
        frame_count += 1;
    }

    println!("\rDecoded {frame_count} frames. Done.");

    if compute_md5 {
        let digest = md5_context.compute();
        println!("MD5: {digest:x}");
    }
    Ok(())
}
