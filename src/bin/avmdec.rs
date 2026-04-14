use clap::Parser;
use rustavm::BackendKind;
use rustavm::{compare_ivf_file, compare_ivf_file_outcomes};
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

    /// Number of decoder worker threads (0 = codec default).
    #[arg(long, value_name = "N")]
    threads: Option<u32>,

    /// Decoder backend to use.
    #[arg(long, value_enum, default_value_t = BackendKind::Libavm)]
    backend: BackendKind,

    /// Optional second backend to compare against for exact output equality.
    #[arg(long, value_enum)]
    compare_backend: Option<BackendKind>,

    /// Compare backend outcomes, including parser progress and terminal error.
    ///
    /// This is useful while the Rust backend is still parser-only and may stop
    /// before producing decoded frames.
    #[arg(long)]
    compare_outcomes: bool,
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

    let downshift_8bit = img.bytes_per_sample() == 2 && img.bit_depth() == 8;
    for i in 0..3 {
        let Some(rows) = img.rows(i) else { continue };
        let w = img.plane_width(i);
        let mut row_buf = if downshift_8bit { vec![0u8; w] } else { Vec::new() };

        for row in rows {
            let bytes: &[u8] = if downshift_8bit {
                // 16-bit container holding 8-bit content: take the low byte
                // of each sample so the output is one byte per pixel.
                for (x, dst) in row_buf.iter_mut().enumerate() {
                    *dst = row.get(x * 2).copied().unwrap_or(0);
                }
                &row_buf
            } else {
                row
            };
            if compute_md5 {
                md5_ctx.consume(bytes);
            }
            if let Some(ref mut f) = out {
                f.write_all(bytes)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(compare_backend) = cli.compare_backend {
        if cli.compare_outcomes {
            let (left, right) =
                compare_ivf_file_outcomes(&cli.input, cli.backend, compare_backend, cli.threads)?;
            println!(
                "Backend outcomes matched: {} vs {} (left frames: {}, right frames: {}, left stop: {:?}, right stop: {:?})",
                left.snapshot.backend,
                right.snapshot.backend,
                left.snapshot.frames.len(),
                right.snapshot.frames.len(),
                left.stopped_at_packet,
                right.stopped_at_packet
            );
        } else {
            let (left, right) =
                compare_ivf_file(&cli.input, cli.backend, compare_backend, cli.threads)?;
            println!(
                "Backends matched exactly: {} vs {} ({} frames)",
                left.backend,
                right.backend,
                left.frames.len()
            );
        }
        return Ok(());
    }

    let input_path = &cli.input;
    let input_file = File::open(input_path)?;
    let mut ivf_reader = IvfReader::new(BufReader::new(input_file))?;

    println!("Input: {input_path}");
    println!("IVF Header: {}x{} @ {}/{} fps, FourCC: {:?}",
        ivf_reader.header.width, ivf_reader.header.height,
        ivf_reader.header.framerate_num, ivf_reader.header.framerate_den,
        ivf_reader.header.fourcc);

    let mut builder = Decoder::builder().backend(cli.backend);
    if let Some(t) = cli.threads {
        builder = builder.threads(t);
    }
    let mut decoder = builder.build()?;
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
                    img.format_raw(), img.bit_depth(), img.width(), img.height());

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
                        Cow::Borrowed(match img.format_raw() {
                            rustavm::sys::avm_img_fmt_AVM_IMG_FMT_I444 => "C444",
                            rustavm::sys::avm_img_fmt_AVM_IMG_FMT_I422 => "C422",
                            _ => {
                                use rustavm::format::ChromaSamplePosition;
                                match img.chroma_sample_position() {
                                    ChromaSamplePosition::Left => "C420mpeg2 XYSCSS=420MPEG2",
                                    ChromaSamplePosition::TopLeft => "C420",
                                    _ => "C420jpeg",
                                }
                            }
                        })
                    } else {
                        let template = match img.format_raw() {
                            rustavm::sys::avm_img_fmt_AVM_IMG_FMT_I44416 => "C444p{} XYSCSS=444P{}",
                            rustavm::sys::avm_img_fmt_AVM_IMG_FMT_I42216 => "C422p{} XYSCSS=422P{}",
                            _ => "C420p{} XYSCSS=420P{}",
                        };
                        Cow::Owned(template.replace("{}", &img.bit_depth().to_string()))
                    };

                    let range = if img.color_range() == rustavm::format::ColorRange::Full {
                        " XCOLORRANGE=FULL"
                    } else {
                        ""
                    };

                    let header = format!("YUV4MPEG2 W{} H{} F{}:{} Ip {}{}\n", 
                        img.width(), img.height(), 
                        ivf_reader.header.framerate_num, ivf_reader.header.framerate_den,
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
