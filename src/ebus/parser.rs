use std::{collections::VecDeque, fmt::{self, Display, Formatter, UpperHex}};

use crate::log::*;

use super::crc8::update_crc;


const SYN: u8 = 0xAA;
const ACK: u8 = 0x00;
const NACK: u8 = 0xFF;
const BROADCAST: u8 = 0xFE;


enum EbusParserState {
    WaitingForSYN,
    WaitingForSrc,
    WaitingForDest,
    WaitingForPB,
    WaitingForSB,
    WaitingForLen,
    WaitingForData,
    WaitingForCRC,
    WaitingForACK,
    WaitingForResponse
}

#[repr(u8)]
enum EnhProtoRequest {
    Init = 0,
    Send = 1,
    Start = 2,
    Info = 3
}

#[repr(u8)]
enum EnhProtoResponse {
    Resetted = 0,
    Received = 1,
    Started = 2,
    Info = 3,
    Failed = 0x0a,
    ErrorEbus = 0x0b,
    ErrorHost = 0x0c
}

#[repr(u8)]
enum EnhProtoErrors {
    ErrorFraming = 0x00,
    ErrorBuffOverrun = 0x01,
}

pub struct EbusRequest {
    src : u8,
    dest : u8,
    pbsb: u16,
    len: u8,
    data: Vec<u8>,
    crc: u8
}
pub struct EbusResponse {
    len: u8,
    data: Vec<u8>,
    crc: u8
}

impl EbusRequest {
    fn clear(&mut self) {
        self.src = 0;
        self.dest = 0;
        self.pbsb = 0;
        self.len = 0;
        self.data.clear();
        self.crc = 0;
    }

    fn calc_crc8(&self) -> u8 {
        let mut crc: u8 = 0;
        crc = update_crc(crc, self.src);
        crc = update_crc(crc, self.dest);
        crc = update_crc(crc, (self.pbsb >> 8) as u8);
        crc = update_crc(crc, (self.pbsb & 0xFF) as u8);
        crc = update_crc(crc, self.len);        
        for b in &self.data {
            crc = update_crc(crc, *b);
        }
        crc
    }

    pub fn src(&self) -> u8 {
        self.src
    }
    pub fn src_hex(&self) -> String {
        format!("{:X?}", self.src)
    }
    pub fn dest(&self) -> u8 {
        self.dest
    }
    pub fn dest_hex(&self) -> String {
        format!("{:X?}", self.dest)
    }
    pub fn pbsb(&self) -> u16 {
        self.pbsb
    }
    pub fn pbsb_hex(&self) -> String {
        format!("{:X?}", self.pbsb)
    }
    pub fn len(&self) -> u8 {
        self.len
    }
    pub fn len_hex(&self) -> String {
        format!("{:X?}", self.len)
    }
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }
    pub fn data_hex(&self) -> String {
        hex::encode_upper(&self.data)
    }
}

impl EbusResponse {
    fn clear(&mut self) {
        self.len = 0;
        self.data.clear();
        self.crc = 0;
    }

    fn calc_crc8(&self) -> u8 {
        let mut crc: u8 = 0;
        crc = update_crc(crc, self.len);
        for b in &self.data {
            crc = update_crc(crc, *b);
        }
        crc
    }

    pub fn len(&self) -> u8 {
        self.len
    }
    pub fn len_hex(&self) -> String {
        format!("{:X?}", self.len)
    }
    pub fn data(&self) -> &Vec<u8> {
        &self.data
    }
    pub fn data_hex(&self) -> String {
        hex::encode_upper(&self.data)
    }

}

impl Display for EbusRequest {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Req: [src: {:02X}, dest: {:02X}, pbsb: {:04X}, len: {:02X}, data: {:02X?}, crc: {:02X}]", 
            self.src, self.dest, self.pbsb, self.len, self.data, self.crc)
    }
}

impl Display for EbusResponse {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Resp: [len: {:02X}, data: {:02X?}, crc: {:02X}]", self.len, self.data, self.crc)
    }
}

enum EbusData {
    EnhancedProtocol(u8, u8),
    PureByte(u8)
}

impl Display for EbusData {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            EbusData::EnhancedProtocol(cmd, data) => write!(f, "e(c{:02X}, d{:02X})", cmd, data),
            EbusData::PureByte(data) => write!(f, "b({:02X}) ", data)
        }
    }
}

pub type EbusCallback = dyn FnMut(&EbusRequest, Option<&EbusResponse>);

pub struct EbusParser {
    state: EbusParserState,
    request: EbusRequest,
    response: EbusResponse,
    incoming_data_len: i8,
    incoming: VecDeque<u8>,
    buffer: VecDeque<EbusData>,
    got_response: bool,
    ack_received: bool,
    got_broadcast: bool,
    callback: Box<EbusCallback>,
}

// function to decode enhanced protocol data from ebus interface
// if receivced byte is >= 0x80 then it is should be decoded into 1 byte as follows: 
// byte = (byte1 - 0xc0) << 6 + (byte2 - 0x80)
fn decode_enhproto_tuple(b1:u8, b2:u8) -> (u8, u8) {
    let data = ((b1 - 0xc0) << 6) + (b2 - 0x80);
    let cmd: u8 = (b1-0xc0) >> 2;
    (cmd, data)
}


impl EbusParser {
    pub fn new(cb : impl FnMut(&EbusRequest, Option<&EbusResponse>) + 'static) -> EbusParser {
        EbusParser {
            state: EbusParserState::WaitingForSYN,
            request: EbusRequest {
                src: 0,
                dest: 0,
                pbsb: 0,
                len: 0,
                data: Vec::new(),
                crc: 0
            },
            response: EbusResponse {
                len: 0,
                data: Vec::new(),
                crc: 0
            },
            incoming: VecDeque::new(),
            buffer: VecDeque::new(),
            incoming_data_len: 0,
            got_response: false,
            ack_received: false,
            got_broadcast: false,
            // callback: Box::new(move |_,_| { cb() })
            callback: Box::new(cb)
        }
    }

    fn clear(&mut self) {
        self.state = EbusParserState::WaitingForSYN;
        self.request.clear();
        self.response.clear();
        self.incoming.clear();
        self.buffer.clear();
        self.incoming_data_len = 0;
        self.got_response = false;
        self.ack_received = false;
        self.got_broadcast = false;
    }

    pub fn feed(&mut self, data: &[u8], len: usize) {
        for i in 0..len {
            self.incoming.push_back(data[i]);
        }
        if self.incoming.len() > 64 {
            self.parse_incoming_data();
        }
    }

    fn parse_incoming_data(&mut self) {
        logD(format!("\n\nIncoming: {:X?}", self.incoming));

        // process incoming data loop
        loop {
            // pop first byte 
            let b1 = match self.incoming.pop_front() {
                Some(b) => b,
                None => break
            };
            if (b1 & 0xC0) == 0xC0 {
                // pop next byte
                let b2 = match self.incoming.pop_front() {
                    Some(b) => b,
                    None => break
                };
                if (b2 & 0x80) == 0x80 {
                    let (cmd, data) = decode_enhproto_tuple(b1,b2);
                    let cmd_e = unsafe { std::mem::transmute::<u8, EnhProtoResponse>(cmd) };
                    match cmd_e  {
                        EnhProtoResponse::Resetted => logln(LogLevel::Debug, " -= Comm resetted. =- ".to_string()),
                        EnhProtoResponse::Received => { self.buffer.push_back(EbusData::EnhancedProtocol(cmd, data)); }
                        EnhProtoResponse::Started => logln(LogLevel::Debug, "Arbitration started. ".to_string()),
                        EnhProtoResponse::Info => logln(LogLevel::Debug, "Info arrived. ".to_string()),
                        EnhProtoResponse::Failed => logln(LogLevel::Debug, "Failed. ".to_string()),
                        EnhProtoResponse::ErrorEbus => logln(LogLevel::Debug,"Comm error ebus. ".to_string()),
                        EnhProtoResponse::ErrorHost => logln(LogLevel::Debug,"Comm error host. ".to_string()),
                    }
                } else {
                    logln(LogLevel::Debug,"EnhProto ERROR!".to_string());
                }
            } else {
                self.buffer.push_back(EbusData::PureByte(b1));
            }
        }
        self.parse_protocol_buffer();
    }

    fn parse_protocol_buffer(&mut self) {
        logDln(format!("\nparse_protocol_buffer, buffer len: {}\n", self.buffer.len()));
        loop {
            // pop first element from buffer
            let b = match self.buffer.pop_front() {
                Some(b) => b,
                None => break
            };
            // deencapsulate data byte
            let byte = match b {
                EbusData::PureByte(b) => b,
                EbusData::EnhancedProtocol(_cmd, data) => data
            };
            logD(format!("({:02x})", byte));
        
            match &self.state {
                EbusParserState::WaitingForSYN => {
                    // print!("WS ");
                    if byte == SYN {
                        self.state = EbusParserState::WaitingForSrc;
                    }
                }
                EbusParserState::WaitingForSrc => {
                    // print!("W1");
                    if byte != SYN {
                        // print!("GS ");
                        self.request.src = byte;
                        self.state = EbusParserState::WaitingForDest;
                    }
                }
                EbusParserState::WaitingForDest => {
                    // print!("GD ");
                    self.request.dest = byte;
                    if self.request.dest == BROADCAST {
                        self.got_broadcast = true;
                    }
                    self.state = EbusParserState::WaitingForPB;
                }
                EbusParserState::WaitingForPB => {
                    // print!("PB ");
                    self.request.pbsb = (byte as u16) << 8;
                    self.state = EbusParserState::WaitingForSB;
                }
                EbusParserState::WaitingForSB => {
                    // print!("SB ");
                    self.request.pbsb |= byte as u16;
                    self.state = EbusParserState::WaitingForLen;
                }
                EbusParserState::WaitingForLen => {
                    // print!("LN ");
                    if byte > 0x10 {
                        // errorneous data - LEN cannot exceed 16 bytes, drop this frame and wait for next one
                        self.clear()
                    } else {
                        if self.got_response {
                            self.response.len = byte;                    
                        } else {
                            self.request.len = byte;
                        }
                        self.incoming_data_len = byte as i8;   
                        self.state = EbusParserState::WaitingForData;
                    }
                }
                EbusParserState::WaitingForData => {
                    // print!("GD ");
                    if self.got_response {
                        self.response.data.push(byte);
                        self.incoming_data_len -= 1;
                        if self.incoming_data_len == 0 {
                            self.state = EbusParserState::WaitingForCRC;
                        } 
                    } else {
                        self.request.data.push(byte);
                        self.incoming_data_len -= 1;
                        if self.incoming_data_len == 0 {
                            self.state = EbusParserState::WaitingForCRC;
                        } 
                    }
                }
                EbusParserState::WaitingForCRC => {
                    // print!("CRC:");
                    if self.got_response {
                        self.response.crc = byte;
                        let crc = self.response.calc_crc8();
                        if crc == byte {
                            // print!("CRC OK");
                        } else {
                            // print!("CRC ERR");
                            // CRC error - drop this frame and wait for next one
                            self.clear();
                        }
                    } else {
                        self.request.crc = byte;
                        let crc = self.request.calc_crc8();
                        if crc == byte {
                            // print!("CRC OK");
                        } else {
                            // print!("CRC ERR");
                            // CRC error - drop this frame and wait for next one
                            self.clear();
                        }
                    }
                    self.state = EbusParserState::WaitingForACK;
                }
                EbusParserState::WaitingForACK => {
                    // print!("WA:");
                    if byte == ACK {
                        // print!("ACK");
                        // we've got ACK - probably master-slave telegram    
                        self.ack_received = true;

                        if self.got_response {
                            self.state = EbusParserState::WaitingForSYN;
                            self.process();
                        } else {
                            self.state = EbusParserState::WaitingForResponse;
                        }
                    } else if byte == NACK {
                        // print!("NACK");
                        // no ACK - devices need to retransmit, drop this frame
                        self.state = EbusParserState::WaitingForSYN;
                        self.clear();
                    } else if byte == SYN {
                        // print!("SYN");
                        // SYN received, if broadcast - request can be not ACKed
                        if self.got_broadcast {
                            self.process();
                        }
                        self.state = EbusParserState::WaitingForSrc;
                    } else {
                        // print!("ERROR");
                        // error - drop this frame and wait for next one
                        self.clear();
                    
                    }
                }
                EbusParserState::WaitingForResponse => {
                    // print!("WRS");
                    if byte == SYN {
                        // print!(":NRS ");
                        // no response - process received frame
                        self.state = EbusParserState::WaitingForSYN;
                        self.process();
                    } else {
                        // we've got response - wait for response data
                        // print!(":RS ");
                        self.got_response = true;
                        self.response.len = byte;
                        self.incoming_data_len = byte as i8;
                        self.state = EbusParserState::WaitingForData;
                    }
                }            
            }
        }
    }

    fn process(&mut self) {
        logD("\nIncoming pkt: ".to_string());
        self.process_frame();
        self.got_response = false;
        self.ack_received = false;
        self.response.clear();
        self.request.clear();
    }

    fn process_frame(&mut self) {
        logIln(format!("{}", self.request));
        if self.got_response {
            logIln(format!(" `-:> {}", self.response));
        }

        // do callback
        if self.got_response {
            (self.callback)(&self.request, Some(&self.response));    
        } else {
            (self.callback)(&self.request, None);
        }
    }
}

