//! Receives live streams from OBS (or any RTMP app, ffmpeg included), like
//! Owncast and PeerTube: `rtmp://domain:1935/live` with the user's stream
//! key. Only publishing is understood; the audio and video are given to
//! ffmpeg as FLV (see `live`), which makes the HLS watched in browsers.
//!
//! RTMP in short: a handshake, then messages cut into chunks of
//! `chunk_size` bytes, each chunk saying which message stream (`csid`) it
//! continues. Commands are AMF0 (see `amf`).

use std::collections::HashMap;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
};

use crate::{
    AppState,
    av::{
        amf::{self, Amf},
        live,
    },
};

const HANDSHAKE_SIZE: usize = 1536;
/// What we cut our messages into, told to the client.
const OUT_CHUNK_SIZE: usize = 4096;
/// Larger messages are refused: a frame of video is far smaller.
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
/// What the client acknowledges, and how much it can send before.
const WINDOW_SIZE: u32 = 2_500_000;

// Message types.
const SET_CHUNK_SIZE: u8 = 1;
const ACKNOWLEDGEMENT: u8 = 3;
const USER_CONTROL: u8 = 4;
const WINDOW_ACK_SIZE: u8 = 5;
const SET_PEER_BANDWIDTH: u8 = 6;
const AUDIO: u8 = 8;
const VIDEO: u8 = 9;
const DATA: u8 = 18;
const COMMAND: u8 = 20;

/// Listens for OBS, until the server stops.
pub async fn serve(address: String, state: AppState) {
    let listener = match TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!("live streams: failed to listen on rtmp://{}: {}", address, err);
            return;
        }
    };
    tracing::info!("live streams: OBS sends to rtmp://{}/live", address);
    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(err) => {
                tracing::warn!("live streams: failed to accept: {}", err);
                continue;
            }
        };
        let _ = socket.set_nodelay(true);
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = Connection::new(socket).run(&state).await {
                tracing::info!("live streams: connection from {} closed: {}", peer, err);
            }
        });
    }
}

/// A message being received on a chunk stream, and what the next chunks
/// of this stream leave out of their header.
#[derive(Default)]
struct ChunkStream {
    timestamp: u32,
    delta: u32,
    length: usize,
    kind: u8,
    stream_id: u32,
    extended: bool,
    payload: Vec<u8>,
}

struct Message {
    kind: u8,
    timestamp: u32,
    payload: Vec<u8>,
}

struct Connection {
    reader: BufReader<ReadHalf<TcpStream>>,
    writer: WriteHalf<TcpStream>,
    chunk_size: usize,
    streams: HashMap<u32, ChunkStream>,
    /// Bytes received, for acknowledgements.
    received: u64,
    acknowledged: u64,
    client_window: u64,
    /// Set once `publish` gave a valid key.
    session: Option<live::Session>,
}

impl Connection {
    fn new(socket: TcpStream) -> Self {
        let (reader, writer) = tokio::io::split(socket);
        Self {
            reader: BufReader::with_capacity(64 * 1024, reader),
            writer,
            chunk_size: 128,
            streams: HashMap::new(),
            received: 0,
            acknowledged: 0,
            client_window: 0,
            session: None,
        }
    }

    async fn run(mut self, state: &AppState) -> Result<(), String> {
        let result = self.receive(state).await;
        // However it ended, the stream ends: ffmpeg finishes the HLS.
        if let Some(session) = self.session.take() {
            session.end(state).await;
        }
        result
    }

    async fn receive(&mut self, state: &AppState) -> Result<(), String> {
        self.handshake().await?;
        loop {
            let message = self.read_message().await?;
            match message.kind {
                SET_CHUNK_SIZE => {
                    let size = read_u32(&message.payload)? & 0x7FFF_FFFF;
                    if size == 0 || size as usize > MAX_MESSAGE_SIZE {
                        return Err(format!("chunk size {size} refused"));
                    }
                    self.chunk_size = size as usize;
                }
                WINDOW_ACK_SIZE => self.client_window = u64::from(read_u32(&message.payload)?),
                COMMAND => {
                    let values = amf::decode_all(&message.payload)?;
                    if !self.command(state, &values).await? {
                        return Ok(());
                    }
                }
                AUDIO | VIDEO | DATA => {
                    if let Some(session) = &mut self.session {
                        session.write_tag(message.kind, message.timestamp, &message.payload).await?;
                    }
                }
                _ => {}
            }
            self.acknowledge().await?;
        }
    }

    /// The simple handshake: the server's random bytes, then the client's
    /// echoed. OBS and ffmpeg accept it.
    async fn handshake(&mut self) -> Result<(), String> {
        let mut c0c1 = vec![0u8; 1 + HANDSHAKE_SIZE];
        self.read_exact(&mut c0c1).await?;
        if c0c1[0] != 3 {
            return Err(format!("RTMP version {} refused", c0c1[0]));
        }
        let mut s0s1s2 = Vec::with_capacity(1 + 2 * HANDSHAKE_SIZE);
        s0s1s2.push(3);
        // S1: time, zeros, then random bytes.
        s0s1s2.extend([0u8; 8]);
        s0s1s2.extend((0..HANDSHAKE_SIZE - 8).map(|index| (index * 7 + 13) as u8 ^ rand_byte()));
        // S2: C1 echoed.
        s0s1s2.extend(&c0c1[1..]);
        self.write_all(&s0s1s2).await?;
        let mut c2 = vec![0u8; HANDSHAKE_SIZE];
        self.read_exact(&mut c2).await
    }

    async fn read_exact(&mut self, buffer: &mut [u8]) -> Result<(), String> {
        self.reader.read_exact(buffer).await.map_err(|err| format!("read: {err}"))?;
        self.received += buffer.len() as u64;
        Ok(())
    }

    async fn read_u8(&mut self) -> Result<u8, String> {
        let mut byte = [0u8; 1];
        self.read_exact(&mut byte).await?;
        Ok(byte[0])
    }

    async fn read_uint(&mut self, size: usize) -> Result<u32, String> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes[4 - size..]).await?;
        Ok(u32::from_be_bytes(bytes))
    }

    /// Chunks until a message is whole.
    async fn read_message(&mut self) -> Result<Message, String> {
        loop {
            let first = self.read_u8().await?;
            let format = first >> 6;
            let csid = match u32::from(first & 0x3F) {
                0 => 64 + u32::from(self.read_u8().await?),
                1 => {
                    let low = u32::from(self.read_u8().await?);
                    64 + low + 256 * u32::from(self.read_u8().await?)
                }
                csid => csid,
            };
            let mut stream = self.streams.remove(&csid).unwrap_or_default();
            if format < 3 {
                let time = self.read_uint(3).await?;
                if format < 2 {
                    stream.length = self.read_uint(3).await? as usize;
                    stream.kind = self.read_u8().await?;
                    if stream.length > MAX_MESSAGE_SIZE {
                        return Err(format!("message of {} bytes refused", stream.length));
                    }
                }
                if format == 0 {
                    let mut id = [0u8; 4];
                    self.read_exact(&mut id).await?;
                    stream.stream_id = u32::from_le_bytes(id);
                }
                stream.extended = time == 0xFF_FFFF;
                let time = if stream.extended { self.read_uint(4).await? } else { time };
                if format == 0 {
                    stream.timestamp = time;
                    stream.delta = 0;
                } else {
                    stream.delta = time;
                    stream.timestamp = stream.timestamp.wrapping_add(time);
                }
            } else {
                // Same header as before: an extended time is repeated.
                if stream.extended {
                    self.read_uint(4).await?;
                }
                // A new message with the same delta.
                if stream.payload.is_empty() {
                    stream.timestamp = stream.timestamp.wrapping_add(stream.delta);
                }
            }
            let size = self.chunk_size.min(stream.length - stream.payload.len());
            let start = stream.payload.len();
            stream.payload.resize(start + size, 0);
            let mut chunk = std::mem::take(&mut stream.payload);
            let read = self.read_exact(&mut chunk[start..]).await;
            stream.payload = chunk;
            read?;
            if stream.payload.len() == stream.length {
                let message = Message {
                    kind: stream.kind,
                    timestamp: stream.timestamp,
                    payload: std::mem::take(&mut stream.payload),
                };
                self.streams.insert(csid, stream);
                return Ok(message);
            }
            self.streams.insert(csid, stream);
        }
    }

    async fn acknowledge(&mut self) -> Result<(), String> {
        if self.client_window > 0 && self.received - self.acknowledged >= self.client_window {
            self.acknowledged = self.received;
            self.send(2, ACKNOWLEDGEMENT, 0, &(self.received as u32).to_be_bytes()).await?;
        }
        Ok(())
    }

    /// Answers a command. `false` ends the connection.
    async fn command(&mut self, state: &AppState, values: &[Amf]) -> Result<bool, String> {
        let name = values.first().and_then(Amf::as_str).unwrap_or_default();
        let transaction = values.get(1).and_then(Amf::as_number).unwrap_or(0.0);
        match name {
            "connect" => {
                self.send(2, WINDOW_ACK_SIZE, 0, &WINDOW_SIZE.to_be_bytes()).await?;
                let mut bandwidth = WINDOW_SIZE.to_be_bytes().to_vec();
                bandwidth.push(2);
                self.send(2, SET_PEER_BANDWIDTH, 0, &bandwidth).await?;
                self.send(2, SET_CHUNK_SIZE, 0, &(OUT_CHUNK_SIZE as u32).to_be_bytes()).await?;
                self.send_command(&[
                    amf::text("_result"),
                    Amf::Number(transaction),
                    amf::object(&[("fmsVer", amf::text("FMS/3,0,1,123")), ("capabilities", Amf::Number(31.0))]),
                    amf::object(&[
                        ("level", amf::text("status")),
                        ("code", amf::text("NetConnection.Connect.Success")),
                        ("description", amf::text("Connection succeeded.")),
                        ("objectEncoding", Amf::Number(0.0)),
                    ]),
                ], 0)
                .await?;
            }
            "createStream" => {
                self.send_command(&[amf::text("_result"), Amf::Number(transaction), Amf::Null, Amf::Number(1.0)], 0).await?;
            }
            "releaseStream" | "FCPublish" => {
                self.send_command(&[amf::text("_result"), Amf::Number(transaction), Amf::Null, Amf::Undefined], 0).await?;
            }
            "publish" => {
                // `publish(transaction, null, key, "live")`. Some apps put
                // the key after `live/` or add `?query`.
                let key = values.get(3).and_then(Amf::as_str).unwrap_or_default();
                let key = key.split('?').next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
                match live::Session::start(state, key).await {
                    Ok(session) => {
                        tracing::info!("live streams: {} is live", session.username);
                        self.session = Some(session);
                        // Stream Begin, then the status OBS waits for.
                        self.send(2, USER_CONTROL, 0, &[0, 0, 0, 0, 0, 1]).await?;
                        self.send_status("status", "NetStream.Publish.Start", "Live.").await?;
                    }
                    Err(reason) => {
                        tracing::info!("live streams: refused a stream: {}", reason);
                        self.send_status("error", "NetStream.Publish.BadName", &reason).await?;
                        return Ok(false);
                    }
                }
            }
            "FCUnpublish" | "deleteStream" | "closeStream" => return Ok(false),
            _ => {}
        }
        Ok(true)
    }

    async fn send_status(&mut self, level: &str, code: &str, description: &str) -> Result<(), String> {
        self.send_command(&[
            amf::text("onStatus"),
            Amf::Number(0.0),
            Amf::Null,
            amf::object(&[("level", amf::text(level)), ("code", amf::text(code)), ("description", amf::text(description))]),
        ], 1)
        .await
    }

    async fn send_command(&mut self, values: &[Amf], stream_id: u32) -> Result<(), String> {
        self.send(3, COMMAND, stream_id, &amf::encode(values)).await
    }

    /// A message in chunks: a full header, then the short header (3).
    async fn send(&mut self, csid: u8, kind: u8, stream_id: u32, payload: &[u8]) -> Result<(), String> {
        let mut data = Vec::with_capacity(payload.len() + 16);
        data.push(csid & 0x3F);
        data.extend([0, 0, 0]);
        data.extend(&(payload.len() as u32).to_be_bytes()[1..]);
        data.push(kind);
        data.extend(stream_id.to_le_bytes());
        for (index, chunk) in payload.chunks(OUT_CHUNK_SIZE).enumerate() {
            if index > 0 {
                data.push(0xC0 | (csid & 0x3F));
            }
            data.extend(chunk);
        }
        self.write_all(&data).await
    }

    async fn write_all(&mut self, data: &[u8]) -> Result<(), String> {
        self.writer.write_all(data).await.map_err(|err| format!("write: {err}"))?;
        self.writer.flush().await.map_err(|err| format!("write: {err}"))
    }
}

fn read_u32(payload: &[u8]) -> Result<u32, String> {
    payload
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| "message too short".to_string())
}

/// Not secret, only to vary the handshake.
fn rand_byte() -> u8 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.subsec_nanos() as u8)
        .unwrap_or(7)
}
