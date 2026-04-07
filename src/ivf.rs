use std::io::{self, Read};

const DEFAULT_MAX_FRAME_SIZE: usize = 64 * 1024 * 1024; // 64 MiB

#[derive(Debug)]
pub struct IvfHeader {
    pub fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub framerate_num: u32,
    pub framerate_den: u32,
    pub num_frames: u32,
}

#[derive(Debug)]
pub struct IvfFrame {
    pub timestamp: u64,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct IvfReader<R: Read> {
    reader: R,
    header: IvfHeader,
    max_frame_size: usize,
}

impl<R: Read> IvfReader<R> {
    pub fn new(reader: R) -> io::Result<Self> {
        Self::with_max_frame_size(reader, DEFAULT_MAX_FRAME_SIZE)
    }

    pub fn with_max_frame_size(mut reader: R, max: usize) -> io::Result<Self> {
        let mut signature = [0u8; 4];
        reader.read_exact(&mut signature)?;
        if &signature != b"DKIF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Not an IVF file"));
        }

        let mut buf = [0u8; 28];
        reader.read_exact(&mut buf)?;

        let version = u16::from_le_bytes([buf[0], buf[1]]);
        if version != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported IVF version {version}"),
            ));
        }

        let header_len = u16::from_le_bytes([buf[2], buf[3]]);
        if header_len != 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected IVF header length {header_len}"),
            ));
        }

        let header = IvfHeader {
            fourcc: [buf[4], buf[5], buf[6], buf[7]],
            width: u16::from_le_bytes([buf[8], buf[9]]),
            height: u16::from_le_bytes([buf[10], buf[11]]),
            framerate_num: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            framerate_den: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            num_frames: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
        };

        if header.width == 0 || header.height == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IVF header has zero dimension",
            ));
        }

        Ok(Self { reader, header, max_frame_size: max })
    }

    pub fn header(&self) -> &IvfHeader {
        &self.header
    }

    pub fn next_frame(&mut self) -> io::Result<Option<IvfFrame>> {
        let mut size_buf = [0u8; 4];
        if let Err(e) = self.reader.read_exact(&mut size_buf) {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e);
        }
        let size = u32::from_le_bytes(size_buf) as usize;

        if size > self.max_frame_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("IVF frame size {size} exceeds limit {}", self.max_frame_size),
            ));
        }

        let mut ts_buf = [0u8; 8];
        self.reader.read_exact(&mut ts_buf)?;
        let timestamp = u64::from_le_bytes(ts_buf);

        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e))?;
        data.resize(size, 0);
        self.reader.read_exact(&mut data)?;

        Ok(Some(IvfFrame { timestamp, data }))
    }
}
