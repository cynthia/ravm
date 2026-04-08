//! High-level streaming helpers.
//!
//! [`decode_ivf`] wraps the common pattern of reading an IVF file, feeding
//! every packet to a [`Decoder`], draining each batch of decoded frames,
//! and finally flushing for B-frame reorder buffers.

use crate::decoder::{Decoder, DecoderError, Frame};
use crate::ivf::IvfReader;
use std::io::Read;

/// Error returned by [`decode_ivf`] — either an I/O error from the IVF
/// reader or a [`DecoderError`] from libavm.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// The IVF reader (or its underlying `Read`) reported an error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// libavm rejected the bitstream.
    #[error("decoder error: {0}")]
    Decoder(#[from] DecoderError),
}

/// Decode every frame from an IVF stream, invoking `sink` once per
/// decoded frame.
///
/// Handles packet iteration, post-decode draining, and the final
/// `flush` + drain that retrieves any frames buffered for B-frame
/// reordering.  Returns the total number of decoded frames.
///
/// Use [`decode_ivf_reader`] from a raw `Read` source, or open the IVF
/// header explicitly via [`IvfReader::open`] / [`IvfReader::new`] first
/// if you need to inspect [`IvfReader::header`] before decoding.
///
/// # Example
///
/// ```no_run
/// use rustavm::ivf::IvfReader;
/// use rustavm::stream::decode_ivf;
///
/// let reader = IvfReader::open("clip.ivf")?;
/// println!("{}x{}", reader.header.width, reader.header.height);
/// let count = decode_ivf(reader, |frame| {
///     println!("{}x{}", frame.width(), frame.height());
/// })?;
/// println!("decoded {count} frames");
/// # Ok::<(), rustavm::stream::StreamError>(())
/// ```
pub fn decode_ivf<R, F>(mut ivf: IvfReader<R>, mut sink: F) -> Result<usize, StreamError>
where
    R: Read,
    F: FnMut(Frame<'_>),
{
    let mut decoder = Decoder::new()?;
    let mut count = 0usize;

    while let Some(packet) = ivf.next_frame()? {
        decoder.decode(&packet.data)?;
        for frame in decoder.get_frames() {
            sink(frame);
            count += 1;
        }
    }

    decoder.flush()?;
    for frame in decoder.get_frames() {
        sink(frame);
        count += 1;
    }

    Ok(count)
}

/// Convenience: open an IVF reader from any `Read` source and call
/// [`decode_ivf`] on it.
///
/// Equivalent to `decode_ivf(IvfReader::new(reader)?, sink)`.
pub fn decode_ivf_reader<R, F>(reader: R, sink: F) -> Result<usize, StreamError>
where
    R: Read,
    F: FnMut(Frame<'_>),
{
    let ivf = IvfReader::new(reader)?;
    decode_ivf(ivf, sink)
}
