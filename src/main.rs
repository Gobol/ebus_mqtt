
use std::fmt::{self, Display, Formatter};
use std::io::{Read};
use std::net::{TcpStream};
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;


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

struct EbusFrame {
    src : u8,
    dest : u8,
    pbsb: u16,
    len: u8,
    data: Vec<u8>,
    crc: u8
}
struct EbusResponse {
    len: u8,
    data: Vec<u8>,
    crc: u8
}

const CRC_LOOKUP_TABLE : [u8; 256] = [
    0x00, 0x9b, 0xad, 0x36, 0xc1, 0x5a, 0x6c, 0xf7, 0x19, 0x82, 0xb4, 0x2f, 0xd8, 0x43, 0x75, 0xee,
    0x32, 0xa9, 0x9f, 0x04, 0xf3, 0x68, 0x5e, 0xc5, 0x2b, 0xb0, 0x86, 0x1d, 0xea, 0x71, 0x47, 0xdc,
    0x64, 0xff, 0xc9, 0x52, 0xa5, 0x3e, 0x08, 0x93, 0x7d, 0xe6, 0xd0, 0x4b, 0xbc, 0x27, 0x11, 0x8a,
    0x56, 0xcd, 0xfb, 0x60, 0x97, 0x0c, 0x3a, 0xa1, 0x4f, 0xd4, 0xe2, 0x79, 0x8e, 0x15, 0x23, 0xb8,
    0xc8, 0x53, 0x65, 0xfe, 0x09, 0x92, 0xa4, 0x3f, 0xd1, 0x4a, 0x7c, 0xe7, 0x10, 0x8b, 0xbd, 0x26,
    0xfa, 0x61, 0x57, 0xcc, 0x3b, 0xa0, 0x96, 0x0d, 0xe3, 0x78, 0x4e, 0xd5, 0x22, 0xb9, 0x8f, 0x14,
    0xac, 0x37, 0x01, 0x9a, 0x6d, 0xf6, 0xc0, 0x5b, 0xb5, 0x2e, 0x18, 0x83, 0x74, 0xef, 0xd9, 0x42,
    0x9e, 0x05, 0x33, 0xa8, 0x5f, 0xc4, 0xf2, 0x69, 0x87, 0x1c, 0x2a, 0xb1, 0x46, 0xdd, 0xeb, 0x70,
    0x0b, 0x90, 0xa6, 0x3d, 0xca, 0x51, 0x67, 0xfc, 0x12, 0x89, 0xbf, 0x24, 0xd3, 0x48, 0x7e, 0xe5,
    0x39, 0xa2, 0x94, 0x0f, 0xf8, 0x63, 0x55, 0xce, 0x20, 0xbb, 0x8d, 0x16, 0xe1, 0x7a, 0x4c, 0xd7,
    0x6f, 0xf4, 0xc2, 0x59, 0xae, 0x35, 0x03, 0x98, 0x76, 0xed, 0xdb, 0x40, 0xb7, 0x2c, 0x1a, 0x81,
    0x5d, 0xc6, 0xf0, 0x6b, 0x9c, 0x07, 0x31, 0xaa, 0x44, 0xdf, 0xe9, 0x72, 0x85, 0x1e, 0x28, 0xb3,
    0xc3, 0x58, 0x6e, 0xf5, 0x02, 0x99, 0xaf, 0x34, 0xda, 0x41, 0x77, 0xec, 0x1b, 0x80, 0xb6, 0x2d,
    0xf1, 0x6a, 0x5c, 0xc7, 0x30, 0xab, 0x9d, 0x06, 0xe8, 0x73, 0x45, 0xde, 0x29, 0xb2, 0x84, 0x1f,
    0xa7, 0x3c, 0x0a, 0x91, 0x66, 0xfd, 0xcb, 0x50, 0xbe, 0x25, 0x13, 0x88, 0x7f, 0xe4, 0xd2, 0x49,
    0x95, 0x0e, 0x38, 0xa3, 0x54, 0xcf, 0xf9, 0x62, 0x8c, 0x17, 0x21, 0xba, 0x4d, 0xd6, 0xe0, 0x7b,
];

fn update_crc(crc: u8, value: u8) -> u8 {
    CRC_LOOKUP_TABLE[crc as usize] ^ value
}


impl EbusFrame {
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
}

impl Display for EbusFrame {
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

struct EbusParser {
    state: EbusParserState,
    request: EbusFrame,
    response: EbusResponse,
    incoming_data_len: i8,
    incoming: Vec<u8>,
    buffer: Vec<EbusData>,
    got_response: bool,
    ack_received: bool
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
    fn new() -> EbusParser {
        EbusParser {
            state: EbusParserState::WaitingForSYN,
            request: EbusFrame {
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
            incoming: Vec::new(),
            buffer: Vec::new(),
            incoming_data_len: 0,
            got_response: false,
            ack_received: false
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
    }

    fn feed(&mut self, data: &[u8], len: usize) {
        for i in 0..len {
            self.incoming.push(data[i]);
        }
        if self.incoming.len() > 32 {
            self.parse_enhproto();
        }
    }

    fn parse_enhproto(&mut self) {
        println!("\n\nIncoming: {:X?}", self.incoming);

        let mut i = 0;

        while i < self.incoming.len() {
            if (self.incoming[i] & 0xC0) == 0xC0 {
                if i+1 < self.incoming.len() {
                    if (self.incoming[i+1] & 0x80) == 0x80 {
                            
                        let (cmd, data) = decode_enhproto_tuple(self.incoming[i], self.incoming[i+1]);
                        let cmd_e = unsafe { std::mem::transmute::<u8, EnhProtoResponse>(cmd) };
                        match cmd_e  {
                            EnhProtoResponse::Resetted => println!(" -= Comm resetted. =- "),
                            EnhProtoResponse::Received => { self.buffer.push(EbusData::EnhancedProtocol(cmd, data)); }
                            EnhProtoResponse::Started => println!("Arbitration started. "),
                            EnhProtoResponse::Info => println!("Info arrived. "),
                            EnhProtoResponse::Failed => println!("Failed. "),
                            EnhProtoResponse::ErrorEbus => println!("Comm error ebus. "),
                            EnhProtoResponse::ErrorHost => println!("Comm error host. "),
                        }
                        i = i+2;
                    } else {
                        println!("EnhProto ERROR!");
                    }
                }
            } else {
                self.buffer.push(EbusData::PureByte(self.incoming[i]));
                i=i+1;
            }
        }
        self.parse_incoming();
        self.incoming.clear();
    }

    fn parse_incoming(&mut self) {
        for i in 0..self.buffer.len() {
            let b = &self.buffer[i];
            let byte = match b {
                EbusData::PureByte(b) => *b,
                EbusData::EnhancedProtocol(_cmd, data) => *data
            };
            print!("({:02x})", byte);
        
            match &self.state {
                EbusParserState::WaitingForSYN => {
                    print!("WS ");
                    if byte == 0xAA {
                        self.state = EbusParserState::WaitingForSrc;
                    }
                }
                EbusParserState::WaitingForSrc => {
                    print!("W1");
                    if byte != 0xAA {
                        print!("GS ");
                        self.request.src = byte;
                        self.state = EbusParserState::WaitingForDest;
                    }
                }
                EbusParserState::WaitingForDest => {
                    print!("GD ");
                    self.request.dest = byte;
                    self.state = EbusParserState::WaitingForPB;
                }
                EbusParserState::WaitingForPB => {
                    print!("PB ");
                    self.request.pbsb = (byte as u16) << 8;
                    self.state = EbusParserState::WaitingForSB;
                }
                EbusParserState::WaitingForSB => {
                    print!("SB ");
                    self.request.pbsb |= byte as u16;
                    self.state = EbusParserState::WaitingForLen;
                }
                EbusParserState::WaitingForLen => {
                    print!("LN ");
                    if byte > 0x10 {
                        // errorneous data - LEN cannot exceed 16 bytes, drop this frame and wait for next one
                        self.state = EbusParserState::WaitingForSYN;
                        self.request.clear();
                        self.response.clear();
                        self.got_response = false;
                        self.ack_received = false;
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
                    print!("GD ");
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
                    print!("CRC:");
                    if self.got_response {
                        self.response.crc = byte;
                        let crc = self.response.calc_crc8();
                        if (crc == byte) {
                            print!("CRC OK");
                        } else {
                            print!("CRC ERR");
                            // CRC error - drop this frame and wait for next one
                            self.clear();
                        }
                    } else {
                        self.request.crc = byte;
                        let crc = self.request.calc_crc8();
                        if (crc == byte) {
                            print!("CRC OK");
                        } else {
                            print!("CRC ERR");
                            // CRC error - drop this frame and wait for next one
                            self.clear();
                        }
                    }
                    self.state = EbusParserState::WaitingForACK;
                }
                EbusParserState::WaitingForACK => {
                    print!("WA:");
                    if byte == 0x00 {
                        print!("ACK");
                        // we've got ACK - probably master-slave telegram    
                        self.ack_received = true;
                    }
                    if self.got_response {
                        self.state = EbusParserState::WaitingForSYN;
                        self.process();
                    } else {
                        self.state = EbusParserState::WaitingForResponse;
                    }
                }
                EbusParserState::WaitingForResponse => {
                    print!("WRS");
                    if byte == 0xAA {
                        print!(":NRS ");
                        // no response - process received frame
                        self.state = EbusParserState::WaitingForSYN;
                        self.process();
                    } else {
                        // we've got response - wait for response data
                        print!(":RS ");
                        self.got_response = true;
                        self.state = EbusParserState::WaitingForLen;
                    }
                }
            
            }
        }
    }

    fn process(&mut self) {
        print!("\nIncoming pkt: ");
        self.process_frame();
        self.incoming.clear();
        self.incoming_data_len = 0;
        self.got_response = false;
        self.ack_received = false;
        self.response.clear();
        self.request.clear();
    }

    fn process_frame(&self) {
        println!("{}", self.request);
        if self.got_response {
            println!(" `-:> {}", self.response);
        }
    }
}


fn main() {
    // IP address and port to connect to
    let ip = "192.168.2.45";
    let port = 9999;

    // Create a TCP stream
    let mut stream = TcpStream::connect(format!("{}:{}", ip, port)).expect("Failed to connect");

    // Create a flag to indicate when to stop receiving data
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Spawn a thread to receive and print data
    let handle = thread::spawn(move || {
        let mut buffer = [0; 1024];
        let mut parser = EbusParser::new();
        while running_clone.load(Ordering::Relaxed) {
            match stream.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    parser.feed(&buffer[0..n], n);
                    // for i in 0..n {
                    //     print!("{:02X} ", buffer[i]);
                    // }
                    // println!();
                }
                Ok(_) => break,
                Err(_) => break,
            }
        }
    });

    // Wait for a keypress to stop receiving data
    let _ = std::io::stdin().read(&mut [0u8]).unwrap();

    // Set the flag to stop receiving data
    running.store(false, Ordering::Relaxed);

    // Wait for the receiving thread to finish
    let _ = handle.join();
}