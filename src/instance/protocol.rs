//! What travels over the instance socket. Every message is a frame: one
//! byte saying what it holds, four bytes of big-endian length, then the
//! payload, which is JSON.
//!
//! One connection carries one request: the newly opened ShellRS sends it
//! as the first frame, the running one answers with one frame and closes.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::link::OpenLink;

/// Frames larger than this are refused, so a confused peer cannot make the
/// other side allocate without bound.
const MAX_FRAME: u32 = 1024 * 1024;

/// The byte every frame starts with.
const JSON: u8 = b'J';

pub fn write_json(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let payload = serde_json::to_vec(message).map_err(io::Error::other)?;
    let length = u32::try_from(payload.len())
        .ok()
        .filter(|length| *length <= MAX_FRAME)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    let mut header = [0; 5];
    header[0] = JSON;
    header[1..].copy_from_slice(&length.to_be_bytes());
    writer.write_all(&header)?;
    writer.write_all(&payload)?;
    writer.flush()
}

/// The next frame's payload, or `None` when the peer closed the connection
/// cleanly between frames.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut tag = [0; 1];
    match reader.read_exact(&mut tag) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    if tag[0] != JSON {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown frame"));
    }
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length);
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0; length as usize];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

pub fn parse_json<T: DeserializeOwned>(payload: &[u8]) -> io::Result<T> {
    serde_json::from_slice(payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// ShellRS was opened again: come forward, and open the link it was
    /// opened with.
    Activate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        open: Option<OpenLink>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    /// The app heard [`Request::Activate`].
    Activated,
    /// The request was not one the app reads.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_round_trips_with_and_without_a_link() {
        for request in [
            Request::Activate { open: None },
            Request::Activate {
                open: Some(OpenLink {
                    url: "ssh://token@10.0.0.9:2222".into(),
                    tab: Some("堡垒机".into()),
                }),
            },
        ] {
            let mut wire = Vec::new();
            write_json(&mut wire, &request).unwrap();
            let payload = read_frame(&mut wire.as_slice()).unwrap().unwrap();
            assert_eq!(parse_json::<Request>(&payload).unwrap(), request);
        }
        assert_eq!(read_frame(&mut [].as_slice()).unwrap(), None);
    }

    #[test]
    fn a_truncated_or_foreign_frame_is_an_error() {
        let mut wire = Vec::new();
        write_json(&mut wire, &Reply::Activated).unwrap();
        wire.pop();
        assert!(read_frame(&mut wire.as_slice()).is_err());
        assert!(read_frame(&mut b"X\0\0\0\0".as_slice()).is_err());
        assert!(read_frame(&mut b"J\xff\xff\xff\xff".as_slice()).is_err());
    }
}
