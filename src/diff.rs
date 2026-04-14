//! Differential decoding helpers.
//!
//! This module provides the normalized frame/output representation needed to
//! compare multiple decoder backends for exact equality.

use crate::backend::BackendKind;
use crate::decoder::{DecodeProgress, Decoder, DecoderError, OwnedFrame, StreamInfo};
use crate::ivf::IvfReader;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;

/// A fully materialized decode result, normalized for backend comparison.
#[derive(Debug, Clone)]
pub struct DecodeSnapshot {
    /// Backend that produced this snapshot.
    pub backend: BackendKind,
    /// Stream information reported by the decoder, if any packet was parsed.
    pub stream_info: Option<StreamInfo>,
    /// Fully owned decoded frames in display order.
    pub frames: Vec<OwnedFrame>,
    /// Final parser/decode progress reported by the backend.
    pub progress: DecodeProgress,
}

/// A decode attempt captured for backend comparison, including early-stop failures.
#[derive(Debug, Clone)]
pub struct DecodeOutcome {
    /// Normalized snapshot materialized before decode completed or failed.
    pub snapshot: DecodeSnapshot,
    /// Packet index where decoding stopped, if a terminal error occurred.
    pub stopped_at_packet: Option<usize>,
    /// String form of the terminal decoder error, if any.
    pub terminal_error: Option<String>,
}

/// High-level mismatch between two decode snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeMismatch {
    /// One backend reported stream info and the other did not, or metadata differed.
    StreamInfo {
        left_backend: BackendKind,
        right_backend: BackendKind,
        left: Option<StreamInfo>,
        right: Option<StreamInfo>,
    },
    /// Backends emitted a different number of frames.
    FrameCount {
        left_backend: BackendKind,
        right_backend: BackendKind,
        left: usize,
        right: usize,
    },
    /// A specific frame differed.
    Frame {
        frame_index: usize,
        mismatch: FrameMismatch,
    },
    /// A specific parser/decode progress field differed between backends.
    ProgressField {
        left_backend: BackendKind,
        right_backend: BackendKind,
        field: &'static str,
        left: String,
        right: String,
    },
    /// Backends stopped at different packets, exposed different terminal errors,
    /// or exposed different terminal decoder errors.
    TerminalState {
        left_backend: BackendKind,
        right_backend: BackendKind,
        left_packet: Option<usize>,
        right_packet: Option<usize>,
        left_error: Option<String>,
        right_error: Option<String>,
    },
}

/// Detailed mismatch for one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameMismatch {
    /// Metadata differed before comparing plane bytes.
    Metadata {
        field: &'static str,
        left: String,
        right: String,
    },
    /// Plane presence/length differed.
    PlaneLayout {
        plane: usize,
        left_len: Option<usize>,
        right_len: Option<usize>,
        left_stride: usize,
        right_stride: usize,
    },
    /// Plane bytes differed.
    PlaneByte {
        plane: usize,
        byte_index: usize,
        left: u8,
        right: u8,
    },
}

impl fmt::Display for DecodeMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamInfo {
                left_backend,
                right_backend,
                left,
                right,
            } => write!(
                f,
                "stream info mismatch between {left_backend} and {right_backend}: left={left:?} right={right:?}"
            ),
            Self::FrameCount {
                left_backend,
                right_backend,
                left,
                right,
            } => write!(
                f,
                "frame count mismatch between {left_backend} and {right_backend}: left={left} right={right}"
            ),
            Self::Frame {
                frame_index,
                mismatch,
            } => write!(f, "frame {frame_index} mismatch: {mismatch}"),
            Self::ProgressField {
                left_backend,
                right_backend,
                field,
                left,
                right,
            } => write!(
                f,
                "progress field `{field}` mismatch between {left_backend} and {right_backend}: left={left} right={right}"
            ),
            Self::TerminalState {
                left_backend,
                right_backend,
                left_packet,
                right_packet,
                left_error,
                right_error,
            } => write!(
                f,
                "terminal state mismatch between {left_backend} and {right_backend}: left_packet={left_packet:?} right_packet={right_packet:?} left_error={left_error:?} right_error={right_error:?}"
            ),
        }
    }
}

impl fmt::Display for FrameMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata { field, left, right } => {
                write!(f, "metadata field `{field}` differs: left={left} right={right}")
            }
            Self::PlaneLayout {
                plane,
                left_len,
                right_len,
                left_stride,
                right_stride,
            } => write!(
                f,
                "plane {plane} layout differs: left_len={left_len:?} right_len={right_len:?} left_stride={left_stride} right_stride={right_stride}"
            ),
            Self::PlaneByte {
                plane,
                byte_index,
                left,
                right,
            } => write!(
                f,
                "plane {plane} differs at byte {byte_index}: left={left:#04x} right={right:#04x}"
            ),
        }
    }
}

impl std::error::Error for DecodeMismatch {}

impl std::error::Error for FrameMismatch {}

fn compare_field<T>(field: &'static str, left: T, right: T) -> Result<(), FrameMismatch>
where
    T: fmt::Debug + PartialEq,
{
    if left == right {
        Ok(())
    } else {
        Err(FrameMismatch::Metadata {
            field,
            left: format!("{left:?}"),
            right: format!("{right:?}"),
        })
    }
}

/// Compare two owned frames for exact equality.
pub fn compare_frames(left: &OwnedFrame, right: &OwnedFrame) -> Result<(), FrameMismatch> {
    compare_field("width", left.width, right.width)?;
    compare_field("height", left.height, right.height)?;
    compare_field("bit_depth", left.bit_depth, right.bit_depth)?;
    compare_field("format", left.format, right.format)?;
    compare_field("format_raw", left.format_raw, right.format_raw)?;
    compare_field("color_range", left.color_range, right.color_range)?;
    compare_field(
        "chroma_sample_position",
        left.chroma_sample_position,
        right.chroma_sample_position,
    )?;
    compare_field("monochrome", left.monochrome, right.monochrome)?;
    compare_field("bytes_per_sample", left.bytes_per_sample, right.bytes_per_sample)?;

    for plane in 0..3 {
        let left_plane = left.plane(plane);
        let right_plane = right.plane(plane);
        if left_plane.is_none() || right_plane.is_none() {
            if left_plane.map(|p| p.len()) != right_plane.map(|p| p.len()) {
                return Err(FrameMismatch::PlaneLayout {
                    plane,
                    left_len: left_plane.map(<[u8]>::len),
                    right_len: right_plane.map(<[u8]>::len),
                    left_stride: left.strides[plane],
                    right_stride: right.strides[plane],
                });
            }
            continue;
        }

        let left_plane = left_plane.expect("checked above");
        let right_plane = right_plane.expect("checked above");
        if left_plane.len() != right_plane.len() || left.strides[plane] != right.strides[plane] {
            return Err(FrameMismatch::PlaneLayout {
                plane,
                left_len: Some(left_plane.len()),
                right_len: Some(right_plane.len()),
                left_stride: left.strides[plane],
                right_stride: right.strides[plane],
            });
        }

        if let Some((byte_index, (left_byte, right_byte))) = left_plane
            .iter()
            .copied()
            .zip(right_plane.iter().copied())
            .enumerate()
            .find(|(_, (l, r))| l != r)
        {
            return Err(FrameMismatch::PlaneByte {
                plane,
                byte_index,
                left: left_byte,
                right: right_byte,
            });
        }
    }

    Ok(())
}

/// Compare two decode snapshots for exact equality.
pub fn compare_snapshots(
    left: &DecodeSnapshot,
    right: &DecodeSnapshot,
) -> Result<(), DecodeMismatch> {
    if left.stream_info != right.stream_info {
        return Err(DecodeMismatch::StreamInfo {
            left_backend: left.backend,
            right_backend: right.backend,
            left: left.stream_info,
            right: right.stream_info,
        });
    }

    if left.frames.len() != right.frames.len() {
        return Err(DecodeMismatch::FrameCount {
            left_backend: left.backend,
            right_backend: right.backend,
            left: left.frames.len(),
            right: right.frames.len(),
        });
    }

    for (frame_index, (left_frame, right_frame)) in
        left.frames.iter().zip(right.frames.iter()).enumerate()
    {
        compare_frames(left_frame, right_frame)
            .map_err(|mismatch| DecodeMismatch::Frame { frame_index, mismatch })?;
    }

    Ok(())
}

/// Compare two decode outcomes, including early-stop failure state and parser progress.
pub fn compare_outcomes(left: &DecodeOutcome, right: &DecodeOutcome) -> Result<(), DecodeMismatch> {
    compare_snapshots(&left.snapshot, &right.snapshot)?;
    if left.stopped_at_packet != right.stopped_at_packet || left.terminal_error != right.terminal_error
    {
        return Err(DecodeMismatch::TerminalState {
            left_backend: left.snapshot.backend,
            right_backend: right.snapshot.backend,
            left_packet: left.stopped_at_packet,
            right_packet: right.stopped_at_packet,
            left_error: left.terminal_error.clone(),
            right_error: right.terminal_error.clone(),
        });
    }

    compare_progress(
        left.snapshot.backend,
        right.snapshot.backend,
        &left.snapshot.progress,
        &right.snapshot.progress,
    )?;

    Ok(())
}

/// Compare only the currently overlapping portions of two decode outcomes.
///
/// This is intended for cross-backend validation while one backend exposes
/// richer parser state than the other. Fields that are `None` on either side
/// are skipped instead of treated as mismatches.
pub fn compare_outcome_overlap(
    left: &DecodeOutcome,
    right: &DecodeOutcome,
) -> Result<(), DecodeMismatch> {
    compare_snapshot_overlap(&left.snapshot, &right.snapshot)?;

    if let (Some(left_packet), Some(right_packet)) = (left.stopped_at_packet, right.stopped_at_packet)
    {
        compare_progress_field(
            left.snapshot.backend,
            right.snapshot.backend,
            "stopped_at_packet",
            left_packet,
            right_packet,
        )?;
    }

    compare_progress_overlap(
        left.snapshot.backend,
        right.snapshot.backend,
        &left.snapshot.progress,
        &right.snapshot.progress,
    )?;

    Ok(())
}

fn compare_snapshot_overlap(
    left: &DecodeSnapshot,
    right: &DecodeSnapshot,
) -> Result<(), DecodeMismatch> {
    if let (Some(left_info), Some(right_info)) = (left.stream_info, right.stream_info) {
        if left_info != right_info {
            return Err(DecodeMismatch::StreamInfo {
                left_backend: left.backend,
                right_backend: right.backend,
                left: Some(left_info),
                right: Some(right_info),
            });
        }
    }

    if !left.frames.is_empty() && !right.frames.is_empty() {
        compare_snapshots(left, right)?;
    }

    Ok(())
}

fn compare_progress(
    left_backend: BackendKind,
    right_backend: BackendKind,
    left: &DecodeProgress,
    right: &DecodeProgress,
) -> Result<(), DecodeMismatch> {
    compare_progress_field(
        left_backend,
        right_backend,
        "backend",
        left.backend,
        right.backend,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "packets_parsed",
        left.packets_parsed,
        right.packets_parsed,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "obus_parsed",
        left.obus_parsed,
        right.obus_parsed,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "frame_packets_seen",
        left.frame_packets_seen,
        right.frame_packets_seen,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "sequence_header",
        left.sequence_header,
        right.sequence_header,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "stream_info",
        left.stream_info,
        right.stream_info,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "last_frame_packet_kind",
        left.last_frame_packet_kind,
        right.last_frame_packet_kind,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "last_frame_header",
        left.last_frame_header,
        right.last_frame_header,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "last_event",
        left.last_event,
        right.last_event,
    )?;
    compare_progress_field(
        left_backend,
        right_backend,
        "recent_events",
        left.recent_events,
        right.recent_events,
    )?;
    Ok(())
}

fn compare_progress_overlap(
    left_backend: BackendKind,
    right_backend: BackendKind,
    left: &DecodeProgress,
    right: &DecodeProgress,
) -> Result<(), DecodeMismatch> {
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "packets_parsed",
        left.packets_parsed,
        right.packets_parsed,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "obus_parsed",
        left.obus_parsed,
        right.obus_parsed,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "frame_packets_seen",
        left.frame_packets_seen,
        right.frame_packets_seen,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "sequence_header",
        left.sequence_header,
        right.sequence_header,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "stream_info",
        left.stream_info,
        right.stream_info,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "last_frame_packet_kind",
        left.last_frame_packet_kind,
        right.last_frame_packet_kind,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "last_frame_header",
        left.last_frame_header,
        right.last_frame_header,
    )?;
    compare_optional_progress_field(
        left_backend,
        right_backend,
        "last_event",
        left.last_event,
        right.last_event,
    )?;
    Ok(())
}

fn compare_progress_field<T>(
    left_backend: BackendKind,
    right_backend: BackendKind,
    field: &'static str,
    left: T,
    right: T,
) -> Result<(), DecodeMismatch>
where
    T: fmt::Debug + PartialEq,
{
    if left == right {
        Ok(())
    } else {
        Err(DecodeMismatch::ProgressField {
            left_backend,
            right_backend,
            field,
            left: format!("{left:?}"),
            right: format!("{right:?}"),
        })
    }
}

fn compare_optional_progress_field<T>(
    left_backend: BackendKind,
    right_backend: BackendKind,
    field: &'static str,
    left: Option<T>,
    right: Option<T>,
) -> Result<(), DecodeMismatch>
where
    T: fmt::Debug + PartialEq,
{
    if let (Some(left), Some(right)) = (left, right) {
        compare_progress_field(left_backend, right_backend, field, left, right)
    } else {
        Ok(())
    }
}

/// Decode the same IVF file through two backends and compare the resulting
/// normalized snapshots for exact equality.
pub fn compare_ivf_file<P: AsRef<Path>>(
    path: P,
    left_backend: BackendKind,
    right_backend: BackendKind,
    threads: Option<u32>,
) -> Result<(DecodeSnapshot, DecodeSnapshot), CompareError> {
    let path = path.as_ref();
    let left = decode_ivf_snapshot(
        IvfReader::new(BufReader::new(File::open(path)?))?,
        left_backend,
        threads,
    )?;
    let right = decode_ivf_snapshot(
        IvfReader::new(BufReader::new(File::open(path)?))?,
        right_backend,
        threads,
    )?;
    compare_snapshots(&left, &right).map_err(Box::new)?;
    Ok((left, right))
}

/// Decode the same IVF file through two backends and compare terminal outcomes,
/// including early-stop errors and parser progress.
pub fn compare_ivf_file_outcomes<P: AsRef<Path>>(
    path: P,
    left_backend: BackendKind,
    right_backend: BackendKind,
    threads: Option<u32>,
) -> Result<(DecodeOutcome, DecodeOutcome), CompareError> {
    let path = path.as_ref();
    let left = decode_ivf_outcome(
        IvfReader::new(BufReader::new(File::open(path)?))?,
        left_backend,
        threads,
    )?;
    let right = decode_ivf_outcome(
        IvfReader::new(BufReader::new(File::open(path)?))?,
        right_backend,
        threads,
    )?;
    compare_outcomes(&left, &right).map_err(Box::new)?;
    Ok((left, right))
}

/// Error returned by [`compare_ivf_file`].
#[derive(Debug, thiserror::Error)]
pub enum CompareError {
    /// Failed to open or parse the IVF file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A backend failed to decode the stream.
    #[error("decoder error: {0}")]
    Decoder(#[from] DecoderError),
    /// Both backends decoded, but the normalized outputs differed.
    #[error("decode outputs differed: {0}")]
    Mismatch(#[from] Box<DecodeMismatch>),
}

/// Decode an IVF stream into a fully-owned snapshot suitable for equality
/// comparison across backends.
pub fn decode_ivf_snapshot<R: Read>(
    mut ivf: IvfReader<R>,
    backend: BackendKind,
    threads: Option<u32>,
) -> Result<DecodeSnapshot, DecoderError> {
    let mut builder = Decoder::builder().backend(backend);
    if let Some(t) = threads {
        builder = builder.threads(t);
    }
    let mut decoder = builder.build()?;
    let mut stream_info = None;
    let mut frames = Vec::new();

    while let Some(packet) = ivf.next_frame().map_err(DecoderError::Io)? {
        decoder.decode(&packet.data)?;
        if stream_info.is_none() {
            stream_info = decoder.get_stream_info().ok();
        }
        for frame in decoder.get_frames() {
            frames.push(frame.to_owned());
        }
    }

    decoder.flush()?;
    for frame in decoder.get_frames() {
        frames.push(frame.to_owned());
    }

    Ok(DecodeSnapshot {
        backend,
        stream_info: stream_info.or(decoder.progress().stream_info),
        frames,
        progress: decoder.progress(),
    })
}

/// Decode an IVF stream into a normalized outcome that preserves early-stop errors.
pub fn decode_ivf_outcome<R: Read>(
    mut ivf: IvfReader<R>,
    backend: BackendKind,
    threads: Option<u32>,
) -> Result<DecodeOutcome, DecoderError> {
    let mut builder = Decoder::builder().backend(backend);
    if let Some(t) = threads {
        builder = builder.threads(t);
    }
    let mut decoder = builder.build()?;
    let mut stream_info = None;
    let mut frames = Vec::new();
    let mut stopped_at_packet = None;
    let mut terminal_error = None;

    for packet_index in 0usize.. {
        let Some(packet) = ivf.next_frame().map_err(DecoderError::Io)? else {
            break;
        };

        match decoder.decode(&packet.data) {
            Ok(()) => {}
            Err(err) => {
                stopped_at_packet = Some(packet_index);
                terminal_error = Some(err.to_string());
                break;
            }
        }
        if stream_info.is_none() {
            stream_info = decoder.get_stream_info().ok();
        }
        for frame in decoder.get_frames() {
            frames.push(frame.to_owned());
        }
    }

    if terminal_error.is_none() {
        decoder.flush()?;
        for frame in decoder.get_frames() {
            frames.push(frame.to_owned());
        }
    }

    let progress = decoder.progress();

    Ok(DecodeOutcome {
        snapshot: DecodeSnapshot {
            backend,
            stream_info: stream_info.or(progress.stream_info),
            frames,
            progress,
        },
        stopped_at_packet,
        terminal_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{ChromaSamplePosition, ColorRange};

    fn sample_frame() -> OwnedFrame {
        OwnedFrame {
            width: 2,
            height: 2,
            bit_depth: 8,
            format: None,
            format_raw: 0,
            color_range: ColorRange::Studio,
            chroma_sample_position: ChromaSamplePosition::Unspecified,
            monochrome: false,
            bytes_per_sample: 1,
            planes: [vec![1, 2, 3, 4], vec![5, 6], vec![7, 8]],
            strides: [2, 1, 1],
        }
    }

    #[test]
    fn compare_frames_detects_byte_mismatch() {
        let left = sample_frame();
        let mut right = sample_frame();
        right.planes[1][1] = 99;
        let mismatch = compare_frames(&left, &right).unwrap_err();
        assert_eq!(
            mismatch,
            FrameMismatch::PlaneByte {
                plane: 1,
                byte_index: 1,
                left: 6,
                right: 99,
            }
        );
    }

    #[test]
    fn compare_snapshots_detects_frame_count_mismatch() {
        let left = DecodeSnapshot {
            backend: BackendKind::Libavm,
            stream_info: None,
            frames: vec![sample_frame()],
            progress: DecodeProgress {
                backend: BackendKind::Libavm,
                packets_parsed: None,
                obus_parsed: None,
                frame_packets_seen: None,
                sequence_header: None,
                stream_info: None,
                last_frame_packet_kind: None,
                last_frame_header: None,
                last_event: None,
                recent_events: [None; 4],
            },
        };
        let right = DecodeSnapshot {
            backend: BackendKind::Rust,
            stream_info: None,
            frames: vec![],
            progress: DecodeProgress {
                backend: BackendKind::Rust,
                packets_parsed: Some(0),
                obus_parsed: Some(0),
                frame_packets_seen: Some(0),
                sequence_header: None,
                stream_info: None,
                last_frame_packet_kind: None,
                last_frame_header: None,
                last_event: None,
                recent_events: [None; 4],
            },
        };
        let mismatch = compare_snapshots(&left, &right).unwrap_err();
        assert_eq!(
            mismatch,
            DecodeMismatch::FrameCount {
                left_backend: BackendKind::Libavm,
                right_backend: BackendKind::Rust,
                left: 1,
                right: 0,
            }
        );
    }

    #[test]
    fn compare_outcomes_accepts_identical_terminal_failures() {
        let progress = DecodeProgress {
            backend: BackendKind::Rust,
            packets_parsed: Some(1),
            obus_parsed: Some(1),
            frame_packets_seen: Some(1),
            sequence_header: None,
            stream_info: None,
            last_frame_packet_kind: None,
            last_frame_header: None,
            last_event: None,
            recent_events: [None; 4],
        };
        let left = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Rust,
                stream_info: None,
                frames: vec![],
                progress,
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decoder feature not implemented: pending".into()),
        };
        let right = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Rust,
                stream_info: None,
                frames: vec![],
                progress,
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decoder feature not implemented: pending".into()),
        };

        compare_outcomes(&left, &right).expect("matching terminal outcome");
    }

    #[test]
    fn compare_outcomes_reports_specific_progress_field() {
        let left = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Rust,
                stream_info: None,
                frames: vec![],
                progress: DecodeProgress {
                    backend: BackendKind::Rust,
                    packets_parsed: Some(1),
                    obus_parsed: Some(2),
                    frame_packets_seen: Some(1),
                    sequence_header: None,
                    stream_info: None,
                    last_frame_packet_kind: None,
                    last_frame_header: None,
                    last_event: None,
                    recent_events: [None; 4],
                },
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decoder feature not implemented: pending".into()),
        };
        let right = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Rust,
                stream_info: None,
                frames: vec![],
                progress: DecodeProgress {
                    backend: BackendKind::Rust,
                    packets_parsed: Some(1),
                    obus_parsed: Some(3),
                    frame_packets_seen: Some(1),
                    sequence_header: None,
                    stream_info: None,
                    last_frame_packet_kind: None,
                    last_frame_header: None,
                    last_event: None,
                    recent_events: [None; 4],
                },
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decoder feature not implemented: pending".into()),
        };

        let mismatch = compare_outcomes(&left, &right).expect_err("progress mismatch");
        assert_eq!(
            mismatch,
            DecodeMismatch::ProgressField {
                left_backend: BackendKind::Rust,
                right_backend: BackendKind::Rust,
                field: "obus_parsed",
                left: "Some(2)".into(),
                right: "Some(3)".into(),
            }
        );
    }

    #[test]
    fn compare_outcome_overlap_ignores_missing_progress_fields() {
        let left = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Rust,
                stream_info: None,
                frames: vec![],
                progress: DecodeProgress {
                    backend: BackendKind::Rust,
                    packets_parsed: Some(1),
                    obus_parsed: Some(3),
                    frame_packets_seen: Some(1),
                    sequence_header: None,
                    stream_info: None,
                    last_frame_packet_kind: None,
                    last_frame_header: None,
                    last_event: None,
                    recent_events: [None; 4],
                },
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decoder feature not implemented: pending".into()),
        };
        let right = DecodeOutcome {
            snapshot: DecodeSnapshot {
                backend: BackendKind::Libavm,
                stream_info: None,
                frames: vec![],
                progress: DecodeProgress {
                    backend: BackendKind::Libavm,
                    packets_parsed: Some(1),
                    obus_parsed: None,
                    frame_packets_seen: None,
                    sequence_header: None,
                    stream_info: None,
                    last_frame_packet_kind: None,
                    last_frame_header: None,
                    last_event: None,
                    recent_events: [None; 4],
                },
            },
            stopped_at_packet: Some(0),
            terminal_error: Some("decode failed".into()),
        };

        compare_outcome_overlap(&left, &right).expect("overlap comparison");
    }
}
