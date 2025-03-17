use crate::SifliTool;
use std::cmp::PartialEq;
use std::io::{Error, Write};
use std::str::FromStr;
use strum::{Display, EnumString};

#[derive(EnumString, Display, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    #[strum(to_string = "burn_erase_all 0x{address:08x}\r")]
    EraseAll { address: u32 },

    #[strum(to_string = "burn_verify 0x{address:08x} 0x{len:08x} 0x{crc:08x}\r")]
    Verify { address: u32, len: u32, crc: u32 },

    #[strum(to_string = "burn_erase_write 0x{address:08x} 0x{len:08x}\r")]
    WriteAndErase { address: u32, len: u32 },

    #[strum(to_string = "burn_write 0x{address:08x} 0x{len:08x}\r")]
    Write { address: u32, len: u32 },

    #[strum(to_string = "burn_reset\r")]
    SoftReset,

    #[strum(to_string = "burn_speed {baud} {delay}\r")]
    SetBaud { baud: u32, delay: u32 },
}

#[derive(EnumString, Display, Debug, Clone, PartialEq, Eq)]
pub enum Response {
    #[strum(serialize = "OK")]
    Ok,
    #[strum(serialize = "Fail")]
    Fail,
    #[strum(serialize = "RX_WAIT")]
    RxWait,
}

const RESPONSE_STR_TABLE: [&str; 3] = ["OK", "Fail", "RX_WAIT"];

pub trait RamCommand {
    fn command(&mut self, cmd: Command) -> Result<Response, std::io::Error>;
    fn send_data(&mut self, data: &[u8]) -> Result<Response, std::io::Error>;
}

const TIMEOUT: u128 = 4000; //ms

impl RamCommand for SifliTool {
    fn command(&mut self, cmd: Command) -> Result<Response, std::io::Error> {
        self.port.write_all(cmd.to_string().as_bytes())?;
        self.port.flush()?;
        self.port.clear(serialport::ClearBuffer::All)?;

        let timeout = match cmd {
            Command::EraseAll { .. } => 30 * 1000,
            _ => TIMEOUT,
        };

        match cmd {
            Command::SetBaud { .. } => {
                return Ok(Response::Ok);
            }
            _ => {}
        }

        let mut buffer = Vec::new();
        let now = std::time::SystemTime::now();
        loop {
            let elapsed = now.elapsed().unwrap().as_millis();
            if elapsed > timeout {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timeout"));
            }

            let mut byte = [0];
            let ret = self.port.read_exact(&mut byte);
            if ret.is_err() {
                continue;
            }
            buffer.push(byte[0]);

            for response_str in RESPONSE_STR_TABLE.iter() {
                let response_bytes = response_str.as_bytes();
                // 对比buffer和response_bytes，如果buffer中包含response_str，就认为接收完毕
                // 不需要转成字符串，直接对比字节
                let exists = buffer
                    .windows(response_bytes.len())
                    .any(|window| window == response_bytes);
                if exists {
                    return Response::from_str(response_str).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                    });
                }
            }
        }
    }

    fn send_data(&mut self, data: &[u8]) -> Result<Response, Error> {
        if !self.base.compat {
            self.port.write_all(data)?;
            self.port.flush()?;
        } else {
            // 每次只发256字节
            for chunk in data.chunks(256) {
                self.port.write_all(chunk)?;
                self.port.flush()?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        let mut buffer = Vec::new();
        let now = std::time::SystemTime::now();
        loop {
            let elapsed = now.elapsed().unwrap().as_millis();
            if elapsed > TIMEOUT {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timeout"));
            }

            let mut byte = [0];
            let ret = self.port.read_exact(&mut byte);
            if ret.is_err() {
                continue;
            }
            buffer.push(byte[0]);

            // 一旦buffer出现RESPONSE_STR_TABLE中的任意一个，不一定是结束字节，也可能是在buffer中间出现，就认为接收完毕
            for response_str in RESPONSE_STR_TABLE.iter() {
                let response_bytes = response_str.as_bytes();
                let exists = buffer
                    .windows(response_bytes.len())
                    .any(|window| window == response_bytes);
                if exists {
                    return Response::from_str(response_str).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                    });
                }
            }
        }
    }
}
