//! Photon Protocol18 values, as used by Albion Online since its 2026-04-13 patch.
//!
//! Only type codes whose wire format is described the same way by the independent sources
//! consulted (see docs/captura-offline.md) are decoded. Codes the sources disagree on
//! (Hashtable, nested arrays, dictionary arrays) and unknown codes stop the decoding of
//! that message with `UnsupportedType` instead of guessing. Every count and length is
//! checked against the bytes left, nesting is limited, and the number of decoded values per
//! message is capped, so malformed input ends in an error, never in a panic or a large
//! allocation.

/// Values in one message beyond which decoding stops.
pub const MAX_VALUES: usize = 65_536;
pub const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Byte(u8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Dictionary(Vec<(Value, Value)>),
    Custom { code: u8, data: Vec<u8> },
}
impl Value {
    /// Integer value of any integer type, for fields whose exact width varies.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Byte(value) => Some(i64::from(*value)),
            Self::Short(value) => Some(i64::from(*value)),
            Self::Int(value) => Some(i64::from(*value)),
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Byte(_) | Self::Short(_) | Self::Int(_) | Self::Long(_) => "integer",
            Self::Float(_) | Self::Double(_) => "float",
            Self::String(_) => "string",
            Self::Bytes(_) => "bytes",
            Self::Array(_) => "array",
            Self::Dictionary(_) => "dictionary",
            Self::Custom { .. } => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    UnsupportedType(u8),
    TooLarge,
    TooDeep,
    InvalidUtf8,
    DuplicateParameter(u8),
    TrailingBytes,
}

#[derive(Debug)]
pub struct Event {
    /// The one-byte dispatch code; Albion puts the real event code in parameter 252.
    pub code: u8,
    pub parameters: Vec<(u8, Value)>,
}
impl Event {
    pub fn parameter(&self, key: u8) -> Option<&Value> {
        self.parameters
            .iter()
            .find(|(parameter, _)| *parameter == key)
            .map(|(_, value)| value)
    }
}

/// Decodes an event message body. On failure the parameters read before the error are
/// returned too, so the caller can still tell which event failed.
pub fn decode_event(data: &[u8]) -> Result<Event, (DecodeError, Event)> {
    let mut reader = Reader {
        data,
        offset: 0,
        values: 0,
    };
    let mut event = Event {
        code: 0,
        parameters: Vec::new(),
    };
    let result = (|| {
        event.code = reader.byte()?;
        let count = reader.count()?;
        if count > 256 {
            return Err(DecodeError::TooLarge);
        }
        for _ in 0..count {
            let key = reader.byte()?;
            let type_code = reader.byte()?;
            let value = reader.value(type_code, 0)?;
            if event
                .parameters
                .iter()
                .any(|(existing, _)| *existing == key)
            {
                return Err(DecodeError::DuplicateParameter(key));
            }
            event.parameters.push((key, value));
        }
        if reader.offset != data.len() {
            return Err(DecodeError::TrailingBytes);
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(event),
        Err(error) => Err((error, event)),
    }
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
    values: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }
    fn take(&mut self, length: usize) -> Result<&[u8], DecodeError> {
        if self.remaining() < length {
            return Err(DecodeError::Truncated);
        }
        let bytes = &self.data[self.offset..self.offset + length];
        self.offset += length;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn varint(&mut self, max_bits: u32) -> Result<u64, DecodeError> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= max_bits {
                return Err(DecodeError::TooLarge);
            }
        }
    }
    /// Collection sizes and string lengths: an unsigned variable-length integer.
    fn count(&mut self) -> Result<usize, DecodeError> {
        let value = self.varint(35)?;
        let value = usize::try_from(value).map_err(|_| DecodeError::TooLarge)?;
        if value > MAX_VALUES {
            return Err(DecodeError::TooLarge);
        }
        Ok(value)
    }
    /// Zigzag-encoded signed integers.
    fn compressed_int(&mut self) -> Result<i32, DecodeError> {
        let raw = u32::try_from(self.varint(35)?).map_err(|_| DecodeError::TooLarge)?;
        Ok(((raw >> 1) as i32) ^ -((raw & 1) as i32))
    }
    fn compressed_long(&mut self) -> Result<i64, DecodeError> {
        let raw = self.varint(70)?;
        Ok(((raw >> 1) as i64) ^ -((raw & 1) as i64))
    }
    fn string(&mut self) -> Result<String, DecodeError> {
        let length = self.count()?;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::InvalidUtf8)
    }
    fn sized_bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let length = self.count()?;
        Ok(self.take(length)?.to_vec())
    }
    /// Elements that occupy at least one byte each cannot outnumber the bytes left.
    fn element_count(&mut self, minimum_size: usize) -> Result<usize, DecodeError> {
        let count = self.count()?;
        if minimum_size > 0 && count > self.remaining() / minimum_size {
            return Err(DecodeError::Truncated);
        }
        self.spend(count)?;
        Ok(count)
    }
    fn spend(&mut self, values: usize) -> Result<(), DecodeError> {
        self.values = self.values.saturating_add(values);
        if self.values > MAX_VALUES {
            return Err(DecodeError::TooLarge);
        }
        Ok(())
    }

    fn value(&mut self, type_code: u8, depth: usize) -> Result<Value, DecodeError> {
        if depth > MAX_DEPTH {
            return Err(DecodeError::TooDeep);
        }
        self.spend(1)?;
        Ok(match type_code {
            0 | 8 => Value::Null,
            2 => Value::Bool(self.byte()? != 0),
            3 => Value::Byte(self.byte()?),
            4 => Value::Short(self.u16()? as i16),
            5 => {
                let bytes = self.take(4)?;
                Value::Float(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            6 => {
                let bytes = self.take(8)?;
                let mut array = [0u8; 8];
                array.copy_from_slice(bytes);
                Value::Double(f64::from_le_bytes(array))
            }
            7 => Value::String(self.string()?),
            9 => Value::Int(self.compressed_int()?),
            10 => Value::Long(self.compressed_long()?),
            11 => Value::Int(i32::from(self.byte()?)),
            12 => Value::Int(-i32::from(self.byte()?)),
            13 => Value::Int(i32::from(self.u16()?)),
            14 => Value::Int(-i32::from(self.u16()?)),
            15 => Value::Long(i64::from(self.byte()?)),
            16 => Value::Long(-i64::from(self.byte()?)),
            17 => Value::Long(i64::from(self.u16()?)),
            18 => Value::Long(-i64::from(self.u16()?)),
            19 => {
                let code = self.byte()?;
                Value::Custom {
                    code,
                    data: self.sized_bytes()?,
                }
            }
            20 => self.dictionary(depth)?,
            23 => {
                // Object array: every element carries its own type code.
                let count = self.element_count(1)?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    let element_type = self.byte()?;
                    items.push(self.value(element_type, depth + 1)?);
                }
                Value::Array(items)
            }
            27 => Value::Bool(false),
            28 => Value::Bool(true),
            29 => Value::Short(0),
            30 => Value::Int(0),
            31 => Value::Long(0),
            32 => Value::Float(0.0),
            33 => Value::Double(0.0),
            34 => Value::Byte(0),
            0x42 => {
                // Booleans packed eight per byte, lowest bit first.
                let count = self.count()?;
                let bytes = self.take(count.div_ceil(8))?.to_vec();
                self.spend(count)?;
                Value::Array(
                    (0..count)
                        .map(|index| Value::Bool(bytes[index / 8] & (1 << (index % 8)) != 0))
                        .collect(),
                )
            }
            0x43 => Value::Bytes(self.sized_bytes()?),
            0x44 => self.array(2, |reader| Ok(Value::Short(reader.u16()? as i16)))?,
            0x45 => self.array(4, |reader| {
                let bytes = reader.take(4)?;
                Ok(Value::Float(f32::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])))
            })?,
            0x46 => self.array(8, |reader| {
                let bytes = reader.take(8)?;
                let mut array = [0u8; 8];
                array.copy_from_slice(bytes);
                Ok(Value::Double(f64::from_le_bytes(array)))
            })?,
            0x47 => self.array(1, |reader| Ok(Value::String(reader.string()?)))?,
            0x49 => self.array(1, |reader| Ok(Value::Int(reader.compressed_int()?)))?,
            0x4A => self.array(1, |reader| Ok(Value::Long(reader.compressed_long()?)))?,
            0x53 => {
                // Custom-type array: the custom type once, then sized payloads.
                let count = self.element_count(1)?;
                let code = self.byte()?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(Value::Custom {
                        code,
                        data: self.sized_bytes()?,
                    });
                }
                Value::Array(items)
            }
            // Slim custom types carry their custom code in the type code itself.
            0x80..=0xE4 => Value::Custom {
                code: type_code,
                data: self.sized_bytes()?,
            },
            other => return Err(DecodeError::UnsupportedType(other)),
        })
    }

    fn array(
        &mut self,
        element_size: usize,
        mut element: impl FnMut(&mut Self) -> Result<Value, DecodeError>,
    ) -> Result<Value, DecodeError> {
        let count = self.element_count(element_size)?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            items.push(element(self)?);
        }
        Ok(Value::Array(items))
    }

    fn dictionary(&mut self, depth: usize) -> Result<Value, DecodeError> {
        let key_type = self.byte()?;
        let value_type = self.byte()?;
        // Sources disagree on nested dictionary/array type descriptors; refuse them.
        for type_code in [key_type, value_type] {
            if type_code == 20 || type_code == 21 || type_code & 0x40 != 0 && type_code < 0x80 {
                return Err(DecodeError::UnsupportedType(type_code));
            }
        }
        let count = self.element_count(0)?;
        let mut entries = Vec::with_capacity(count.min(self.remaining()));
        for _ in 0..count {
            let key_code = if key_type == 0 {
                self.byte()?
            } else {
                key_type
            };
            let key = self.value(key_code, depth + 1)?;
            let value_code = if value_type == 0 {
                self.byte()?
            } else {
                value_type
            };
            let value = self.value(value_code, depth + 1)?;
            entries.push((key, value));
        }
        Ok(Value::Dictionary(entries))
    }
}
