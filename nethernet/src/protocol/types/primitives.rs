use crate::protocol::constants::MAX_BYTES;
use std::io::{self, Read, Write};

/// Reads a single byte from the given reader.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// let mut c = Cursor::new([0x42u8]);
/// let b = read_u8(&mut c).unwrap();
/// assert_eq!(b, 0x42);
/// ```
pub fn read_u8(r: &mut dyn Read) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

/// Writes a single byte to the provided writer.
///
/// # Returns
///
/// `Ok(())` on success, or an I/O error if the write fails.
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_u8(&mut buf, 0x42).unwrap();
/// assert_eq!(buf, vec![0x42]);
/// ```
pub fn write_u8(w: &mut dyn Write, value: u8) -> io::Result<()> {
    w.write_all(&[value])
}

/// Read a 32-bit little-endian signed integer from a reader.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// let bytes = 0x01020304i32.to_le_bytes();
/// let mut cursor = Cursor::new(bytes);
/// let val = nethernet::protocol::types::primitives::read_i32_le(&mut cursor).unwrap();
/// assert_eq!(val, 0x01020304i32);
/// ```
pub fn read_i32_le(r: &mut dyn Read) -> io::Result<i32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

/// Writes a 32-bit signed integer to the writer in little-endian byte order.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// let mut buf = Cursor::new(Vec::new());
/// write_i32_le(&mut buf, -1).unwrap();
/// assert_eq!(buf.into_inner(), (-1i32).to_le_bytes().to_vec());
/// ```
pub fn write_i32_le(w: &mut dyn Write, value: i32) -> io::Result<()> {
    w.write_all(&value.to_le_bytes())
}

/// Reads a length-prefixed byte array using a custom length reader.
///
/// The provided `read_length` closure is invoked to obtain the length prefix; the function
/// validates that the declared length does not exceed `MAX_BYTES`, converts it to `usize`,
/// allocates a buffer of that size, and reads exactly that many bytes into the returned `Vec<u8>`.
///
/// # Parameters
///
/// - `r`: source implementing `Read`.
/// - `read_length`: closure that reads and returns the length prefix from the reader.
///
/// # Returns
///
/// A `Vec<u8>` containing the exact number of bytes specified by the length prefix.
///
/// # Examples
///
/// ```
/// use std::io::{Cursor, Read};
///
/// // Cursor layout: [length: u8, data...]
/// let mut cursor = Cursor::new(vec![3u8, 10, 20, 30]);
/// let bytes = read_bytes(&mut cursor, |r| {
///     let mut b = [0u8; 1];
///     r.read_exact(&mut b)?;
///     Ok(b[0])
/// }).unwrap();
/// assert_eq!(bytes, vec![10u8, 20, 30]);
/// ```
pub fn read_bytes<L: Into<u64>>(
    r: &mut dyn Read,
    read_length: impl Fn(&mut dyn Read) -> io::Result<L>,
) -> io::Result<Vec<u8>> {
    let length: u64 = read_length(r)?.into();

    // Validate length before allocation to prevent OOM attacks
    if length > MAX_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "byte array length {} exceeds maximum allowed {}",
                length, MAX_BYTES
            ),
        ));
    }

    let buf_size: usize = length.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "length too large for buffer allocation",
        )
    })?;
    let mut buf = vec![0u8; buf_size];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Reads a length-prefixed byte array where the length prefix is a `u8`.
///
/// The returned `Vec<u8>` contains exactly the number of bytes specified by the prefix.
///
/// # Examples
///
/// ```
/// use std::io::Cursor;
/// let mut buf = Cursor::new(vec![3u8, b'a', b'b', b'c']);
/// let v = read_bytes_u8(&mut buf).unwrap();
/// assert_eq!(v, b"abc");
/// ```
pub fn read_bytes_u8(r: &mut dyn Read) -> io::Result<Vec<u8>> {
    read_bytes(r, |r| read_u8(r))
}

/// Reads a length-prefixed byte array where the length is encoded as a 32-bit little-endian integer.

///

/// The function first reads a 32-bit little-endian unsigned length, validates it against the module's

/// maximum allowed size, then reads and returns exactly that many bytes.

///

/// # Returns

///

/// A `Vec<u8>` containing the bytes whose length was specified by the 32-bit little-endian prefix.

///

/// # Errors

///

/// Returns an `io::Error` if reading the length or the bytes fails, or if the declared length exceeds the

/// allowed maximum or cannot be represented for allocation.

///

/// # Examples

///

/// ```

/// use std::io::Cursor;

/// // length = 3 (little-endian), data = [1, 2, 3]

/// let mut buf = Cursor::new([3u8, 0, 0, 0, 1, 2, 3].to_vec());

/// let bytes = nethernet::protocol::types::primitives::read_bytes_u32(&mut buf).unwrap();

/// assert_eq!(bytes, vec![1, 2, 3]);

/// ```
pub fn read_bytes_u32(r: &mut dyn Read) -> io::Result<Vec<u8>> {
    use super::U32LE;
    let length = U32LE::read(r)?.0;

    // Length already consumed by U32LE::read above, so the closure passed to read_bytes
    // intentionally ignores its reader parameter and returns the pre-read length.
    read_bytes(r, |_| Ok(length))
}

/// Writes `data` prefixed by its length using the provided `write_length` closure.
///
/// The function checks that `data.len()` does not exceed `MAX_BYTES`, converts the
/// length to the target integer type `L`, calls `write_length` to emit the length,
/// then writes the raw bytes.
///
/// # Examples
///
/// ```
/// use std::io::Write;
/// let mut buf = Vec::new();
/// let data = b"hi";
/// // write length as a single u8 followed by the bytes
/// write_bytes(&mut buf, data, |w, len: u8| w.write_all(&[len])).unwrap();
/// assert_eq!(buf, vec![2, b'h', b'i']);
/// ```
pub fn write_bytes<L: TryFrom<usize>>(
    w: &mut dyn Write,
    data: &[u8],
    write_length: impl Fn(&mut dyn Write, L) -> io::Result<()>,
) -> io::Result<()>
where
    <L as TryFrom<usize>>::Error: std::fmt::Debug,
{
    if data.len() > MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "data length {} exceeds maximum allowed {}",
                data.len(),
                MAX_BYTES
            ),
        ));
    }
    let length = L::try_from(data.len()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("length conversion failed: {:?}", e),
        )
    })?;
    write_length(w, length)?;
    w.write_all(data)?;
    Ok(())
}

/// Writes a length-prefixed byte array using a single-byte (u8) length prefix.
///
/// Errors if the data length exceeds MAX_BYTES, if the length cannot be represented as a `u8`,
/// or if an underlying write operation fails.
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_bytes_u8(&mut buf, b"hello").unwrap();
/// let read = read_bytes_u8(&mut &buf[..]).unwrap();
/// assert_eq!(&read[..], b"hello");
/// ```
pub fn write_bytes_u8(w: &mut dyn Write, data: &[u8]) -> io::Result<()> {
    write_bytes(w, data, |w, len: u8| write_u8(w, len))
}

/// Writes a little-endian u32 length prefix followed by the provided byte slice.
///
/// The function encodes the length of `data` as a 32-bit little-endian integer,
/// writes that length, then writes the raw bytes from `data`.
///
/// # Returns
///
/// `Ok(())` on success; `Err` if writing fails or if `data`'s length cannot be encoded
/// as a u32 or exceeds the allowed maximum.
///
/// # Examples
///
/// ```
/// let mut buf = Vec::new();
/// write_bytes_u32(&mut buf, &[1, 2, 3]).unwrap();
/// // length 3 as little-endian u32 followed by the bytes
/// assert_eq!(buf, vec![3, 0, 0, 0, 1, 2, 3]);
/// ```
pub fn write_bytes_u32(w: &mut dyn Write, data: &[u8]) -> io::Result<()> {
    use super::U32LE;
    write_bytes(w, data, |w, len: u32| U32LE(len).write(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn u8_roundtrip() {
        let value = 0x42u8;
        let mut buf = Vec::new();
        write_u8(&mut buf, value).unwrap();
        let mut cursor = Cursor::new(buf);
        let decoded = read_u8(&mut cursor).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn bytes_u8_roundtrip() {
        let data = vec![1, 2, 3, 4, 5];
        let mut buf = Vec::new();
        write_bytes_u8(&mut buf, &data).unwrap();
        let mut cursor = Cursor::new(buf);
        let decoded = read_bytes_u8(&mut cursor).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn bytes_u32_roundtrip() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut buf = Vec::new();
        write_bytes_u32(&mut buf, &data).unwrap();
        let mut cursor = Cursor::new(buf);
        let decoded = read_bytes_u32(&mut cursor).unwrap();
        assert_eq!(decoded, data);
    }
}