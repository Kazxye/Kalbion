//! Capture file containers: classic PCAP (micro- and nanosecond, either byte order) and
//! PCAPNG. Only reads bytes the user recorded; nothing here opens a network interface.
//!
//! Every packet record gets a 1-based position in the file. Records are read one at a time
//! with hard size limits, so a hostile or corrupted file cannot make the reader allocate
//! more than one record's worth of memory.
use crate::CaptureError;
use sha2::{Digest, Sha256};
use std::io::Read;

/// Link-layer types this reader hands on (see `net`); others are counted and skipped.
pub const LINK_NULL: u32 = 0;
pub const LINK_ETHERNET: u32 = 1;
pub const LINK_RAW: u32 = 101;
pub const LINK_LOOP: u32 = 108;
pub const LINK_LINUX_SLL: u32 = 113;
pub const LINK_IPV4: u32 = 228;
pub const LINK_IPV6: u32 = 229;
pub const LINK_LINUX_SLL2: u32 = 276;

const PCAP_MICROS: u32 = 0xA1B2_C3D4;
const PCAP_NANOS: u32 = 0xA1B2_3C4D;
const PCAPNG_SECTION: u32 = 0x0A0D_0D0A;
const PCAPNG_BYTE_ORDER: u32 = 0x1A2B_3C4D;
/// Non-packet PCAPNG blocks (statistics, name resolution, comments) are skipped, but never
/// buffered beyond this size.
const MAX_OTHER_BLOCK: u32 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Pcap,
    Pcapng,
}

#[derive(Debug, Clone)]
pub struct Frame {
    /// 1-based position of the packet record in the file.
    pub index: u64,
    /// Nanoseconds since the Unix epoch, as written by the capturing tool.
    pub timestamp_ns: i128,
    pub link_type: u32,
    pub data: Vec<u8>,
    /// The record holds fewer bytes than the packet had on the wire (snap length).
    pub cut_short: bool,
}

#[derive(Clone, Copy)]
enum Order {
    Little,
    Big,
}
impl Order {
    fn u16(self, bytes: &[u8]) -> u16 {
        let bytes = [bytes[0], bytes[1]];
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }
    fn u32(self, bytes: &[u8]) -> u32 {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }
}

#[derive(Clone, Copy)]
struct Interface {
    link_type: u32,
    /// Timestamp unit: `10^-n` seconds, or `2^-n` seconds when `binary` is set.
    resolution: u8,
    binary: bool,
}

enum Kind {
    Pcap {
        order: Order,
        nanos: bool,
        link_type: u32,
    },
    Pcapng {
        order: Order,
        interfaces: Vec<Interface>,
    },
}

/// Reads packet records and hashes everything read: the whole file (`file_sha256`) and its
/// beginning up to the end of the first packet record (`fingerprint`). The fingerprint
/// identifies the capture itself, so a copy taken while the capture was still being written
/// and the finished file give the same event identities for the packets they share.
pub struct Reader<R> {
    source: R,
    kind: Kind,
    format: Format,
    max_packet: usize,
    max_bytes: u64,
    bytes_read: u64,
    file_hash: Sha256,
    fingerprint: Option<Sha256>,
    fingerprint_done: Option<[u8; 32]>,
    next_index: u64,
    /// The file ended in the middle of a record.
    pub truncated: bool,
}

impl<R: Read> Reader<R> {
    pub fn new(source: R, max_packet: usize, max_bytes: u64) -> Result<Self, CaptureError> {
        let mut reader = Self {
            source,
            kind: Kind::Pcap {
                order: Order::Little,
                nanos: false,
                link_type: 0,
            },
            format: Format::Pcap,
            max_packet,
            max_bytes,
            bytes_read: 0,
            file_hash: Sha256::new(),
            fingerprint: Some(Sha256::new()),
            fingerprint_done: None,
            next_index: 1,
            truncated: false,
        };
        let mut magic = [0u8; 4];
        if !reader.fill(&mut magic)? {
            return Err(CaptureError::Unsupported(
                "Arquivo vazio ou curto demais para ser uma captura".into(),
            ));
        }
        let little = u32::from_le_bytes(magic);
        let big = u32::from_be_bytes(magic);
        if little == PCAPNG_SECTION {
            reader.format = Format::Pcapng;
            reader.kind = Kind::Pcapng {
                order: Order::Little,
                interfaces: Vec::new(),
            };
            reader.section_header()?;
        } else if [PCAP_MICROS, PCAP_NANOS].contains(&little)
            || [PCAP_MICROS, PCAP_NANOS].contains(&big)
        {
            let order = if [PCAP_MICROS, PCAP_NANOS].contains(&little) {
                Order::Little
            } else {
                Order::Big
            };
            let nanos = order.u32(&magic) == PCAP_NANOS;
            let mut header = [0u8; 20];
            if !reader.fill(&mut header)? {
                return Err(CaptureError::Unsupported(
                    "Cabeçalho PCAP incompleto".into(),
                ));
            }
            let major = order.u16(&header[0..2]);
            if major != 2 {
                return Err(CaptureError::Unsupported(format!(
                    "Versão de PCAP não suportada ({major})"
                )));
            }
            // The upper bits of this field carry FCS information, not the link type.
            let link_type = order.u32(&header[16..20]) & 0xFFFF;
            reader.kind = Kind::Pcap {
                order,
                nanos,
                link_type,
            };
        } else {
            return Err(CaptureError::Unsupported(
                "Formato de arquivo não reconhecido; use PCAP ou PCAPNG".into(),
            ));
        }
        Ok(reader)
    }

    pub fn format(&self) -> Format {
        self.format
    }
    /// SHA-256 of the file start up to the end of the first packet record.
    pub fn fingerprint(&mut self) -> [u8; 32] {
        if let Some(done) = self.fingerprint_done {
            return done;
        }
        let hash = self
            .fingerprint
            .take()
            .unwrap_or_default()
            .finalize()
            .into();
        self.fingerprint_done = Some(hash);
        hash
    }
    /// Reads whatever is left (so the whole-file hash covers it) and returns the hash.
    pub fn finish(mut self) -> Result<([u8; 32], [u8; 32]), CaptureError> {
        let fingerprint = self.fingerprint();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = self.source.read(&mut buffer).map_err(CaptureError::Io)?;
            if read == 0 {
                break;
            }
            self.account(&buffer[..read])?;
        }
        Ok((fingerprint, self.file_hash.finalize().into()))
    }

    /// The next packet record, or `None` at the end of the file. A record that is cut off by
    /// the end of the file sets `truncated` and ends the reading.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, CaptureError> {
        loop {
            let frame = match self.kind {
                Kind::Pcap { .. } => self.pcap_record()?,
                Kind::Pcapng { .. } => match self.pcapng_block()? {
                    Block::Packet(frame) => Some(frame),
                    Block::Other => continue,
                    Block::End => None,
                },
            };
            if frame.is_some() && self.fingerprint_done.is_none() {
                self.fingerprint();
            }
            return Ok(frame);
        }
    }

    fn account(&mut self, bytes: &[u8]) -> Result<(), CaptureError> {
        self.bytes_read += bytes.len() as u64;
        if self.bytes_read > self.max_bytes {
            return Err(CaptureError::Limit(format!(
                "Arquivo maior que o limite de {} MB",
                self.max_bytes / (1024 * 1024)
            )));
        }
        self.file_hash.update(bytes);
        if let Some(fingerprint) = self.fingerprint.as_mut() {
            fingerprint.update(bytes);
        }
        Ok(())
    }
    /// Fills `buffer` completely. `Ok(false)` when the file ended before the first byte;
    /// an end in the middle of the buffer marks the file as truncated.
    fn fill(&mut self, buffer: &mut [u8]) -> Result<bool, CaptureError> {
        let mut filled = 0;
        while filled < buffer.len() {
            match self.source.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(CaptureError::Io(error)),
            }
        }
        let (read, _) = buffer.split_at(filled);
        let read = read.to_vec();
        self.account(&read)?;
        if filled == buffer.len() {
            return Ok(true);
        }
        if filled > 0 {
            self.truncated = true;
        }
        Ok(false)
    }
    fn take_index(&mut self) -> u64 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }

    fn pcap_record(&mut self) -> Result<Option<Frame>, CaptureError> {
        let Kind::Pcap {
            order,
            nanos,
            link_type,
        } = self.kind
        else {
            return Ok(None);
        };
        let mut header = [0u8; 16];
        if !self.fill(&mut header)? {
            return Ok(None);
        }
        let seconds = order.u32(&header[0..4]);
        let fraction = order.u32(&header[4..8]);
        let captured = order.u32(&header[8..12]) as usize;
        let original = order.u32(&header[12..16]) as usize;
        if captured > self.max_packet {
            return Err(CaptureError::Malformed(format!(
                "Registro de pacote com {captured} bytes excede o limite de {} bytes",
                self.max_packet
            )));
        }
        let mut data = vec![0u8; captured];
        if !self.fill(&mut data)? && captured > 0 {
            self.truncated = true;
            return Ok(None);
        }
        let fraction_ns = if nanos {
            i128::from(fraction)
        } else {
            i128::from(fraction) * 1000
        };
        Ok(Some(Frame {
            index: self.take_index(),
            timestamp_ns: i128::from(seconds) * 1_000_000_000 + fraction_ns,
            link_type,
            cut_short: captured < original,
            data,
        }))
    }

    fn section_header(&mut self) -> Result<(), CaptureError> {
        // Block type was already read; then total length and the byte-order magic.
        let mut head = [0u8; 8];
        if !self.fill(&mut head)? {
            return Err(CaptureError::Unsupported(
                "Cabeçalho PCAPNG incompleto".into(),
            ));
        }
        let order = if u32::from_le_bytes([head[4], head[5], head[6], head[7]]) == PCAPNG_BYTE_ORDER
        {
            Order::Little
        } else if u32::from_be_bytes([head[4], head[5], head[6], head[7]]) == PCAPNG_BYTE_ORDER {
            Order::Big
        } else {
            return Err(CaptureError::Unsupported(
                "Seção PCAPNG com ordem de bytes inválida".into(),
            ));
        };
        let total = order.u32(&head[0..4]);
        if total < 28 || total % 4 != 0 || total > MAX_OTHER_BLOCK {
            return Err(CaptureError::Malformed(
                "Bloco de seção PCAPNG inválido".into(),
            ));
        }
        // Rest of the block: version, section length, options, trailing length.
        let mut rest = vec![0u8; total as usize - 12];
        if !self.fill(&mut rest)? {
            return Err(CaptureError::Unsupported(
                "Cabeçalho PCAPNG incompleto".into(),
            ));
        }
        let major = order.u16(&rest[0..2]);
        if major != 1 {
            return Err(CaptureError::Unsupported(format!(
                "Versão de PCAPNG não suportada ({major})"
            )));
        }
        if order.u32(&rest[rest.len() - 4..]) != total {
            return Err(CaptureError::Malformed(
                "Bloco de seção PCAPNG inválido".into(),
            ));
        }
        self.kind = Kind::Pcapng {
            order,
            interfaces: Vec::new(),
        };
        Ok(())
    }

    fn pcapng_block(&mut self) -> Result<Block, CaptureError> {
        let order = match self.kind {
            Kind::Pcapng { order, .. } => order,
            Kind::Pcap { .. } => return Ok(Block::End),
        };
        let mut head = [0u8; 8];
        if !self.fill(&mut head)? {
            return Ok(Block::End);
        }
        // A new section may switch byte order, so its type is recognized in either order.
        if u32::from_le_bytes([head[0], head[1], head[2], head[3]]) == PCAPNG_SECTION {
            return self.next_section(head);
        }
        let block_type = order.u32(&head[0..4]);
        let total = order.u32(&head[4..8]);
        let packet_block = matches!(block_type, 2 | 3 | 6);
        let limit = if packet_block {
            self.max_packet as u32 + 64
        } else {
            MAX_OTHER_BLOCK
        };
        if total < 12 || total % 4 != 0 {
            return Err(CaptureError::Malformed(format!(
                "Bloco PCAPNG com tamanho inválido ({total})"
            )));
        }
        if total > limit {
            return Err(CaptureError::Malformed(format!(
                "Bloco PCAPNG com {total} bytes excede o limite"
            )));
        }
        let mut body = vec![0u8; total as usize - 8];
        if !self.fill(&mut body)? {
            self.truncated = true;
            return Ok(Block::End);
        }
        if order.u32(&body[body.len() - 4..]) != total {
            return Err(CaptureError::Malformed(
                "Bloco PCAPNG com tamanhos inconsistentes".into(),
            ));
        }
        let body = &body[..body.len() - 4];
        match block_type {
            1 => {
                self.interface(order, body)?;
                Ok(Block::Other)
            }
            6 | 2 => self
                .enhanced_packet(order, block_type, body)
                .map(Block::Packet),
            3 => {
                // Simple packet blocks carry no timestamp; Kalbion needs one per event.
                let index = self.take_index();
                Err(CaptureError::Unsupported(format!(
                    "Pacote {index} sem horário (Simple Packet Block); grave a captura com horários"
                )))
            }
            _ => Ok(Block::Other),
        }
    }

    fn next_section(&mut self, head: [u8; 8]) -> Result<Block, CaptureError> {
        let mut magic = [0u8; 4];
        if !self.fill(&mut magic)? {
            return Ok(Block::End);
        }
        let order = if u32::from_le_bytes(magic) == PCAPNG_BYTE_ORDER {
            Order::Little
        } else if u32::from_be_bytes(magic) == PCAPNG_BYTE_ORDER {
            Order::Big
        } else {
            return Err(CaptureError::Malformed("Seção PCAPNG inválida".into()));
        };
        let total = order.u32(&head[4..8]);
        if total < 28 || total % 4 != 0 || total > MAX_OTHER_BLOCK {
            return Err(CaptureError::Malformed(
                "Bloco de seção PCAPNG inválido".into(),
            ));
        }
        let mut rest = vec![0u8; total as usize - 12];
        if !self.fill(&mut rest)? {
            self.truncated = true;
            return Ok(Block::End);
        }
        if order.u16(&rest[0..2]) != 1 || order.u32(&rest[rest.len() - 4..]) != total {
            return Err(CaptureError::Malformed("Seção PCAPNG inválida".into()));
        }
        self.kind = Kind::Pcapng {
            order,
            interfaces: Vec::new(),
        };
        Ok(Block::Other)
    }

    fn interface(&mut self, order: Order, body: &[u8]) -> Result<(), CaptureError> {
        if body.len() < 8 {
            return Err(CaptureError::Malformed("Interface PCAPNG inválida".into()));
        }
        let mut interface = Interface {
            link_type: u32::from(order.u16(&body[0..2])),
            resolution: 6,
            binary: false,
        };
        let mut options = &body[8..];
        while options.len() >= 4 {
            let code = order.u16(&options[0..2]);
            let length = order.u16(&options[2..4]) as usize;
            let padded = length.div_ceil(4) * 4;
            if code == 0 || options.len() < 4 + padded {
                break;
            }
            if code == 9 && length == 1 {
                let value = options[4];
                interface.binary = value & 0x80 != 0;
                interface.resolution = value & 0x7F;
            }
            options = &options[4 + padded..];
        }
        if (!interface.binary && interface.resolution > 12)
            || (interface.binary && interface.resolution > 63)
        {
            return Err(CaptureError::Unsupported(
                "Resolução de horário PCAPNG não suportada".into(),
            ));
        }
        if let Kind::Pcapng { interfaces, .. } = &mut self.kind {
            if interfaces.len() >= 256 {
                return Err(CaptureError::Limit("Interfaces PCAPNG demais".into()));
            }
            interfaces.push(interface);
        }
        Ok(())
    }

    fn enhanced_packet(
        &mut self,
        order: Order,
        block_type: u32,
        body: &[u8],
    ) -> Result<Frame, CaptureError> {
        let index = self.take_index();
        // Enhanced (6) and obsolete (2) packet blocks differ only in the interface field size.
        let (interface_id, rest) = if block_type == 6 {
            if body.len() < 20 {
                return Err(CaptureError::Malformed(format!(
                    "Pacote {index} PCAPNG inválido"
                )));
            }
            (order.u32(&body[0..4]) as usize, &body[4..])
        } else {
            if body.len() < 20 {
                return Err(CaptureError::Malformed(format!(
                    "Pacote {index} PCAPNG inválido"
                )));
            }
            (order.u16(&body[0..2]) as usize, &body[4..])
        };
        let interface = match &self.kind {
            Kind::Pcapng { interfaces, .. } => interfaces.get(interface_id).copied(),
            Kind::Pcap { .. } => None,
        }
        .ok_or_else(|| {
            CaptureError::Malformed(format!("Pacote {index} cita interface PCAPNG inexistente"))
        })?;
        let high = u64::from(order.u32(&rest[0..4]));
        let low = u64::from(order.u32(&rest[4..8]));
        let captured = order.u32(&rest[8..12]) as usize;
        let original = order.u32(&rest[12..16]) as usize;
        let data = &rest[16..];
        if captured > data.len() || captured > self.max_packet {
            return Err(CaptureError::Malformed(format!(
                "Pacote {index} PCAPNG com tamanho inconsistente"
            )));
        }
        let ticks = u128::from((high << 32) | low);
        let timestamp_ns = if interface.binary {
            (ticks * 1_000_000_000) >> interface.resolution
        } else {
            let unit = 10u128.pow(u32::from(interface.resolution));
            ticks * 1_000_000_000 / unit
        };
        Ok(Frame {
            index,
            timestamp_ns: i128::try_from(timestamp_ns).unwrap_or(i128::MAX),
            link_type: interface.link_type,
            cut_short: captured < original,
            data: data[..captured].to_vec(),
        })
    }
}

enum Block {
    Packet(Frame),
    Other,
    End,
}
